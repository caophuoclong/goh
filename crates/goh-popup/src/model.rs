//! Windowing, width, truncation and accessibility text for the popup.

use serde::Deserialize;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

// Geometry from design-system/tokens.json.
pub const WIDTH_MIN: f32 = 320.0;
pub const WIDTH_MAX: f32 = 520.0;
pub const ROW_HEIGHT: f32 = 28.0;
pub const FOOTER_HEIGHT: f32 = 28.0;
pub const PAD: f32 = 8.0; // space-2
/// Advance of one monospace cell at the 13 px popup font.
pub const CELL_WIDTH: f32 = 7.8;

const MARKER_COLS: usize = 2; // "❯ "
const GAP_COLS: usize = 2;
/// A description narrower than this is dropped entirely rather than shown as a stub.
const MIN_DESCRIPTION_COLS: usize = 6;
const ELLIPSIS: &str = "…";

#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    /// UTF-8 byte ranges of `label` that matched what the user typed (from the engine).
    #[serde(default)]
    pub match_ranges: Vec<[usize; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct State {
    pub id: String,
    pub items: Vec<Item>,
    pub selected: usize,
    #[serde(default)]
    pub window_start: usize,
    #[serde(default)]
    pub context: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// `<id>-opt-<index>`, with the absolute index.
    pub dom_id: String,
    pub selected: bool,
    /// Text as drawn (ellipsised if it did not fit).
    pub label: String,
    /// Byte ranges of `label` (as drawn) to highlight; on grapheme boundaries.
    pub matches: Vec<[usize; 2]>,
    pub description: Option<String>,
    /// Full label when it was shortened, for the tooltip.
    pub tooltip: Option<String>,
    /// What a screen reader reads: always the full label, then the description.
    pub accessible_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub width: f32,
    pub height: f32,
    pub rows: Vec<Row>,
    pub context: String,
    /// `n/total`, only when there are more items than rows.
    pub position: Option<String>,
    pub listbox_id: String,
    pub active_descendant: String,
    /// Polite status text: changes only when the selection changes.
    pub status: String,
    pub window_start: usize,
}

/// Move the selection by `delta`, wrapping at both ends.
pub fn select(state: &State, delta: isize, max_rows: usize) -> State {
    let n = state.items.len();
    let mut next = state.clone();
    if n == 0 {
        return next;
    }
    next.selected = (state.selected as isize + delta).rem_euclid(n as isize) as usize;
    next.window_start = window_start(next.selected, state.window_start, max_rows.min(n), n);
    next
}

/// Smallest move of the window that keeps `selected` visible: one row at a time, and a jump only
/// when the selection wraps.
pub fn window_start(selected: usize, start: usize, rows: usize, total: usize) -> usize {
    let start = start.min(total.saturating_sub(rows));
    if selected < start {
        selected
    } else if selected >= start + rows {
        selected + 1 - rows
    } else {
        start
    }
}

fn cols(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Longest grapheme-aligned prefix of `s` that fits in `max_cols`.
fn prefix_fitting(s: &str, max_cols: usize) -> &str {
    let mut used = 0;
    let mut end = 0;
    for (i, g) in s.grapheme_indices(true) {
        let w = cols(g);
        if used + w > max_cols {
            break;
        }
        used += w;
        end = i + g.len();
    }
    &s[..end]
}

/// Snap byte ranges outward to grapheme boundaries (so a combining mark is never split from its
/// base), clamp to the label, drop empty ones and merge overlaps.
pub fn snap_ranges(label: &str, ranges: &[[usize; 2]]) -> Vec<[usize; 2]> {
    let bounds: Vec<(usize, usize)> = label.grapheme_indices(true).map(|(i, g)| (i, i + g.len())).collect();
    let mut out: Vec<[usize; 2]> = Vec::new();
    let mut sorted: Vec<[usize; 2]> = ranges.iter().copied().filter(|r| r[0] < r[1] && r[0] < label.len()).collect();
    sorted.sort();
    for [s, e] in sorted {
        let e = e.min(label.len());
        let start = bounds.iter().rev().find(|(gs, _)| *gs <= s).map(|b| b.0).unwrap_or(0);
        let end = bounds.iter().find(|(_, ge)| *ge >= e).map(|b| b.1).unwrap_or(label.len());
        match out.last_mut() {
            Some(last) if start <= last[1] => last[1] = last[1].max(end),
            _ => out.push([start, end]),
        }
    }
    out
}

/// Build what to draw. `None` means "do not draw an overlay" (nothing to show, or less than the
/// minimum width is available, in which case the caller uses the inline fallback).
pub fn view(state: &State, max_rows: usize, available_width: f32) -> Option<View> {
    let n = state.items.len();
    if n == 0 || available_width < WIDTH_MIN {
        return None;
    }
    let selected = state.selected.min(n - 1);
    let rows_visible = max_rows.max(1).min(n);
    let start = window_start(selected, state.window_start, rows_visible, n);

    // Width follows content between the min and max, never beyond the space available.
    let widest = state.items[start..start + rows_visible]
        .iter()
        .map(|it| MARKER_COLS + cols(&it.label) + it.description.as_deref().map_or(0, |d| GAP_COLS + cols(d)))
        .max()
        .unwrap_or(0);
    let wanted = 2.0 * PAD + widest as f32 * CELL_WIDTH;
    let width = wanted.clamp(WIDTH_MIN, WIDTH_MAX.min(available_width));
    let inner_cols = ((width - 2.0 * PAD) / CELL_WIDTH).floor() as usize;
    let room = inner_cols.saturating_sub(MARKER_COLS);

    let rows = (start..start + rows_visible)
        .map(|i| {
            let it = &state.items[i];
            let full = snap_ranges(&it.label, &it.match_ranges);
            let (label, description, tooltip) = if cols(&it.label) > room {
                // The label itself must be shortened; the description is already gone.
                let kept = prefix_fitting(&it.label, room.saturating_sub(cols(ELLIPSIS)));
                (format!("{kept}{ELLIPSIS}"), None, Some(it.label.clone()))
            } else {
                let desc_room = room - cols(&it.label);
                let desc = it.description.as_deref().filter(|d| !d.is_empty()).and_then(|d| {
                    let space = desc_room.checked_sub(GAP_COLS)?;
                    if space < MIN_DESCRIPTION_COLS {
                        None
                    } else if cols(d) <= space {
                        Some(d.to_string())
                    } else {
                        Some(format!("{}{ELLIPSIS}", prefix_fitting(d, space - cols(ELLIPSIS))))
                    }
                });
                (it.label.clone(), desc, None)
            };
            // Highlights only the part that is still drawn.
            let drawn_len = if tooltip.is_some() { label.len() - ELLIPSIS.len() } else { label.len() };
            let matches = full
                .iter()
                .filter(|r| r[0] < drawn_len)
                .map(|r| [r[0], r[1].min(drawn_len)])
                .collect();
            let accessible_name = match it.description.as_deref().filter(|d| !d.is_empty()) {
                Some(d) => format!("{}, {d}", it.label),
                None => it.label.clone(),
            };
            Row { dom_id: format!("{}-opt-{i}", state.id), selected: i == selected, label, matches, description, tooltip, accessible_name }
        })
        .collect::<Vec<_>>();

    let current = &state.items[selected];
    Some(View {
        width,
        height: rows.len() as f32 * ROW_HEIGHT + FOOTER_HEIGHT,
        position: (n > rows_visible).then(|| format!("{}/{n}", selected + 1)),
        listbox_id: state.id.clone(),
        active_descendant: format!("{}-opt-{selected}", state.id),
        status: format!("{}, {} of {n}", current.label, selected + 1),
        context: state.context.clone(),
        window_start: start,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str, desc: &str) -> Item {
        Item { label: label.into(), description: (!desc.is_empty()).then(|| desc.into()), match_ranges: vec![] }
    }

    fn state(n: usize, selected: usize, window_start: usize) -> State {
        State {
            id: "pop".into(),
            items: (0..n).map(|i| item(&format!("item-{i:02}"), "A description")).collect(),
            selected,
            window_start,
            context: "git · subcommands".into(),
        }
    }

    #[test]
    fn u01_twelve_results_select_eleventh() {
        // Walk down one row at a time from the top: the window moves one row per step.
        let mut s = state(12, 0, 0);
        let mut starts = vec![];
        for _ in 0..10 {
            s = select(&s, 1, 8);
            starts.push(s.window_start);
        }
        assert_eq!(s.selected, 10);
        assert_eq!(starts, [0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);
        let v = view(&s, 8, 600.0).unwrap();
        assert!(v.rows.iter().any(|r| r.selected && r.label == "item-10"), "selected row is visible");
        assert_eq!(v.position.as_deref(), Some("11/12"));
        assert_eq!(v.rows.len(), 8);
        assert_eq!(v.window_start, 3);
    }

    #[test]
    fn u02_selection_wraps_and_window_jumps() {
        let last = select(&state(12, 0, 0), -1, 8);
        assert_eq!((last.selected, last.window_start), (11, 4), "Up on first selects last; window jumps to the end");
        let first = select(&last, 1, 8);
        assert_eq!((first.selected, first.window_start), (0, 0), "Down on last selects first; window jumps to the top");
    }

    #[test]
    fn footer_position_only_when_more_items_than_rows() {
        assert_eq!(view(&state(8, 0, 0), 8, 600.0).unwrap().position, None);
        assert_eq!(view(&state(9, 0, 0), 8, 600.0).unwrap().position.as_deref(), Some("1/9"));
    }

    #[test]
    fn u03_description_shrinks_then_disappears_then_label_is_ellipsised() {
        let long = "x".repeat(70);
        let mk = |label: &str, desc: &str| State { items: vec![item(label, desc)], ..state(1, 0, 0) };
        let desc = "d".repeat(80);

        // Plenty of room for the label, not for the whole description: description is shortened.
        let v = view(&mk("checkout", &desc), 8, 330.0).unwrap();
        let r = &v.rows[0];
        assert_eq!(r.label, "checkout");
        assert!(r.description.as_deref().unwrap().ends_with('…'));
        assert!(r.tooltip.is_none());

        // Label nearly fills the row: description disappears completely, label untouched.
        let v = view(&mk(&"l".repeat(36), &desc), 8, 330.0).unwrap();
        assert_eq!(v.rows[0].description, None);
        assert_eq!(v.rows[0].label, "l".repeat(36));

        // Label does not fit at all: ellipsis, full label in tooltip and accessible name.
        let v = view(&mk(&long, "desc"), 8, 330.0).unwrap();
        let r = &v.rows[0];
        assert!(r.label.ends_with('…') && r.label.chars().count() < long.len());
        assert_eq!(r.description, None);
        assert_eq!(r.tooltip.as_deref(), Some(long.as_str()));
        assert!(r.accessible_name.starts_with(&long));
    }

    #[test]
    fn u04_less_than_minimum_width_draws_nothing() {
        assert!(view(&state(3, 0, 0), 8, 319.0).is_none());
        assert!(view(&state(3, 0, 0), 8, 320.0).is_some());
        assert!(view(&state(0, 0, 0), 8, 600.0).is_none(), "never an empty popup");
    }

    #[test]
    fn width_follows_content_within_bounds() {
        let w = |label: &str, avail| view(&State { items: vec![item(label, "")], ..state(1, 0, 0) }, 8, avail).unwrap().width;
        assert_eq!(w("a", 800.0), WIDTH_MIN);
        assert_eq!(w(&"w".repeat(200), 800.0), WIDTH_MAX);
        assert_eq!(w(&"w".repeat(200), 400.0), 400.0, "never wider than the space available");
    }

    #[test]
    fn u05_ranges_land_on_grapheme_boundaries() {
        // "e" + combining acute (3 bytes, one grapheme) then "x".
        let label = "e\u{301}x";
        assert_eq!(snap_ranges(label, &[[0, 1]]), vec![[0, 3]], "range ending inside the cluster grows to include the mark");
        assert_eq!(snap_ranges(label, &[[1, 3]]), vec![[0, 3]], "range starting inside the cluster grows back to its base");
        assert_eq!(snap_ranges(label, &[[3, 4]]), vec![[3, 4]]);
        // CJK: each character is 3 bytes and 2 columns.
        assert_eq!(snap_ranges("日本語", &[[3, 6]]), vec![[3, 6]]);
        // Mid-character offsets, out-of-range, empty and overlapping ranges are handled.
        assert_eq!(snap_ranges("日本語", &[[4, 5]]), vec![[3, 6]]);
        assert_eq!(snap_ranges("abc", &[[2, 99]]), vec![[2, 3]]);
        assert_eq!(snap_ranges("abc", &[[5, 9], [2, 2]]), Vec::<[usize; 2]>::new());
        assert_eq!(snap_ranges("abcdef", &[[0, 2], [1, 4], [4, 5]]), vec![[0, 5]]);
    }

    #[test]
    fn u05_cjk_width_is_counted_in_columns_not_characters() {
        let long = "日本語".repeat(20); // 60 chars, 120 columns
        let s = State { items: vec![item(&long, "")], ..state(1, 0, 0) };
        let r = &view(&s, 8, 330.0).unwrap().rows[0].clone();
        assert!(r.label.ends_with('…'));
        let shown = r.label.trim_end_matches('…');
        assert!(cols(shown) < (((330.0f32 - 16.0) / CELL_WIDTH) as usize) - MARKER_COLS, "fits the row in columns");
        assert!(long.starts_with(shown), "cut on a character boundary");
    }

    #[test]
    fn truncation_never_splits_a_combining_mark() {
        let label = "e\u{301}".repeat(60); // 60 columns, 120 chars
        let s = State { items: vec![item(&label, "")], ..state(1, 0, 0) };
        let r = &view(&s, 8, 330.0).unwrap().rows[0].clone();
        let shown = r.label.trim_end_matches('…');
        assert!(shown.chars().count().is_multiple_of(2) && !shown.is_empty(), "whole base+mark pairs only");
    }

    #[test]
    fn u06_only_the_items_own_matches_are_highlighted() {
        // Fuzzy: `cn` matches the c and n of "clean"; they are not a prefix.
        let mut it = item("clean", "");
        it.match_ranges = vec![[0, 1], [4, 5]];
        let s = State { items: vec![it], ..state(1, 0, 0) };
        let r = &view(&s, 8, 400.0).unwrap().rows[0];
        assert_eq!(r.matches, vec![[0, 1], [4, 5]]);
        let (a, b) = (&r.label[r.matches[0][0]..r.matches[0][1]], &r.label[r.matches[1][0]..r.matches[1][1]]);
        assert_eq!((a, b), ("c", "n"));
    }

    #[test]
    fn highlights_are_clipped_to_the_drawn_label() {
        let label = "x".repeat(70);
        let mut it = item(&label, "");
        it.match_ranges = vec![[0, 2], [60, 65]];
        let s = State { items: vec![it], ..state(1, 0, 0) };
        let r = &view(&s, 8, 330.0).unwrap().rows[0].clone();
        assert_eq!(r.matches, vec![[0, 2]], "matches beyond the ellipsis are not drawn");
    }

    #[test]
    fn u12_ids_and_status_follow_the_selection_only() {
        let s = state(12, 10, 3);
        let v = view(&s, 8, 600.0).unwrap();
        assert_eq!(v.listbox_id, "pop");
        assert_eq!(v.active_descendant, "pop-opt-10");
        assert!(v.rows.iter().any(|r| r.dom_id == "pop-opt-10" && r.selected));
        assert_eq!(v.status, "item-10, 11 of 12");

        // Same selection, different context text or window: the announcement is unchanged.
        let mut other = s.clone();
        other.context = "something else".into();
        other.window_start = 4;
        assert_eq!(view(&other, 8, 600.0).unwrap().status, v.status);
        // A selection change changes it.
        assert_ne!(view(&select(&s, 1, 8), 8, 600.0).unwrap().status, v.status);
    }

    #[test]
    fn accessible_name_includes_description() {
        let s = State { items: vec![item("checkout", "Switch branches")], ..state(1, 0, 0) };
        assert_eq!(view(&s, 8, 600.0).unwrap().rows[0].accessible_name, "checkout, Switch branches");
    }

    #[test]
    fn out_of_range_state_is_clamped_not_a_panic() {
        let s = State { selected: 99, window_start: 99, ..state(5, 0, 0) };
        let v = view(&s, 8, 600.0).unwrap();
        assert!(v.rows.iter().any(|r| r.selected));
        assert_eq!(select(&state(0, 0, 0), 1, 8).items.len(), 0);
    }
}

//! `goh-popup`: draws the suggestion popup in a small borderless window.
//!
//! Presentational (docs/07): it draws newline-delimited JSON frames at the supplied anchor. Wheel
//! navigation requests go back to the adapter; only a confirmed frame changes selection. The popup
//! never takes keyboard focus and never inserts text.
//! Whether an overlay may be drawn at all is decided by the caller with `goh_popup::display`.
//!
//!   {"type":"show","state":{...},"anchor":"auto","mode":"auto","theme":{...},"max_rows":8}
//!   {"type":"hide"}   {"type":"quit"}
//!
//! `"anchor":"auto"` asks the Accessibility API for the caret (verified apps only; see goh-anchor);
//! the display-mode gate then decides overlay or inline, and a decision line is printed on stdout.
//! `"anchor":{"x":..,"y":..,"line_height":..}` is a position vouched for by the caller (tests, demos).
//! Without a verified anchor nothing is drawn.
//!
//! Flags: `--resolve` prints the decision for the focused terminal and exits (no window);
//! `--screenshot out.png` saves the first drawn frame and exits.

#[cfg(target_os = "macos")]
mod coretext;
#[cfg(any(target_os = "macos", test))]
mod scroll;

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, vec2};
use goh_anchor::{Caret, Outcome, Rect as Screen};
use goh_popup::display::{self, Anchor as AnchorState, Capabilities, Display, Mode as DisplayMode};
use goh_popup::model::{self, View, CELL_WIDTH, FOOTER_HEIGHT, PAD, ROW_HEIGHT};
use goh_popup::font::{self, TermFont};
use goh_popup::placement::{self, Placement};
use goh_popup::watch::{plan_tick, TickAction};
use goh_theme::{Mode, Rgb, Terminal};
use serde::Deserialize;

const RADIUS_POPUP: u8 = 6;
const RADIUS_ROW: u8 = 4;
const FONT: f32 = 13.0;
const FONT_MIN: f32 = 10.0;
const FONT_MAX: f32 = 18.0;
/// How often an open menu is re-checked against the terminal's state.
const TICK: Duration = Duration::from_millis(50);

// Frames are short-lived and sent one at a time, so the size difference does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Frame {
    Show(Request),
    Hide,
    Quit,
}

/// Everything a `show` frame says; kept so the popup can re-check it while the menu is open.
#[derive(Deserialize, Clone)]
struct Request {
    state: model::State,
    anchor: Option<AnchorSpec>,
    #[serde(default)]
    mode: DisplayMode,
    #[serde(default)]
    theme: HashMap<String, String>,
    #[serde(default = "default_rows")]
    max_rows: usize,
    #[serde(default)]
    enter_accepts: bool,
    /// Only used with a manual anchor that has no `screen`.
    #[serde(default = "default_width")]
    available_width: f32,
    /// The terminal's cell height in points, when the sender knows it (iTerm2 needs it).
    #[serde(default)]
    cell_height: Option<f64>,
    /// Allow asking the terminal for its cell height on the tty. Off by default: when a shell's
    /// line editor is reading the terminal, the reply would be taken as typing.
    #[serde(default)]
    tty_query: bool,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum AnchorSpec {
    Manual(ManualAnchor),
    Keyword(String),
}

#[derive(Deserialize, Clone, Copy)]
struct ManualAnchor {
    x: f64,
    y: f64,
    #[serde(default)]
    line_height: f64,
    screen: Option<[f64; 4]>,
}

fn default_rows() -> usize {
    8
}
fn default_width() -> f32 {
    800.0
}

struct Colors {
    base: Color32,
    selected: Color32,
    border: Color32,
    primary: Color32,
    muted: Color32,
    accent: Color32,
    matched: Color32,
}

impl Colors {
    /// Frame tokens win; anything missing comes from the design system's dark theme.
    fn from(theme: &HashMap<String, String>) -> Colors {
        let fallback = goh_theme::resolve(Mode::Dark, &Terminal::default()).tokens;
        let pick = |name: &str, default: Rgb| {
            let c = theme.get(name).and_then(|h| Rgb::from_hex(h)).unwrap_or(default);
            Color32::from_rgb(c.0[0], c.0[1], c.0[2])
        };
        Colors {
            base: pick("surface-base", fallback.surface_base),
            selected: pick("surface-selected", fallback.surface_selected),
            border: pick("border-subtle", fallback.border_subtle),
            primary: pick("text-primary", fallback.text_primary),
            muted: pick("text-muted", fallback.text_muted),
            accent: pick("accent", fallback.accent),
            matched: pick("text-match", fallback.text_match),
        }
    }
}

struct Shown {
    enter_accepts: bool,
    view: View,
    colors: Colors,
}

struct Resolved {
    view: View,
    at: Placement,
    reason: &'static str,
    focus: Option<goh_anchor::Focus>,
}

struct App {
    #[cfg(target_os = "macos")]
    scroll: Option<scroll::Monitor>,
    icons: HashMap<&'static str, egui::TextureHandle>,
    frames: Receiver<Frame>,
    shown: Option<Shown>,
    /// The menu currently open (until `hide`), re-checked every TICK.
    last: Option<Request>,
    /// The window and terminal text element that first showed this menu. Never follow another tab.
    owner: Option<goh_anchor::Focus>,
    visible: bool,
    at: Option<Placement>,
    last_tick: Instant,
    drawn: u32,
    screenshot: Option<PathBuf>,
    shot_rx: Receiver<Arc<egui::ColorImage>>,
    shot_tx: Sender<Arc<egui::ColorImage>>,
    shot_requested: bool,
}

impl App {
    fn apply(&mut self, ctx: &egui::Context, frame: Frame) {
        match frame {
            Frame::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Frame::Hide => {
                self.last = None;
                self.owner = None;
                self.hide_window(ctx);
            }
            Frame::Show(req) => {
                let mut watch = true;
                match resolve_request(&req) {
                    Err(reason) => {
                        if self.owner.is_none() {
                            emit(serde_json::json!({"type": "decision", "display": "inline", "reason": reason}));
                            watch = false;
                        }
                        self.hide_window(ctx);
                    }
                    Ok(r) => {
                        if self.owns_focus(&r.focus) {
                            if self.owner.is_none() { self.owner = r.focus; }
                            emit(serde_json::json!({"type": "decision", "display": "overlay", "reason": r.reason,
                                "x": r.at.x, "y": r.at.y, "above": r.at.above, "mouse_scroll": self.mouse_scroll()}));
                            self.show_window(ctx, r.view, r.at, &req.theme, &req.state.id, req.enter_accepts);
                        } else {
                            self.hide_window(ctx);
                        }
                    }
                }
                self.last = watch.then_some(req);
                self.last_tick = Instant::now();
            }
        }
    }

    fn owns_focus(&self, focus: &Option<goh_anchor::Focus>) -> bool {
        self.owner.as_ref().is_none_or(|owner| focus.as_ref() == Some(owner))
    }

    fn mouse_scroll(&self) -> bool {
        #[cfg(target_os = "macos")]
        { self.scroll.is_some() }
        #[cfg(not(target_os = "macos"))]
        { false }
    }

    fn show_window(&mut self, ctx: &egui::Context, view: View, at: Placement, theme: &HashMap<String, String>, _id: &str, enter_accepts: bool) {
        #[cfg(target_os = "macos")]
        if let Some(scroll) = &self.scroll {
            scroll.target(_id, Screen { x: at.x, y: at.y, w: view.width.into(), h: view.height.into() });
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(view.width, view.height)));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(Pos2::new(at.x as f32, at.y as f32)));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        self.shown = Some(Shown { enter_accepts, view, colors: Colors::from(theme) });
        self.drawn = 0;
        self.visible = true;
        self.at = Some(at);
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "macos")]
        if let Some(scroll) = &self.scroll { scroll.hide(); }
        self.shown = None;
        self.visible = false;
        self.at = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    /// Re-check while a menu is open: hide if the terminal is no longer usable (minimised, hidden,
    /// another app focused), follow it if its window moved, come back if it returns. Silent: only
    /// explicit `show` frames produce a decision line.
    fn tick(&mut self, ctx: &egui::Context) {
        let Some(req) = self.last.clone() else { return };
        let result = resolve_request(&req);
        let now = result.as_ref().ok().filter(|r| self.owns_focus(&r.focus)).map(|r| r.at);
        match (plan_tick(self.visible, self.at, now), result) {
            (TickAction::Hide, _) => self.hide_window(ctx),
            (TickAction::Show, Ok(r)) => self.show_window(ctx, r.view, r.at, &req.theme, &req.state.id, req.enter_accepts),
            (TickAction::Move, Ok(r)) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(Pos2::new(r.at.x as f32, r.at.y as f32)));
                #[cfg(target_os = "macos")]
                if let Some(scroll) = &self.scroll {
                    scroll.target(&req.state.id, Screen { x: r.at.x, y: r.at.y, w: r.view.width.into(), h: r.view.height.into() });
                }
                self.at = Some(r.at);
            }
            _ => {}
        }
    }
}

fn emit(v: serde_json::Value) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn resolve_request(r: &Request) -> Result<Resolved, String> {
    let opts = goh_anchor::Options { cell_height: r.cell_height, allow_tty_query: r.tty_query };
    resolve_show(&r.state, r.anchor.clone(), r.mode, r.max_rows, r.available_width, &opts)
}

/// Everything that decides whether, and where, the overlay is drawn. `Err` is the reason for inline.
fn resolve_show(
    state: &model::State,
    anchor: Option<AnchorSpec>,
    mode: DisplayMode,
    max_rows: usize,
    manual_width: f32,
    opts: &goh_anchor::Options,
) -> Result<Resolved, String> {
    let mut focus = None;
    // Manual positions are vouched for by the caller; "auto" must come from a verified source.
    let (caret, screen, verified, width): (Caret, Screen, bool, f64) = match anchor {
        Some(AnchorSpec::Manual(m)) => {
            let screen = m.screen.map(|[x, y, w, h]| Screen { x, y, w, h }).unwrap_or(Screen { x: -1e6, y: -1e6, w: 2e6, h: 2e6 });
            let width = if m.screen.is_some() { placement::available_width(screen) } else { manual_width as f64 };
            (Caret { x: m.x, y: m.y, height: m.line_height }, screen, true, width)
        }
        Some(AnchorSpec::Keyword(k)) if k == "auto" => match goh_anchor::locate_with(opts).0 {
            Outcome::Verified(a) => {
                focus = Some(a.focus);
                (a.caret, a.display, true, placement::available_width(a.display))
            }
            Outcome::Unverified(reason) => return Err(resolve_reason(mode, reason.to_string())),
        },
        // `file:PATH`, for tests and demos: "x y line_height" in the file is a caller-vouched position
        // (like a manual anchor) that is re-read on every check; a missing or empty file means the
        // terminal is not usable (as if it were minimised).
        Some(AnchorSpec::Keyword(k)) if k.starts_with("file:") => {
            let text = std::fs::read_to_string(&k["file:".len()..]).map_err(|_| "anchor file not readable".to_string())?;
            let v: Vec<f64> = text.split_whitespace().filter_map(|t| t.parse().ok()).collect();
            let [x, y, h] = v[..] else { return Err("anchor file has no position".into()) };
            (Caret { x, y, height: h }, Screen { x: -1e6, y: -1e6, w: 2e6, h: 2e6 }, true, manual_width as f64)
        }
        _ => return Err("no caret anchor was supplied".into()),
    };
    let caps = Capabilities {
        anchor: if verified { AnchorState::Verified } else { AnchorState::Unverified },
        // Surface backend must preserve focus independently of caret verification.
        focus_safe: goh_popup::platform::focus_safe(),
        available_width: width as f32,
    };
    let decision = display::resolve(mode, &caps);
    if decision.display != Display::Overlay {
        return Err(decision.reason.to_string());
    }
    let view = model::view_with(state, max_rows, width as f32, cell_width()).ok_or_else(|| "nothing to show".to_string())?;
    let at = placement::place(caret, (view.width as f64, view.height as f64), screen);
    Ok(Resolved { view, at, reason: decision.reason, focus })
}

fn resolve_reason(mode: DisplayMode, reason: String) -> String {
    if mode == DisplayMode::Overlay { format!("overlay requested but the caret position is not verified: {reason}") } else { reason }
}

/// `--resolve`: what would happen for the focused terminal right now? No window.
fn resolve_only() -> i32 {
    let (outcome, trace) = goh_anchor::locate_traced();
    let (decision, detail, code) = match outcome {
        Outcome::Verified(a) => {
            let caps = Capabilities {
                anchor: AnchorState::Verified,
                focus_safe: goh_popup::platform::focus_safe(),
                available_width: placement::available_width(a.display) as f32,
            };
            let d = display::resolve(DisplayMode::Auto, &caps);
            let code = if d.display == Display::Overlay { 0 } else { 1 };
            (d.reason.to_string(), serde_json::json!({
                "app": a.app.name(),
                "caret": {"x": a.caret.x, "y": a.caret.y, "height": a.caret.height},
                "display": {"x": a.display.x, "y": a.display.y, "w": a.display.w, "h": a.display.h},
            }), code)
        }
        Outcome::Unverified(r) => (r.to_string(), serde_json::Value::Null, 1),
    };
    let display = if code == 0 { "overlay" } else { "inline" };
    println!("{}", serde_json::json!({"display": display, "reason": decision, "anchor": detail, "trace": trace}));
    code
}

/// Split `label` into (text, is_match) runs along the highlight ranges.
fn runs<'a>(label: &'a str, matches: &[[usize; 2]]) -> Vec<(&'a str, bool)> {
    let mut out = Vec::new();
    let mut at = 0;
    for &[s, e] in matches {
        if s > at {
            out.push((&label[at..s], false));
        }
        out.push((&label[s..e], true));
        at = e;
    }
    if at < label.len() {
        out.push((&label[at..], false));
    }
    out
}

fn load_icons(ctx: &egui::Context) -> HashMap<&'static str, egui::TextureHandle> {
    goh_popup::icons::ASSETS.iter().filter_map(|&(name, bytes)| {
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        if info.color_type != png::ColorType::Rgba { return None; }
        let image = egui::ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &buf[..info.buffer_size()]);
        Some((name, ctx.load_texture(format!("command-{name}"), image, egui::TextureOptions::LINEAR)))
    }).collect()
}

fn logo(painter: &egui::Painter, icons: &HashMap<&str, egui::TextureHandle>, key: &str, x: f32, mid: f32, colors: &Colors) {
    let rect = Rect::from_center_size(Pos2::new(x + 8.0, mid), vec2(16.0, 16.0));
    if let Some(texture) = icons.get(key) {
        let tint = if matches!(key, "rust" | "github") { colors.primary } else { Color32::WHITE };
        painter.image(texture.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
    } else {
        // Font-independent >_ fallback in the same 16-point slot as the branded logos.
        let stroke = Stroke::new(1.2, colors.muted);
        let point = |dx, dy| Pos2::new(rect.left() + dx, rect.top() + dy);
        painter.rect_stroke(rect, 2, stroke, StrokeKind::Inside);
        painter.line_segment([point(4.0, 5.0), point(7.0, 8.0)], stroke);
        painter.line_segment([point(7.0, 8.0), point(4.0, 11.0)], stroke);
        painter.line_segment([point(9.0, 11.0), point(12.0, 11.0)], stroke);
    }
}

fn draw(ui: &mut egui::Ui, v: &View, c: &Colors, enter_accepts: bool, icons: &HashMap<&str, egui::TextureHandle>) {
    let full = ui.max_rect();
    let font = FontId::monospace(font_size());
    let small = FontId::monospace(font_size() - 1.0);
    let painter = ui.painter().clone();
    painter.rect(full, RADIUS_POPUP, c.base, Stroke::new(1.0, c.border), StrokeKind::Inside);

    let list_id = ui.scope_id().with(&v.listbox_id);
    ui.ctx().accesskit_node_builder(list_id, |n| n.set_role(egui::Role::ListBox));

    for (i, row) in v.rows.iter().enumerate() {
        let top = full.top() + i as f32 * ROW_HEIGHT;
        let r = Rect::from_min_size(Pos2::new(full.left() + 4.0, top + 2.0), vec2(full.width() - 8.0, ROW_HEIGHT - 4.0));
        if row.selected {
            painter.rect_filled(r, RADIUS_ROW, c.selected);
        }
        let mid = r.center().y;
        let mut x = full.left() + PAD; // the model's width assumes PAD on both sides
        painter.text(Pos2::new(x, mid), Align2::LEFT_CENTER, if row.selected { "❯" } else { " " }, font.clone(), c.accent);
        x += 2.0 * cell_width();
        if let Some(key) = row.icon {
            logo(&painter, icons, key, x, mid, c);
            x += 3.0 * cell_width();
        }

        for (text, is_match) in runs(&row.label, &row.matches) {
            let (color, underline) = if is_match { (c.matched, Stroke::new(1.0, c.matched)) } else { (c.primary, Stroke::NONE) };
            let galley = painter.layout_no_wrap(text.to_string(), font.clone(), color);
            let w = galley.size().x;
            painter.galley_with_override_text_color(Pos2::new(x, mid - galley.size().y / 2.0), galley, color);
            if underline != Stroke::NONE {
                painter.hline(x..=x + w, mid + 8.0, underline);
            }
            x += w;
        }
        if let Some(d) = &row.description {
            painter.text(Pos2::new(x + 2.0 * cell_width(), mid), Align2::LEFT_CENTER, d, font.clone(), c.muted);
        }

        let id = ui.scope_id().with(&row.dom_id);
        let selected = row.selected;
        let name = row.accessible_name.clone();
        ui.ctx().accesskit_node_builder(id, |n| {
            n.set_role(egui::Role::ListBoxOption);
            n.set_label(name);
            n.set_selected(selected);
        });
    }

    // Footer: context on the left, position and key hints on the right.
    let fy = full.top() + v.rows.len() as f32 * ROW_HEIGHT;
    painter.hline(full.left() + 1.0..=full.right() - 1.0, fy, Stroke::new(1.0, c.border));
    let mid = fy + FOOTER_HEIGHT / 2.0;
    let mut context_x = full.left() + PAD + 4.0;
    if let Some(key) = goh_popup::icons::key(&v.context, "") {
        logo(&painter, icons, key, context_x, mid, c);
        context_x += 22.0;
    }
    painter.text(Pos2::new(context_x, mid), Align2::LEFT_CENTER, &v.context, small.clone(), c.muted);
    let accept = if enter_accepts { "Tab/↵/→" } else { "Tab/→" };
    let right = match &v.position {
        Some(p) => format!("{p}   ↑↓  {accept}  Esc"),
        None => format!("↑↓  {accept}  Esc"),
    };
    painter.text(Pos2::new(full.right() - PAD - 4.0, mid), Align2::RIGHT_CENTER, right, small, c.muted);
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    /// Runs even while the window is hidden, so a `show` frame can bring it back.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Frames can queue up while typing is fast; only the newest one matters (each `show` costs an
        // Accessibility read), so apply just the last, unless a quit is waiting.
        let mut newest: Option<Frame> = None;
        while let Ok(f) = self.frames.try_recv() {
            if matches!(f, Frame::Quit) {
                newest = Some(f);
                break;
            }
            newest = Some(f);
        }
        if let Some(f) = newest {
            self.apply(ctx, f);
        }
        if self.last.is_some() {
            if self.last_tick.elapsed() >= TICK {
                self.last_tick = Instant::now();
                self.tick(ctx);
            }
            ctx.request_repaint_after(TICK.saturating_sub(self.last_tick.elapsed()));
        }
        #[cfg(target_os = "macos")]
        if let Some((id, delta)) = self.scroll.as_ref().and_then(scroll::Monitor::take) {
            if self.visible && self.last.as_ref().is_some_and(|r| r.state.id == id) {
                emit(serde_json::json!({"type": "navigate", "id": id, "delta": delta}));
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(s) = &self.shown {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| draw(ui, &s.view, &s.colors, s.enter_accepts, &self.icons));
            self.drawn += 1;
            if self.screenshot.is_some() && self.drawn == 6 && !self.shot_requested {
                self.shot_requested = true;
                let tx = self.shot_tx.clone();
                ctx.request_screenshot(move |img| {
                    let _ = tx.send(img);
                });
            }
            if self.drawn < 8 {
                ctx.request_repaint();
            }
        }
        if let (Ok(img), Some(path)) = (self.shot_rx.try_recv(), &self.screenshot) {
            if let Err(e) = save_png(path, &img) {
                eprintln!("goh-popup: screenshot failed: {e}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.shot_requested {
            ctx.request_repaint();
        }
    }
}

fn save_png(path: &PathBuf, img: &egui::ColorImage) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, img.width() as u32, img.height() as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let bytes: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
    enc.write_header()?.write_image_data(&bytes)?;
    Ok(())
}

/// The terminal's own font, when it could be found: file bytes, the size to draw at and its cell width.
struct TerminalFont {
    bytes: Vec<u8>,
    size: f32,
    cell_width: f32,
}

static TERMINAL_FONT: std::sync::OnceLock<Option<TerminalFont>> = std::sync::OnceLock::new();

fn terminal_font() -> Option<&'static TerminalFont> {
    TERMINAL_FONT.get_or_init(load_terminal_font).as_ref()
}

fn font_size() -> f32 {
    terminal_font().map_or(FONT, |f| f.size)
}

fn cell_width() -> f32 {
    terminal_font().map_or(CELL_WIDTH, |f| f.cell_width)
}

/// What font the popup uses, first match wins: `GOH_POPUP_FONT` (`Name` or `Name:size`), then `popup.font` and
/// `popup.fontSize` in the config file (`goh config`), then the terminal's own settings (VS Code, iTerm2).
/// Terminals that keep it somewhere unreadable (the Claude app, Terminal.app) get the built-in monospace.
fn configured_font() -> Option<TermFont> {
    if let Some(spec) = std::env::var("GOH_POPUP_FONT").ok().filter(|s| !s.is_empty()) {
        return font::parse_override(&spec);
    }
    let file = goh_theme::read_config();
    let popup = file.as_ref().and_then(|c| c.get("popup"));
    let size = popup.and_then(|p| p.get("fontSize")).and_then(|v| v.as_f64()).map(|s| s as f32);
    if let Some(name) = popup.and_then(|p| p.get("font")).and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
        return Some(TermFont { families: vec![name.trim().to_string()], size });
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    match std::env::var("TERM_PROGRAM").ok()?.as_str() {
        "vscode" => font::vscode(&std::fs::read_to_string(home.join("Library/Application Support/Code/User/settings.json")).ok()?),
        "iTerm.app" => {
            let out = std::process::Command::new("plutil")
                .args(["-convert", "json", "-o", "-"])
                .arg(home.join("Library/Preferences/com.googlecode.iterm2.plist"))
                .output()
                .ok()?;
            font::iterm(&String::from_utf8_lossy(&out.stdout), std::env::var("ITERM_PROFILE").ok().as_deref())
                .map(|f| TermFont { size: f.size.or(size), ..f })
        }
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn load_terminal_font() -> Option<TerminalFont> {
    let wanted = configured_font()?;
    for family in &wanted.families {
        let want = font::normalize(family);
        let Some(path) = coretext::font_file(family, |n| font::normalize(n) == want) else { continue };
        let Ok(bytes) = std::fs::read(&path) else { continue };
        if bytes.len() > 32 << 20 {
            continue;
        }
        let size = wanted.size.unwrap_or(FONT).clamp(FONT_MIN, FONT_MAX);
        if let Some(cell_width) = font::cell_width(&bytes, size) {
            return Some(TerminalFont { bytes, size, cell_width });
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn load_terminal_font() -> Option<TerminalFont> {
    let wanted = configured_font()?;
    for family in &wanted.families {
        let output = std::process::Command::new("fc-match").args(["-f", "%{file}", "--", family]).output().ok()?;
        if !output.status.success() { continue; }
        let path = String::from_utf8(output.stdout).ok()?;
        let Ok(metadata) = std::fs::metadata(&path) else { continue; };
        if metadata.len() > 32 << 20 { continue; }
        let Ok(bytes) = std::fs::read(&path) else { continue; };
        let size = wanted.size.unwrap_or(FONT).clamp(FONT_MIN, FONT_MAX);
        if let Some(cell_width) = font::cell_width(&bytes, size) {
            return Some(TerminalFont { bytes, size, cell_width });
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn load_terminal_font() -> Option<TerminalFont> {
    None
}

fn install_terminal_font(ctx: &egui::Context) {
    if let Some(f) = terminal_font() {
        ctx.add_font(egui::epaint::text::FontInsert::new(
            "terminal",
            egui::FontData::from_owned(f.bytes.clone()),
            vec![egui::epaint::text::InsertFontFamily { family: egui::FontFamily::Monospace, priority: egui::epaint::text::FontPriority::Highest }],
        ));
    }
}

/// Opt-in (`popup.ownPermission` in the config, `GOH_POPUP_OWN_PERMISSION=on`): run the popup as its own
/// "responsible" process, so macOS asks for the Accessibility permission once, for goh-popup, instead of once per
/// terminal app. A child of the shell is attributed to the terminal that runs it; a process spawned with
/// responsibility disclaimed answers for itself.
/// The disclaim call is private API (Chromium and LLDB use it); without it the popup runs as before. This process
/// stays as the one the adapter knows (its pid, its stdin and stdout are shared), waits, and exits as the child does.
/// macOS ties the permission to the binary: an unsigned build is a new binary after each update and asks again.
#[cfg(target_os = "macos")]
fn run_as_own_responsible_process() {
    use std::ffi::{CString, OsString};
    use std::os::unix::ffi::OsStrExt;
    const MARK: &str = "GOH_POPUP_RESPONSIBLE";
    if std::env::var_os("GOH_POPUP_OWN_PERMISSION").is_none_or(|v| v != "on") || std::env::var_os(MARK).is_some() {
        return;
    }
    let cstr = |s: &std::ffi::OsStr| CString::new(s.as_bytes()).ok();
    let Some(exe) = std::env::current_exe().ok().and_then(|p| cstr(p.as_os_str())) else { return };
    let Some(args) = std::env::args_os().map(|a| cstr(&a)).collect::<Option<Vec<_>>>() else { return };
    let mut vars: Vec<OsString> = std::env::vars_os()
        .filter(|(k, _)| k != MARK)
        .map(|(mut k, v)| {
            k.push("=");
            k.push(v);
            k
        })
        .collect();
    vars.push(format!("{MARK}=1").into());
    let Some(env) = vars.iter().map(|e| cstr(e)).collect::<Option<Vec<CString>>>() else { return };
    let ptrs = |v: &[CString]| v.iter().map(|s| s.as_ptr().cast_mut()).chain(std::iter::once(std::ptr::null_mut())).collect::<Vec<_>>();
    let (argv, envp) = (ptrs(&args), ptrs(&env));
    // SAFETY: the symbol, when present, has this signature (<spawn_private.h>). `attr` is initialised before use
    // and destroyed after; every pointer handed to posix_spawn outlives the call and the arrays end in null.
    let pid = unsafe {
        let sym = libc::dlsym(libc::RTLD_DEFAULT, c"responsibility_spawnattrs_setdisclaim".as_ptr());
        if sym.is_null() {
            return;
        }
        let disclaim: unsafe extern "C" fn(*mut libc::posix_spawnattr_t, bool) -> libc::c_int = std::mem::transmute(sym);
        let mut attr: libc::posix_spawnattr_t = std::ptr::null_mut();
        if libc::posix_spawnattr_init(&mut attr) != 0 {
            return;
        }
        let mut pid = 0;
        let spawned = disclaim(&mut attr, true) == 0 && libc::posix_spawn(&mut pid, exe.as_ptr(), std::ptr::null(), &attr, argv.as_ptr(), envp.as_ptr()) == 0;
        libc::posix_spawnattr_destroy(&mut attr);
        if !spawned {
            return;
        }
        pid
    };
    let mut status = 0;
    // SAFETY: waits for our own child; `status` is a valid out pointer.
    while unsafe { libc::waitpid(pid, &mut status, 0) } == -1 {
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            std::process::exit(1);
        }
    }
    let code = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) } else if libc::WIFSIGNALED(status) { 128 + libc::WTERMSIG(status) } else { 1 };
    std::process::exit(code)
}

/// In the popup that answers for itself: ask macOS to list it under Accessibility (with its prompt) while it
/// is not allowed yet. The terminal's own permission no longer counts for it.
#[cfg(target_os = "macos")]
fn ask_for_own_permission() {
    if std::env::var_os("GOH_POPUP_RESPONSIBLE").is_some() && goh_anchor::accessibility_trusted() == Some(false) {
        goh_anchor::request_accessibility();
    }
}

fn main() -> eframe::Result {
    #[cfg(target_os = "macos")]
    {
        run_as_own_responsible_process();
        ask_for_own_permission();
    }
    if std::env::args().any(|a| a == "--probe") {
        // Run from the terminal you want to test; the delay leaves time to focus it again if needed.
        std::thread::sleep(std::time::Duration::from_secs(2));
        goh_anchor::probe().iter().for_each(|l| println!("{l}"));
        return Ok(());
    }
    if std::env::args().any(|a| a == "--resolve") {
        std::process::exit(resolve_only());
    }
    let screenshot = std::env::args().skip_while(|a| a != "--screenshot").nth(1).map(PathBuf::from);

    // A popup must never become the active app or take clicks: the terminal keeps focus.
    let viewport = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_taskbar(false)
        .with_active(false)
        .with_visible(false)
        .with_mouse_passthrough(true)
        .with_resizable(false)
        .with_inner_size([WIDTH_PLACEHOLDER, 100.0]);
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport,
        #[cfg(target_os = "macos")]
        event_loop_builder: Some(Box::new(|b| {
            use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
            // winit activates the app (stealing focus) at launch unless told not to.
            b.with_activation_policy(ActivationPolicy::Accessory).with_activate_ignoring_other_apps(false);
        })),
        ..Default::default()
    };

    #[cfg(target_os = "linux")]
    {
        if !goh_popup::platform::focus_safe() {
            // Answer frames without opening an unpositionable Wayland top-level window.
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                match serde_json::from_str::<Frame>(&line) {
                    Ok(Frame::Quit) => break,
                    Ok(Frame::Show(_)) => println!("{}", serde_json::json!({"type":"decision", "display":"inline", "reason":"native Wayland popup surface provider unavailable"})),
                    _ => {}
                }
            }
            return Ok(());
        }
        goh_popup::platform::x11::configure(&mut options);
    }

    eframe::run_native(
        "goh-popup",
        options,
        Box::new(move |cc| {
            let (tx, frames) = channel();
            let ctx = cc.egui_ctx.clone();
            install_terminal_font(&cc.egui_ctx);
            #[cfg(target_os = "linux")]
            cc.egui_ctx.set_zoom_factor(1.0);
            std::thread::spawn(move || {
                for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                    match serde_json::from_str::<Frame>(&line) {
                        Ok(f) => {
                            let quit = matches!(f, Frame::Quit);
                            if tx.send(f).is_err() || quit {
                                break;
                            }
                            ctx.request_repaint();
                        }
                        Err(e) => eprintln!("goh-popup: ignoring malformed frame: {e}"),
                    }
                }
                // stdin closed: the adapter is gone, so the popup goes too.
                let _ = tx.send(Frame::Quit);
                ctx.request_repaint();
            });
            let (shot_tx, shot_rx) = channel();
            Ok(Box::new(App {
                #[cfg(target_os = "macos")]
                scroll: scroll::Monitor::new(cc.egui_ctx.clone()),
                icons: load_icons(&cc.egui_ctx),
                frames, shown: None, last: None, owner: None, visible: false, at: None, last_tick: Instant::now(), drawn: 0, screenshot, shot_rx, shot_tx, shot_requested: false }))
        }),
    )
}

const WIDTH_PLACEHOLDER: f32 = model::WIDTH_MIN;

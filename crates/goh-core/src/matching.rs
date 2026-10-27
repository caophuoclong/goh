//! Prefix / case- and accent-folded prefix / in-order subsequence matching.
//! Ranges are UTF-8 byte ranges of the label.

use unicode_normalization::UnicodeNormalization;

#[derive(Debug, PartialEq)]
pub struct Match {
    pub score: f64,
    pub ranges: Vec<[usize; 2]>,
}

fn fold(c: char) -> char {
    if matches!(c, 'đ' | 'Đ') {
        return 'd';
    }
    let base = std::iter::once(c).nfd().next().unwrap_or(c);
    base.to_lowercase().next().unwrap_or(base)
}

pub fn match_label(typed: &str, label: &str) -> Option<Match> {
    if typed.is_empty() {
        return Some(Match { score: 100.0, ranges: vec![] });
    }
    if label.starts_with(typed) {
        return Some(Match { score: 100.0, ranges: vec![[0, typed.len()]] });
    }

    let want: Vec<char> = typed.chars().map(fold).collect();
    let have: Vec<(usize, char, usize)> = label
        .char_indices()
        .map(|(i, c)| (i, fold(c), i + c.len_utf8()))
        .collect();

    if have.len() >= want.len() && have.iter().zip(&want).all(|(h, w)| h.1 == *w) {
        return Some(Match { score: 90.0, ranges: vec![[0, have[want.len() - 1].2]] });
    }

    // One typed character matches a prefix only: scattered in-order matches for a single letter are noise.
    if want.len() < 2 {
        return None;
    }
    let mut ranges: Vec<[usize; 2]> = Vec::new();
    let mut next = 0;
    let mut first = None;
    let mut last = 0;
    for (idx, (start, c, end)) in have.iter().enumerate() {
        if next < want.len() && *c == want[next] {
            match ranges.last_mut() {
                Some(r) if r[1] == *start => r[1] = *end,
                _ => ranges.push([*start, *end]),
            }
            first.get_or_insert(idx);
            last = idx;
            next += 1;
        }
    }
    if next < want.len() {
        return None;
    }
    let gaps = last - first.unwrap_or(0) + 1 - want.len();
    Some(Match { score: 50.0 - gaps.min(40) as f64, ranges })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_prefix_beats_folded_beats_subsequence() {
        let exact = match_label("ch", "checkout").unwrap();
        let folded = match_label("CH", "checkout").unwrap();
        let sub = match_label("ch", "fetch").unwrap();
        assert!(exact.score > folded.score && folded.score > sub.score);
        assert_eq!(sub.ranges, vec![[3, 5]]);
    }

    #[test]
    fn accent_folded_prefix_keeps_byte_ranges() {
        let m = match_label("tai", "Tài liệu/").unwrap();
        assert_eq!(m.ranges, vec![[0, 4]]); // "Tài" = 1 + 2 + 1 bytes
    }

    #[test]
    fn a_single_character_matches_prefixes_only() {
        assert!(match_label("c", "checkout").is_some());
        assert!(match_label("c", "fetch").is_none(), "a lone letter found in the middle is not a match");
        assert!(match_label("ch", "fetch").is_some(), "two characters may match in order");
    }

    #[test]
    fn non_match_is_none() {
        assert!(match_label("zz", "checkout").is_none());
    }
}

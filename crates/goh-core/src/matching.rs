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
    // The tightest in-order run wins: `goh` in `phuoclong/personal/goh` is its last three letters, not `g…o…h`
    // spread out from the first `g`.
    let mut best: Option<(usize, Vec<[usize; 2]>)> = None;
    for first in (0..have.len()).filter(|&i| have[i].1 == want[0]) {
        let mut ranges: Vec<[usize; 2]> = Vec::new();
        let mut next = 0;
        let mut last = first;
        for (idx, (start, c, end)) in have.iter().enumerate().skip(first) {
            if *c == want[next] {
                match ranges.last_mut() {
                    Some(r) if r[1] == *start => r[1] = *end,
                    _ => ranges.push([*start, *end]),
                }
                last = idx;
                next += 1;
                if next == want.len() {
                    break;
                }
            }
        }
        if next < want.len() {
            break; // a later start cannot fit it either
        }
        let gaps = last - first + 1 - want.len();
        if !matches!(best, Some((b, _)) if b <= gaps) {
            best = Some((gaps, ranges));
        }
    }
    let (gaps, ranges) = best?;
    Some(Match { score: 50.0 - gaps.min(40) as f64, ranges })
}

/// A path is matched by its last component first: `goh` finds `~/personal/goh` before `~/.oh-my-zsh`, whose full
/// path only spells it across `phuoclong/.oh`. The whole path still matches when the last component does not,
/// ranked below every match on a last component (those score at least 10). Typing a `/` matches the whole path.
pub fn match_path(typed: &str, label: &str) -> Option<Match> {
    let base = label.trim_end_matches('/').rfind('/').map_or(0, |i| i + 1);
    if base == 0 || typed.contains('/') {
        return match_label(typed, label);
    }
    if let Some(m) = match_label(typed, &label[base..]) {
        let ranges = m.ranges.iter().map(|r| [r[0] + base, r[1] + base]).collect();
        return Some(Match { score: m.score, ranges });
    }
    match_label(typed, label).map(|m| Match { score: m.score / 5.0, ..m })
}

/// A near miss for a word that was almost typed right: one slip (a letter swapped, dropped, added or wrong) in
/// a word of four or more characters, two from six. Ranked below every real match; no ranges, since nothing
/// of what was typed lines up with the label.
pub fn near(typed: &str, label: &str) -> Option<Match> {
    let want: Vec<char> = typed.chars().map(fold).collect();
    if want.len() < 4 {
        return None;
    }
    let have: Vec<char> = label.trim_end_matches('/').chars().map(fold).collect();
    let limit = if want.len() >= 6 { 2 } else { 1 };
    if have.len().abs_diff(want.len()) > limit {
        return None;
    }
    let d = distance(&want, &have);
    (1..=limit).contains(&d).then(|| Match { score: 40.0 - d as f64, ranges: vec![] })
}

/// Optimal string alignment distance: insertions, deletions, substitutions and adjacent swaps.
fn distance(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[n][m]
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
    fn a_slip_in_a_longer_word_is_a_near_miss() {
        assert!(near("chekcout", "checkout").is_some(), "swapped letters");
        assert!(near("chckout", "checkout").is_some(), "dropped letter");
        assert!(near("checkoutt", "checkout").is_some(), "extra letter");
        assert!(near("statsu", "status").is_some());
        assert!(near("verbsoe", "verbose").is_some());
        assert!(near("CHEKCOUT", "checkout").is_some(), "case is folded");
        assert!(near("brnch", "branch").is_some());
        assert!(near("bra", "branch").is_none(), "short words are prefixes only");
        assert!(near("stat", "star").is_some() && near("stax", "star").is_some());
        assert!(near("xyzw", "status").is_none());
        assert!(near("chkot", "checkout").is_none(), "too far for a short word");
        assert!(near("checkout", "checkout").is_none(), "a right word is not a slip");
    }

    #[test]
    fn subsequence_takes_the_tightest_run() {
        let m = match_label("goh", "phuoclong/personal/goh").unwrap();
        assert_eq!(m.score, 50.0);
        assert_eq!(m.ranges, vec![[19, 22]]);
    }

    #[test]
    fn a_path_matches_by_its_last_component_first() {
        let own = match_path("goh", "/Users/me/personal/goh").unwrap();
        let spread = match_path("goh", "/Users/phuoclong/.oh-my-zsh").unwrap();
        let nested = match_path("goh", "/Users/phuoclong/.oh-my-zsh/lib").unwrap();
        assert_eq!(own.ranges, vec![[19, 22]]);
        assert!(own.score > spread.score && own.score > nested.score);
        assert!(match_path("lib", "/Users/phuoclong/.oh-my-zsh/lib/").unwrap().score == 100.0, "trailing slash");
        assert_eq!(match_path("my/l", "/Users/x/.oh-my-zsh/lib").unwrap().ranges, vec![[13, 15], [19, 21]]);
        assert!(match_path("zz", "/Users/x/goh").is_none());
    }

    #[test]
    fn non_match_is_none() {
        assert!(match_label("zz", "checkout").is_none());
    }
}

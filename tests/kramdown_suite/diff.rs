//! A line diff, for how a case's HTML differs from kramdown's.

use std::fmt::Write as _;

/// The most lines of difference shown for one case.
const DIFF_LINES: usize = 40;

/// The lines that differ between `expected` and `actual`, `-` for
/// expected and `+` for actual, with a line around each change, up to
/// `DIFF_LINES` of them.
pub fn diff(expected: &str, actual: &str) -> String {
    let a: Vec<&str> = expected.lines().collect();
    let b: Vec<&str> = actual.lines().collect();
    // The longest common subsequence of the lines after each pair.
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut lines = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            lines.push((' ', a[i]));
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            lines.push(('+', b[j]));
            j += 1;
        } else {
            lines.push(('-', a[i]));
            i += 1;
        }
    }
    let changed = |k: usize| lines.get(k).is_some_and(|(sign, _)| *sign != ' ');
    let mut out = String::new();
    let mut shown = 0;
    let mut last = None;
    for (k, (sign, line)) in lines.iter().enumerate() {
        let near = changed(k) || changed(k + 1) || (k > 0 && changed(k - 1));
        if !near {
            continue;
        }
        if shown == DIFF_LINES {
            out.push_str("  …\n");
            break;
        }
        if last.is_some_and(|last| last + 1 < k) {
            out.push_str("  …\n");
        }
        writeln!(out, "{sign} {line}").unwrap();
        shown += 1;
        last = Some(k);
    }
    out
}

use std::fmt::Write;

use super::FileDiff;
use super::lines::{DiffLine, Sign, line_ops, split_lines};

const CONTEXT_LINES: usize = 3;
const MAX_DIFF_LINES: usize = 60;

#[derive(Clone, Debug)]
pub struct Hunk<'a> {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<DiffLine<'a>>,
}

/// Unified diff of two texts; `None` when they have no differing lines.
pub fn diff_texts(path: &str, old: &str, new: &str) -> Option<FileDiff> {
    let ops = line_ops(&split_lines(old), &split_lines(new));
    file_diff_of(path, group_hunks(&ops))
}

pub fn diff_from_unified(path: &str, body: &str) -> Option<FileDiff> {
    file_diff_of(path, parse_unified(body))
}

pub fn file_diff_of(path: &str, hunks: Vec<Hunk<'_>>) -> Option<FileDiff> {
    if hunks.is_empty() {
        return None;
    }
    let added = hunks
        .iter()
        .map(|hunk| count_of(&hunk.lines, Sign::Added))
        .sum();
    let removed = hunks
        .iter()
        .map(|hunk| count_of(&hunk.lines, Sign::Removed))
        .sum();
    let (capped, truncated) = cap_hunks(hunks, MAX_DIFF_LINES);
    Some(FileDiff {
        path: path.to_owned(),
        diff: format_diff(path, &capped),
        added,
        removed,
        truncated,
    })
}

fn group_hunks<'a>(ops: &[DiffLine<'a>]) -> Vec<Hunk<'a>> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (index, _) in ops
        .iter()
        .enumerate()
        .filter(|(_, op)| op.sign != Sign::Context)
    {
        let start = index.saturating_sub(CONTEXT_LINES);
        let end = (index + CONTEXT_LINES).min(ops.len() - 1);
        match ranges.last_mut() {
            Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
            _ => ranges.push((start, end)),
        }
    }

    ranges
        .into_iter()
        .map(|(start, end)| {
            let mut old_start = 1;
            let mut new_start = 1;
            for op in &ops[..start] {
                if op.sign != Sign::Added {
                    old_start += 1;
                }
                if op.sign != Sign::Removed {
                    new_start += 1;
                }
            }
            let mut lines = ops[start..=end].to_vec();
            let leading = lines
                .iter()
                .take_while(|line| is_blank_context(line))
                .count();
            lines.drain(..leading);
            old_start += leading;
            new_start += leading;
            while lines.last().is_some_and(is_blank_context) {
                lines.pop();
            }
            Hunk {
                old_start,
                new_start,
                lines,
            }
        })
        .collect()
}

fn is_blank_context(line: &DiffLine<'_>) -> bool {
    line.sign == Sign::Context && line.text.trim().is_empty()
}

fn count_of(lines: &[DiffLine<'_>], sign: Sign) -> usize {
    lines.iter().filter(|line| line.sign == sign).count()
}

fn cap_hunks(hunks: Vec<Hunk<'_>>, max_lines: usize) -> (Vec<Hunk<'_>>, usize) {
    let total: usize = hunks.iter().map(|hunk| hunk.lines.len()).sum();
    if total <= max_lines {
        return (hunks, 0);
    }
    let mut kept = Vec::new();
    let mut remaining = max_lines;
    for hunk in hunks {
        if remaining == 0 {
            break;
        }
        let length = hunk.lines.len();
        kept.push(Hunk {
            old_start: hunk.old_start,
            new_start: hunk.new_start,
            lines: hunk.lines.into_iter().take(remaining).collect(),
        });
        remaining = remaining.saturating_sub(length);
    }
    (kept, total - max_lines)
}

fn format_diff(path: &str, hunks: &[Hunk<'_>]) -> String {
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    for hunk in hunks {
        let old_count = hunk.lines.len() - count_of(&hunk.lines, Sign::Added);
        let new_count = hunk.lines.len() - count_of(&hunk.lines, Sign::Removed);
        let _ = writeln!(
            out,
            "@@ -{},{old_count} +{},{new_count} @@",
            hunk.old_start, hunk.new_start
        );
        for line in &hunk.lines {
            let _ = writeln!(out, "{}{}", line.sign.symbol(), line.text);
        }
    }
    out
}

fn parse_unified(body: &str) -> Vec<Hunk<'_>> {
    let mut hunks: Vec<Hunk<'_>> = Vec::new();
    for raw in body.split('\n') {
        if let Some((old_start, new_start)) = parse_header(raw) {
            hunks.push(Hunk {
                old_start,
                new_start,
                lines: Vec::new(),
            });
        } else if let (Some(current), Some(line)) = (hunks.last_mut(), DiffLine::parse(raw)) {
            current.lines.push(line);
        }
    }
    hunks.retain(|hunk| !hunk.lines.is_empty());
    hunks
}

/// `@@ -a[,n] +b[,n] @@`, returning the two start lines.
fn parse_header(raw: &str) -> Option<(usize, usize)> {
    let rest = raw.strip_prefix("@@ -")?;
    let (old_start, rest) = take_number(rest)?;
    let rest = skip_count(rest).strip_prefix(" +")?;
    let (new_start, rest) = take_number(rest)?;
    skip_count(rest)
        .starts_with(" @@")
        .then_some((old_start, new_start))
}

fn take_number(text: &str) -> Option<(usize, &str)> {
    let end = text
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    Some((text[..end].parse().ok()?, &text[end..]))
}

fn skip_count(text: &str) -> &str {
    text.strip_prefix(',')
        .and_then(take_number)
        .map_or(text, |(_, rest)| rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined(lines: &[&str]) -> String {
        lines.join("\n")
    }

    #[test]
    fn identical_texts_have_no_diff() {
        assert_eq!(diff_texts("a.ts", "same\n", "same\n"), None);
    }

    #[test]
    fn a_replaced_line_keeps_context_and_counts() {
        let diff = diff_texts("a.ts", "one\ntwo\nthree\n", "one\n2\nthree\n").unwrap();
        assert_eq!((diff.added, diff.removed), (1, 1));
        assert_eq!(
            diff.diff,
            joined(&[
                "--- a/a.ts",
                "+++ b/a.ts",
                "@@ -1,3 +1,3 @@",
                " one",
                "-two",
                "+2",
                " three",
                "",
            ])
        );
    }

    #[test]
    fn blank_context_at_the_edges_of_a_hunk_is_trimmed() {
        let diff = diff_texts("a.rs", "\n\nuse b;\nx\n", "\n\nuse b;\n").unwrap();
        assert_eq!(
            diff.diff,
            joined(&[
                "--- a/a.rs",
                "+++ b/a.rs",
                "@@ -3,2 +3,1 @@",
                " use b;",
                "-x",
                ""
            ])
        );
    }

    #[test]
    fn very_large_rewrites_fall_back_to_remove_all_add_all() {
        let before = (0..2100)
            .map(|i| format!("old{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let after = (0..2100)
            .map(|i| format!("new{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let diff = diff_texts("big.ts", &before, &after).unwrap();
        assert_eq!((diff.removed, diff.added), (2100, 2100));
    }

    #[test]
    fn far_apart_changes_stay_in_separate_hunks() {
        let before = (0..20)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let after = before
            .replacen("l1\n", "L1\n", 1)
            .replacen("l18\n", "L18\n", 1);
        let diff = diff_texts("a.ts", &before, &after).unwrap();
        assert_eq!(diff.diff.lines().filter(|l| l.starts_with("@@")).count(), 2);
        assert_eq!((diff.added, diff.removed), (2, 2));
    }

    #[test]
    fn a_new_file_is_all_additions() {
        let diff = diff_texts("new.ts", "", "a\nb\n").unwrap();
        assert_eq!((diff.added, diff.removed), (2, 0));
        assert!(diff.diff.contains("@@ -1,0 +1,2 @@"));
    }

    #[test]
    fn long_output_is_capped_and_hunk_counts_recomputed() {
        let content = (0..200)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let diff = diff_texts("big.ts", "", &content).unwrap();
        assert_eq!((diff.added, diff.truncated), (200, 140));
        assert!(diff.diff.contains("@@ -1,0 +1,60 @@"));
        assert_eq!(
            diff.diff.lines().filter(|l| l.starts_with("+line")).count(),
            60
        );
    }

    #[test]
    fn unified_bodies_keep_their_own_line_numbers() {
        let diff = diff_from_unified("a.ts", "@@ -10,3 +12 @@\n ctx\n-x\n+y\n").unwrap();
        assert!(diff.diff.contains("@@ -10,2 +12,2 @@"));
        assert_eq!((diff.added, diff.removed), (1, 1));
    }

    #[test]
    fn bodies_without_a_valid_header_or_lines_yield_nothing() {
        assert_eq!(diff_from_unified("a.ts", "just text\n"), None);
        assert_eq!(diff_from_unified("a.ts", "@@ -1 +1 @@\n"), None);
        assert_eq!(diff_from_unified("a.ts", "@@ -x +1 @@\n+y\n"), None);
    }
}

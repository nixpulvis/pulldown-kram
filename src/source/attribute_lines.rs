use std::borrow::Cow;

use super::{Fence, Offsets, Rewrite};
use crate::attributes::AttributeLists;

/// kramdown reads a line of only attribute lists as a block of its own,
/// ending the one before it. CommonMark reads a line indented as code, or
/// a link definition, right after it as more of the paragraph the lists
/// are in, so this puts a blank line between them, outside lists, quotes
/// and HTML, where one would end more than the paragraph.
pub(crate) struct AttributeLines;

impl AttributeLines {
    /// `markdown`, with a blank line after each unindented line of only
    /// attribute lists that code or a link definition comes right after,
    /// and the offsets of the lines added.
    pub fn separate(markdown: &str) -> (Cow<'_, str>, Offsets) {
        if !markdown.contains("{:") {
            return (Cow::Borrowed(markdown), Offsets::default());
        }
        let mut out = Rewrite::of(markdown);
        let mut fence = Fence::default();
        // Whether the lines since the last blank one may be in a list, a
        // quote or HTML.
        let mut contained = false;
        let mut after_blank = true;
        let mut lines = markdown.split_inclusive('\n').peekable();
        while let Some(line) = lines.next() {
            out.keep(line);
            if fence.read(line) {
                after_blank = false;
                continue;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                after_blank = true;
                continue;
            }
            let indent = Self::indent(line);
            if after_blank {
                contained = (1..4).contains(&indent)
                    || indent == 0 && (trimmed.starts_with(['<', '>']) || Self::item(trimmed));
                after_blank = false;
            }
            let only_lists =
                || AttributeLists::parse(trimmed).is_some_and(|(_, len)| len == trimmed.len());
            let block_next = lines.peek().is_some_and(|next| {
                let indent = Self::indent(next);
                let next = next.trim();
                !next.is_empty() && (indent >= 4 || Self::link_definition(next))
            });
            if !contained && indent == 0 && block_next && only_lists() {
                out.insert("\n");
            }
        }
        out.finish(markdown)
    }

    /// Whether `trimmed` looks like the start of a link definition, like
    /// `[a]: b`, but not a footnote's.
    fn link_definition(trimmed: &str) -> bool {
        trimmed
            .strip_prefix('[')
            .filter(|label| !label.starts_with('^'))
            .and_then(|label| label.find(']'))
            .is_some_and(|end| trimmed[end + 2..].starts_with(':'))
    }

    /// Whether `trimmed` starts a list item, like `* a` or `1. a`.
    fn item(trimmed: &str) -> bool {
        let digits = trimmed.trim_start_matches(|c: char| c.is_ascii_digit());
        let rest = if digits.len() < trimmed.len() {
            digits.strip_prefix(['.', ')'])
        } else {
            trimmed.strip_prefix(['*', '-', '+'])
        };
        rest.is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
    }

    /// The columns `line` is indented by, with tab stops at multiples of
    /// four.
    fn indent(line: &str) -> usize {
        let mut columns = 0;
        for c in line.chars() {
            match c {
                ' ' => columns += 1,
                '\t' => columns += 4 - columns % 4,
                _ => break,
            }
        }
        columns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_lines_separate_ends_a_line_of_lists_before_code_or_a_link_definition() {
        let src = "a\n{:.x}\n    b\n\n{:.y}\n\tc\n\n{:.z}\n[d]: e\n";
        let (text, offsets) = AttributeLines::separate(src);
        assert_eq!(
            text,
            "a\n{:.x}\n\n    b\n\n{:.y}\n\n\tc\n\n{:.z}\n\n[d]: e\n"
        );
        assert_eq!(offsets.was(src, &text, "{:.x}\n\n"), "{:.x}\n");
        offsets.assert_fit(src, &text);
        // Not before what isn't code or a link definition, after more than
        // lists, in a list, a quote, HTML or a fence, or indented.
        let src = "{:.x}\n  a\n\n{:.x}\n[^b]: c\n\n{:.x}\n[d] e: f\n\nb {:.x}\n    c\n\n\
            * d\n{:.x}\n    e\n\n> f\n{:.x}\n    g\n\n<div>\n{:.x}\n    h\n\n```\n{:.x}\n    i\n\
            ```\n\n  j\n {:.x}\n    k\n";
        let (text, offsets) = AttributeLines::separate(src);
        assert!(matches!(text, Cow::Borrowed(_)));
        assert!(offsets.is_empty());
    }
}

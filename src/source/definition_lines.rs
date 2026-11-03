use std::borrow::Cow;

use super::{Fence, Offsets, Rewrite};
use crate::text::Lead;

/// kramdown reads two things about definitions differently from
/// pulldown-cmark, which this rewrites the lines for. A `:` more than one
/// blank line after the terms starts no definition, so it's escaped. And a
/// definition with nothing on its first line has its content indented by
/// four, not two past the `:`, so the lines of it indented by four or more
/// lose the difference.
pub(crate) struct DefinitionLines {
    out: Rewrite,
    fence: Fence,
    /// The blank lines in a row before this one.
    blanks: usize,
    /// Whether the lines are in a definition, which a `:` after blank lines
    /// adds one to.
    in_definition: bool,
    /// For a definition with nothing on its first line, the spaces to take
    /// off its lines, and whether a line of it has been read yet.
    shift: Option<(usize, bool)>,
}

impl DefinitionLines {
    /// `markdown`, with its definitions' lines rewritten as kramdown reads
    /// them, and the offsets of the edits.
    pub fn rewrite(markdown: &str) -> (Cow<'_, str>, Offsets) {
        let starts_definition =
            |line: &str| line.trim_start_matches([' ', '\t', '>']).starts_with(':');
        if !markdown.lines().any(starts_definition) {
            return (Cow::Borrowed(markdown), Offsets::default());
        }
        let mut lines = Self {
            out: Rewrite::of(markdown),
            fence: Fence::default(),
            blanks: 0,
            in_definition: false,
            shift: None,
        };
        for line in markdown.split_inclusive('\n') {
            lines.line(line);
        }
        lines.out.finish(markdown)
    }

    fn line(&mut self, line: &str) {
        let shift = self.take_shift(line);
        if self.fence.read(line) {
            self.blanks = 0;
            self.keep(line, shift);
            return;
        }
        let text = line.trim_end_matches(['\n', '\r']);
        let lead = Lead::of(text);
        let rest = &text[lead.len..];
        if rest.trim().is_empty() {
            self.blanks += 1;
            self.keep(line, shift);
            return;
        }
        let indent = lead.indent;
        let marker = indent <= 3
            && rest
                .strip_prefix(':')
                .is_some_and(|after| after.is_empty() || after.starts_with([' ', '\t']));
        if marker && self.blanks >= 2 && !self.in_definition {
            self.out.keep(&line[..lead.len]);
            self.out.insert("\\");
            self.out.keep(&line[lead.len..]);
        } else if marker {
            let first_line_empty = rest[1..].trim().is_empty();
            let quoted = lead.quoted;
            if first_line_empty && !quoted && self.blanks == 0 && self.out.read > 0 && indent < 2 {
                self.shift = Some((2 - indent, false));
            }
            self.in_definition = true;
            self.keep(line, shift);
        } else {
            if self.blanks > 0 && indent == 0 {
                self.in_definition = false;
            }
            self.keep(line, shift);
        }
        self.blanks = 0;
    }

    /// The spaces to take off `line`, if it's indented by four or more in a
    /// definition with nothing on its first line, which ends at a line
    /// indented by less than two, or a blank line right after the first.
    fn take_shift(&mut self, line: &str) -> usize {
        let Some((by, started)) = self.shift else {
            return 0;
        };
        if line.trim().is_empty() {
            if !started {
                self.shift = None;
            }
            return 0;
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent < 2 {
            self.shift = None;
            return 0;
        }
        self.shift = Some((by, true));
        if indent >= 4 { by } else { 0 }
    }

    /// Keeps `line`, less `shift` of the four spaces it starts with.
    fn keep(&mut self, line: &str, shift: usize) {
        if shift == 0 {
            self.out.keep(line);
            return;
        }
        self.out.replace(4, &"    "[shift..]);
        self.out.keep(&line[4..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_lines_rewrite_escapes_a_definition_far_from_its_terms() {
        let src = "a\n\n\n: b\n\n> c\n>\n>\n>  : d\n\ne\n\n: f\n\n\n: g\n\n    : h\n";
        let (text, offsets) = DefinitionLines::rewrite(src);
        // Not after one blank line, nor after a definition, nor as code.
        assert_eq!(
            text,
            "a\n\n\n\\: b\n\n> c\n>\n>\n>  \\: d\n\ne\n\n: f\n\n\n: g\n\n    : h\n"
        );
        assert_eq!(offsets.was(src, &text, "\\: b"), ": b");
        offsets.assert_fit(src, &text);
        let (text, _) = DefinitionLines::rewrite("```\na\n\n\n: b\n```\n");
        assert!(matches!(text, Cow::Borrowed(_)));
    }

    #[test]
    fn definition_lines_rewrite_indents_a_definition_empty_on_its_first_line_by_four() {
        let src = "a\n: \n        b\n   c\n\n      d\ne\n        f\n";
        let (text, offsets) = DefinitionLines::rewrite(src);
        // Only the lines indented by four or more, until one by less than two.
        assert_eq!(text, "a\n: \n      b\n   c\n\n    d\ne\n        f\n");
        assert_eq!(offsets.was(src, &text, "      b"), "        b");
        offsets.assert_fit(src, &text);
        // By one with the `:` indented by one, and not at all after a blank
        // line, which ends the definition, or with something on the line.
        for (src, rewritten) in [
            ("a\n : \n     b\n", "a\n : \n    b\n"),
            ("a\n: \n\n    b\n", "a\n: \n\n    b\n"),
            ("a\n: x\n    b\n", "a\n: x\n    b\n"),
        ] {
            assert_eq!(DefinitionLines::rewrite(src).0, rewritten, "{src:?}");
        }
    }
}

use std::borrow::Cow;

use super::{Fence, HtmlTag, Offsets, Rewrite};

/// kramdown parses Markdown inside an HTML element marked `markdown="1"`.
/// CommonMark only does so when blank lines separate the tags from the
/// Markdown, so this adds them.
pub(crate) struct MarkdownElements {
    out: Rewrite,
    /// The elements still open, innermost last.
    open: Vec<Element>,
    fence: Fence,
}

/// An open `markdown="1"` element.
struct Element {
    name: String,
    /// How many elements of the same name are open inside it.
    depth: usize,
}

impl MarkdownElements {
    /// `markdown`, with blank lines around what's inside each
    /// `markdown="1"` element, and the offsets of the lines added.
    pub fn separate(markdown: &str) -> (Cow<'_, str>, Offsets) {
        if !markdown.contains("markdown=\"1\"") {
            return (Cow::Borrowed(markdown), Offsets::default());
        }
        let mut elements = Self {
            out: Rewrite::of(markdown),
            open: Vec::new(),
            fence: Fence::default(),
        };
        for line in markdown.split_inclusive('\n') {
            elements.line(line);
        }
        elements.out.finish(markdown)
    }

    fn line(&mut self, line: &str) {
        if self.fence.read(line) {
            self.out.keep(line);
            return;
        }
        let trimmed = line.trim();
        if self.closes(trimmed) {
            self.out.insert("\n");
            self.out.keep(line);
            return;
        }
        self.out.keep(line);
        if let Some(name) = Element::opened_by(trimmed) {
            if !line.ends_with('\n') {
                self.out.insert("\n");
            }
            self.out.insert("\n");
            self.open.push(Element { name, depth: 0 });
        }
    }

    /// Whether `trimmed` closes the innermost open element, keeping count of
    /// elements of the same name opened inside it.
    fn closes(&mut self, trimmed: &str) -> bool {
        let Some(element) = self.open.last_mut() else {
            return false;
        };
        match HtmlTag::starting(trimmed) {
            Some(tag) if tag.name == element.name && tag.closing => {
                if element.depth == 0 {
                    self.open.pop();
                    return true;
                }
                element.depth -= 1;
            }
            // One closed on the same line, like `<div>x</div>`, isn't open.
            Some(tag) if tag.name == element.name && !Self::closed_in(trimmed, tag.name) => {
                element.depth += 1;
            }
            _ => {}
        }
        false
    }

    /// Whether `trimmed` has a closing tag named `name` in it.
    fn closed_in(trimmed: &str, name: &str) -> bool {
        trimmed
            .match_indices("</")
            .any(|(i, _)| trimmed[i + "</".len()..].starts_with(name))
    }

    /// `html` without the `markdown="1"` in its tags, which kramdown reads
    /// and leaves out, or `None` if there's none to leave out.
    pub fn without_attribute(html: &str) -> Option<String> {
        const ATTRIBUTE: &str = "markdown=\"1\"";
        let mut out = String::new();
        let mut kept = 0;
        for (i, _) in html.match_indices(ATTRIBUTE) {
            let before = &html[..i];
            let in_tag = before.rfind('<') > before.rfind('>');
            let start = before
                .trim_end_matches(|c: char| c.is_ascii_whitespace())
                .len();
            if in_tag && start < i {
                out.push_str(&html[kept..start]);
                kept = i + ATTRIBUTE.len();
            }
        }
        if kept == 0 {
            return None;
        }
        out.push_str(&html[kept..]);
        Some(out)
    }
}

impl Element {
    /// The name of the `markdown="1"` element `trimmed` opens, if it's a line
    /// of only that tag.
    fn opened_by(trimmed: &str) -> Option<String> {
        if !(trimmed.ends_with('>') && trimmed.contains("markdown=\"1\"")) {
            return None;
        }
        let tag = HtmlTag::starting(trimmed).filter(|tag| !tag.closing)?;
        Some(tag.name.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_elements_separate_splits_markdown_from_tags() {
        let src = "<div markdown=\"1\">\n```rust\n</div>\n```\n<div>\n</div>\n</div>\nafter\n";
        assert_eq!(
            MarkdownElements::separate(src).0,
            "<div markdown=\"1\">\n\n```rust\n</div>\n```\n<div>\n</div>\n\n</div>\nafter\n"
        );
        // Elements closed on their own line, or only alike in their start,
        // don't nest.
        let src = "<p markdown=\"1\">\n<p>x</p>\n<pre>\n</pre>\n</p>\n";
        assert_eq!(
            MarkdownElements::separate(src).0,
            "<p markdown=\"1\">\n\n<p>x</p>\n<pre>\n</pre>\n\n</p>\n"
        );
        // A fence only closes with as many marks.
        let src = "<div markdown=\"1\">\n````\n```\n</div>\n````\n</div>\n";
        assert_eq!(
            MarkdownElements::separate(src).0,
            "<div markdown=\"1\">\n\n````\n```\n</div>\n````\n\n</div>\n"
        );
        // Nothing to separate in a fence.
        let (text, offsets) = MarkdownElements::separate("```\n<div markdown=\"1\">\n```\n");
        assert!(matches!(text, Cow::Borrowed(_)));
        assert!(offsets.is_empty());
    }

    #[test]
    fn markdown_elements_without_attribute_takes_it_only_out_of_tags() {
        assert_eq!(
            MarkdownElements::without_attribute("<div class=\"a\"\n  markdown=\"1\" id=\"b\">\n"),
            Some("<div class=\"a\" id=\"b\">\n".into())
        );
        assert_eq!(
            MarkdownElements::without_attribute("<p markdown=\"1\">a</p><p markdown=\"1\">"),
            Some("<p>a</p><p>".into())
        );
        // Not in text, nor as part of another attribute.
        for html in ["<pre>x markdown=\"1\"</pre>", "<p data-markdown=\"1\">"] {
            assert_eq!(MarkdownElements::without_attribute(html), None, "{html}");
        }
    }

    #[test]
    fn markdown_elements_separate_gives_an_element_ending_the_input_its_blank_line() {
        let src = "<div markdown=\"1\">";
        let (text, offsets) = MarkdownElements::separate(src);
        assert_eq!(text, "<div markdown=\"1\">\n\n");
        // The added lines are where the input ends.
        assert_eq!(
            offsets.original(src.len()..text.len()),
            src.len()..src.len()
        );
        offsets.assert_fit(src, &text);
    }
}

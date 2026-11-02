//! Rewrites the few things pulldown-cmark's CommonMark parser reads
//! differently from kramdown, before it reads them.

use std::borrow::Cow;

/// kramdown parses Markdown inside an HTML element marked `markdown="1"`.
/// CommonMark only does so when blank lines separate the tags from the
/// Markdown, so this adds them.
pub(crate) struct MarkdownElements {
    out: String,
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
    /// `markdown="1"` element.
    pub fn separate(markdown: &str) -> Cow<'_, str> {
        if !markdown.contains("markdown=\"1\"") {
            return Cow::Borrowed(markdown);
        }
        let mut elements = Self {
            out: String::with_capacity(markdown.len() + 64),
            open: Vec::new(),
            fence: Fence::default(),
        };
        for line in markdown.split_inclusive('\n') {
            elements.line(line);
        }
        Cow::Owned(elements.out)
    }

    fn line(&mut self, line: &str) {
        if self.fence.read(line) {
            self.out.push_str(line);
            return;
        }
        let trimmed = line.trim();
        if self.closes(trimmed) {
            self.out.push('\n');
            self.out.push_str(line);
            return;
        }
        self.out.push_str(line);
        if let Some(name) = Element::opened_by(trimmed) {
            if !line.ends_with('\n') {
                self.out.push('\n');
            }
            self.out.push('\n');
            self.open.push(Element { name, depth: 0 });
        }
    }

    /// Whether `trimmed` closes the innermost open element, keeping count of
    /// elements of the same name opened inside it.
    fn closes(&mut self, trimmed: &str) -> bool {
        let Some(element) = self.open.last_mut() else {
            return false;
        };
        match Tag::starting(trimmed) {
            Some(tag) if tag.name == element.name && tag.closing => {
                if element.depth == 0 {
                    self.open.pop();
                    return true;
                }
                element.depth -= 1;
            }
            // One closed on the same line, like `<div>x</div>`, isn't open.
            Some(tag)
                if tag.name == element.name && !trimmed.contains(&format!("</{}", tag.name)) =>
            {
                element.depth += 1;
            }
            _ => {}
        }
        false
    }
}

impl Element {
    /// The name of the `markdown="1"` element `trimmed` opens, if it's a line
    /// of only that tag.
    fn opened_by(trimmed: &str) -> Option<String> {
        if !(trimmed.ends_with('>') && trimmed.contains("markdown=\"1\"")) {
            return None;
        }
        let tag = Tag::starting(trimmed).filter(|tag| !tag.closing)?;
        Some(tag.name.to_owned())
    }
}

/// The tag at the start of a line.
struct Tag<'a> {
    name: &'a str,
    /// Whether it's a closing tag, like `</div>`.
    closing: bool,
}

impl<'a> Tag<'a> {
    /// The tag `trimmed` starts with, if it starts with one.
    fn starting(trimmed: &'a str) -> Option<Self> {
        let rest = trimmed.strip_prefix('<')?;
        let (rest, closing) = match rest.strip_prefix('/') {
            Some(rest) => (rest, true),
            None => (rest, false),
        };
        let len = rest
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(rest.len());
        (len > 0).then(|| Self {
            name: &rest[..len],
            closing,
        })
    }
}

/// kramdown reads a line of only a span-level tag, like `<img>` or `<span>`,
/// as the start of a paragraph, Markdown on the lines after it included.
/// CommonMark starts an HTML block there instead, which runs to the next
/// blank line and leaves any Markdown in it as text. Joining the next line
/// onto the tag's makes the same paragraph of it.
pub(crate) struct SpanTags;

impl SpanTags {
    /// The span-level elements kramdown keeps in paragraphs.
    const NAMES: [&str; 38] = [
        "a", "abbr", "acronym", "b", "bdo", "big", "br", "button", "cite", "code", "del", "dfn",
        "em", "i", "img", "input", "ins", "kbd", "label", "mark", "option", "q", "rb", "rbc", "rp",
        "rt", "rtc", "ruby", "samp", "select", "small", "span", "strong", "sub", "sup", "textarea",
        "tt", "var",
    ];

    /// `markdown`, with each line of only a span tag joined onto the next
    /// line, when that's text.
    pub fn join(markdown: &str) -> Cow<'_, str> {
        let mut out = String::new();
        let mut joined = false;
        let mut fence = Fence::default();
        let mut lines = markdown.split_inclusive('\n').peekable();
        while let Some(line) = lines.next() {
            if fence.read(line) {
                out.push_str(line);
                continue;
            }
            let next_is_text = lines.peek().is_some_and(|next| {
                let next = next.trim();
                !next.is_empty() && !next.starts_with("```") && !next.starts_with("~~~")
            });
            if next_is_text && Self::alone(line) {
                out.push_str(line.trim_end());
                out.push(' ');
                joined = true;
            } else {
                out.push_str(line);
            }
        }
        if joined {
            Cow::Owned(out)
        } else {
            Cow::Borrowed(markdown)
        }
    }

    /// Whether `line` is only a span-level tag, opening or closing, and not
    /// indented enough to be code.
    fn alone(line: &str) -> bool {
        let indent = line.len() - line.trim_start().len();
        let Some(tag) = line
            .trim()
            .strip_prefix('<')
            .and_then(|tag| tag.strip_suffix('>'))
            .filter(|tag| indent < 4 && !tag.contains(['<', '>']))
        else {
            return false;
        };
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        Self::NAMES.contains(&name.as_str())
    }
}

/// Keeps track of fenced code blocks, where nothing is kramdown.
#[derive(Default)]
struct Fence {
    /// The character and length of the open fence, if inside one.
    open: Option<(char, usize)>,
}

impl Fence {
    /// Reads `line`, returning whether it's part of a fenced code block,
    /// fences included. A fence closes at a line of only as many of its
    /// characters or more.
    fn read(&mut self, line: &str) -> bool {
        let trimmed = line.trim();
        let run = |c: char| trimmed.len() - trimmed.trim_start_matches(c).len();
        if let Some((c, len)) = self.open {
            if run(c) >= len && trimmed.trim_start_matches(c).is_empty() {
                self.open = None;
            }
            return true;
        }
        for c in ['`', '~'] {
            let len = run(c);
            if len >= 3 {
                self.open = Some((c, len));
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_markdown_from_tags() {
        let src = "<div markdown=\"1\">\n```rust\n</div>\n```\n<div>\n</div>\n</div>\nafter\n";
        assert_eq!(
            MarkdownElements::separate(src),
            "<div markdown=\"1\">\n\n```rust\n</div>\n```\n<div>\n</div>\n\n</div>\nafter\n"
        );
        // Elements closed on their own line, or only alike in their start,
        // don't nest.
        let src = "<p markdown=\"1\">\n<p>x</p>\n<pre>\n</pre>\n</p>\n";
        assert_eq!(
            MarkdownElements::separate(src),
            "<p markdown=\"1\">\n\n<p>x</p>\n<pre>\n</pre>\n\n</p>\n"
        );
        // A fence only closes with as many marks.
        let src = "<div markdown=\"1\">\n````\n```\n</div>\n````\n</div>\n";
        assert_eq!(
            MarkdownElements::separate(src),
            "<div markdown=\"1\">\n\n````\n```\n</div>\n````\n\n</div>\n"
        );
    }

    #[test]
    fn span_tags_start_paragraphs() {
        assert_eq!(
            SpanTags::join("<img src=\"a\" />\n![b](c)\n"),
            "<img src=\"a\" /> ![b](c)\n"
        );
        assert_eq!(
            SpanTags::join("<span class=\"g\">\n![a](b)\n![c](d)\n</span>\n"),
            "<span class=\"g\"> ![a](b)\n![c](d)\n</span>\n"
        );
        // Not block elements, tags before a blank line or a fence, or code.
        let src = "<div>\n*a*\n</div>\n\n<img />\n\n<b>\n```\n<b>\nx\n```\n    <b>\n    x\n";
        assert!(matches!(SpanTags::join(src), Cow::Borrowed(_)));
    }

    #[test]
    fn span_tags_ignore_case_and_may_close() {
        assert_eq!(SpanTags::join("<IMG src=x>\na\n"), "<IMG src=x> a\n");
        assert_eq!(SpanTags::join("</span>\na\n"), "</span> a\n");
        // Not two tags on a line.
        assert!(matches!(SpanTags::join("<span><b>\na\n"), Cow::Borrowed(_)));
    }

    #[test]
    fn a_markdown_element_ending_the_input_gets_its_blank_line() {
        assert_eq!(
            MarkdownElements::separate("<div markdown=\"1\">"),
            "<div markdown=\"1\">\n\n"
        );
    }

    #[test]
    fn fences_close_only_with_their_own_marks() {
        let mut fence = Fence::default();
        assert!(!fence.read("a\n"));
        assert!(fence.read("~~~ruby\n"));
        assert!(fence.read("```\n"));
        // Not with more after the marks.
        assert!(fence.read("~~~ x\n"));
        assert!(fence.read("~~~~\n"));
        assert!(!fence.read("b\n"));
        // Not with fewer marks.
        assert!(fence.read("````\n"));
        assert!(fence.read("```\n"));
        assert!(fence.read("````\n"));
        assert!(!fence.read("c\n"));
    }
}

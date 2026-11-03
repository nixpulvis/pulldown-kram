use std::borrow::Cow;

use super::{Fence, Offsets, Rewrite};

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
    /// line, when that's text, and the offsets of the joins.
    pub fn join(markdown: &str) -> (Cow<'_, str>, Offsets) {
        let mut out = Rewrite::of(markdown);
        let mut fence = Fence::default();
        let mut lines = markdown.split_inclusive('\n').peekable();
        while let Some(line) = lines.next() {
            if fence.read(line) {
                out.keep(line);
                continue;
            }
            let next_is_text = lines.peek().is_some_and(|next| {
                let next = next.trim();
                !next.is_empty() && !next.starts_with("```") && !next.starts_with("~~~")
            });
            if next_is_text && Self::alone(line) {
                let tag = line.trim_end();
                out.keep(tag);
                out.replace(line.len() - tag.len(), " ");
            } else {
                out.keep(line);
            }
        }
        out.finish(markdown)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_tags_join_starts_paragraphs_with_span_tags() {
        assert_eq!(
            SpanTags::join("<img src=\"a\" />\n![b](c)\n").0,
            "<img src=\"a\" /> ![b](c)\n"
        );
        assert_eq!(
            SpanTags::join("<span class=\"g\">\n![a](b)\n![c](d)\n</span>\n").0,
            "<span class=\"g\"> ![a](b)\n![c](d)\n</span>\n"
        );
        // Not block elements, tags before a blank line or a fence, or code.
        let src = "<div>\n*a*\n</div>\n\n<img />\n\n<b>\n```\n<b>\nx\n```\n    <b>\n    x\n";
        let (text, offsets) = SpanTags::join(src);
        assert!(matches!(text, Cow::Borrowed(_)));
        assert!(offsets.is_empty());
    }

    #[test]
    fn span_tags_join_ignores_case_and_joins_closing_tags() {
        assert_eq!(SpanTags::join("<IMG src=x>\na\n").0, "<IMG src=x> a\n");
        assert_eq!(SpanTags::join("</span>\na\n").0, "</span> a\n");
        // Not two tags on a line.
        assert!(matches!(
            SpanTags::join("<span><b>\na\n").0,
            Cow::Borrowed(_)
        ));
    }
}

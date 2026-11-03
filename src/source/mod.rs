//! The rewrites made to the Markdown before pulldown-cmark reads it,
//! wherever kramdown reads a line differently from CommonMark, and the
//! offsets that map the rewritten text back to the original.
//!
//! - [`StandIns`] replaces syntax pulldown-cmark can't read at all with
//!   stand-ins, which the reader swaps back, and puts a header row in each
//!   table that has none.
//! - [`MarkdownElements`] separates the content of `markdown="1"` elements
//!   from their tags.
//! - [`HtmlBlocks`] ends HTML blocks at their end tags.
//! - [`SpanTags`] joins a line of only a span tag onto the next line.
//! - [`AttributeLines`] separates a line of attribute lists from code or a
//!   link definition right after it.
//! - [`DefinitionLines`] fixes the blank lines and indentation of
//!   definitions.

mod attribute_lines;
mod definition_lines;
mod html_blocks;
mod markdown_elements;
mod offsets;
mod span_tags;
mod stand_ins;

use std::borrow::Cow;

use attribute_lines::AttributeLines;
use definition_lines::DefinitionLines;
use html_blocks::HtmlBlocks;
pub(crate) use markdown_elements::MarkdownElements;
pub(crate) use offsets::Offsets;
use offsets::Rewrite;
use span_tags::SpanTags;
pub(crate) use stand_ins::{Mark, Marked, StandIn, StandIns};

use crate::Extensions;
use crate::text::Lead;

/// A rewrite of some Markdown, giving the new text and the offsets of its
/// edits, or the Markdown itself, borrowed, when there's nothing to change.
type Rewriter = for<'s> fn(&'s str) -> (Cow<'s, str>, Offsets);

/// The rewrites made after the stand-ins go in, in the order they're made,
/// each with the extension it's for. Each reads the text the one before it
/// wrote.
pub(crate) const REWRITES: [(Extensions, Rewriter); 5] = [
    (
        Extensions::ENABLE_MARKDOWN_ATTRIBUTE,
        MarkdownElements::rewrite,
    ),
    (Extensions::ENABLE_HTML_BLOCK_ENDS, HtmlBlocks::rewrite),
    (Extensions::ENABLE_SPAN_TAG_PARAGRAPHS, SpanTags::rewrite),
    (Extensions::ENABLE_BLOCK_ATTRIBUTES, AttributeLines::rewrite),
    (
        Extensions::ENABLE_DEFINITION_LISTS,
        DefinitionLines::rewrite,
    ),
];

/// The HTML tag at the start of some text.
struct HtmlTag<'a> {
    name: &'a str,
    /// Whether it's a closing tag, like `</div>`.
    closing: bool,
}

impl<'a> HtmlTag<'a> {
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

/// Keeps track of fenced code blocks, where nothing is kramdown.
///
/// A fence is only one when it's indented by less than four, past any quote
/// markers, or past the content of the list item it may be in. Indented by
/// more, it's a line of an indented code block.
#[derive(Default)]
struct Fence {
    /// The character and length of the open fence, if inside one.
    open: Option<(char, usize)>,
    /// The column the content of the list item the lines may be in starts
    /// at, or `0` outside one.
    item: usize,
    /// Whether the last line was blank.
    after_blank: bool,
}

impl Fence {
    /// Reads `line`, returning whether it's part of a fenced code block,
    /// fences included. A fence closes at a line of only as many of its
    /// characters or more.
    fn read(&mut self, line: &str) -> bool {
        let text = line.trim_end();
        let lead = Lead::of(text);
        let mut indent = lead.indent;
        let mut rest = &text[lead.len..];
        if let Some((c, len)) = self.open {
            let run = rest.len() - rest.trim_start_matches(c).len();
            let only_marks = rest.trim_start_matches(c).trim().is_empty();
            if indent < self.item + 4 && run >= len && only_marks {
                self.open = None;
            }
            return true;
        }
        if rest.is_empty() {
            self.after_blank = true;
            return false;
        }
        if std::mem::take(&mut self.after_blank) && indent < self.item {
            self.item = 0;
        }
        if indent < self.item + 4
            && let Some(width) = Self::item_marker(rest)
        {
            self.item = indent + width;
            indent += width;
            rest = &rest[width..];
        }
        if indent >= self.item + 4 {
            return false;
        }
        for c in ['`', '~'] {
            let len = rest.len() - rest.trim_start_matches(c).len();
            if len >= 3 {
                self.open = Some((c, len));
                return true;
            }
        }
        false
    }

    /// How wide the list item marker `text` starts with is, like `* ` or
    /// `1. `, with the spaces after it, if it starts with one.
    fn item_marker(text: &str) -> Option<usize> {
        let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let marker = if digits > 0 {
            text[digits..].strip_prefix(['.', ')'])?;
            digits + 1
        } else {
            text.strip_prefix(['*', '-', '+'])?;
            1
        };
        let after = &text[marker..];
        let spaces = after.len() - after.trim_start_matches(' ').len();
        match spaces {
            0 if after.is_empty() => Some(marker),
            0 => None,
            // Indented code after the marker starts one column past it.
            5.. => Some(marker + 1),
            _ => Some(marker + spaces),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_read_closes_only_with_its_own_marks() {
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

    #[test]
    fn fence_read_skips_indented_code() {
        let mut fence = Fence::default();
        for line in [
            "a\n",
            "\n",
            "    ```\n",
            "\n",
            "*[b]: c\n",
            ">     ```\n",
            "\t```\n",
        ] {
            assert!(!fence.read(line), "{line:?}");
        }
        // Inside a fence, a line indented as code doesn't close it.
        assert!(fence.read("```\n"));
        assert!(fence.read("    ```\n"));
        assert!(fence.read("   ```\n"));
        assert!(!fence.read("d\n"));
    }

    #[test]
    fn fence_read_follows_list_items() {
        let mut fence = Fence::default();
        assert!(!fence.read("1. a\n"));
        assert!(!fence.read("\n"));
        assert!(fence.read("    ```\n"));
        assert!(fence.read("    x\n"));
        assert!(fence.read("    ```\n"));
        // On the marker's line too.
        assert!(fence.read("- ```\n"));
        assert!(fence.read("  ```\n"));
        // Past the item, it's code again.
        assert!(!fence.read("\n"));
        assert!(!fence.read("b\n"));
        assert!(!fence.read("\n"));
        assert!(!fence.read("    ```\n"));
    }
}

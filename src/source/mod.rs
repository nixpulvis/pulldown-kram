//! Rewrites the few things pulldown-cmark's CommonMark parser reads
//! differently from kramdown, before it reads them, keeping track of where
//! the rewritten text was in the original.

mod attribute_lines;
mod definition_lines;
mod html_blocks;
mod markdown_elements;
mod offsets;
mod span_tags;
mod stand_ins;

pub(crate) use attribute_lines::AttributeLines;
pub(crate) use definition_lines::DefinitionLines;
pub(crate) use html_blocks::HtmlBlocks;
pub(crate) use markdown_elements::MarkdownElements;
pub(crate) use offsets::Offsets;
use offsets::Rewrite;
pub(crate) use span_tags::SpanTags;
pub(crate) use stand_ins::{Mark, Marked, StandIn, StandIns};

/// The tag at the start of a line.
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
        let lead = text.len() - text.trim_start_matches([' ', '>']).len();
        // The spaces after the last quote marker.
        let mut indent = lead - text[..lead].rfind('>').map_or(0, |i| i + 1);
        let mut rest = &text[lead..];
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

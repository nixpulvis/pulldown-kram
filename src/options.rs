//! What to read: pulldown-cmark's options, and which of kramdown's
//! additions to make to them.

use bitflags::bitflags;

/// How to read a [`Document`](crate::Document): pulldown-cmark's options
/// for the CommonMark underneath, and the kramdown extensions to read on
/// top of it.
///
/// pulldown-cmark's options convert into these with every extension on,
/// reading the document as Jekyll does.
///
/// ```
/// use pulldown_kram::{Extensions, Options, pulldown_cmark};
///
/// let mut options = Options::from(pulldown_cmark::Options::ENABLE_TABLES);
/// options.kramdown.remove(Extensions::ENABLE_TABLES_OF_CONTENTS);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// pulldown-cmark's options. `ENABLE_HEADING_ATTRIBUTES` is always left
    /// off, as it reads `{…}` after a heading differently from kramdown.
    pub markdown: pulldown_cmark::Options,
    /// The kramdown extensions to read.
    pub kramdown: Extensions,
}

impl Default for Options {
    /// pulldown-cmark's defaults, with every extension.
    fn default() -> Self {
        pulldown_cmark::Options::empty().into()
    }
}

impl From<pulldown_cmark::Options> for Options {
    /// `markdown` with every extension.
    fn from(markdown: pulldown_cmark::Options) -> Self {
        Self {
            markdown,
            kramdown: Extensions::all(),
        }
    }
}

bitflags! {
    /// kramdown's additions to Markdown, each of which can be left off.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Extensions: u32 {
        /// Attribute lists for blocks, like `{:.note}` on the line after a
        /// paragraph, or alone between blocks for the one after.
        const ENABLE_BLOCK_ATTRIBUTES = 1 << 0;
        /// Attribute lists right after a span, like `*a*{:.x}`, and the empty
        /// list, `{::}`, separating spans.
        const ENABLE_SPAN_ATTRIBUTES = 1 << 1;
        /// Attribute lists at the start of a list item, like `* {:.x} a`.
        const ENABLE_ITEM_ATTRIBUTES = 1 << 2;
        /// Header ids written after a heading's text, `# Hello {#hi}`.
        const ENABLE_HEADER_IDS = 1 << 3;
        /// Ids made from the text of headings without one, the way
        /// kramdown's GFM parser makes them. kramdown's `auto_ids` option.
        const ENABLE_AUTO_IDS = 1 << 4;
        /// Markdown inside HTML elements marked `markdown="1"`.
        const ENABLE_MARKDOWN_ATTRIBUTE = 1 << 5;
        /// A line of only a span tag, like `<img>`, starting a paragraph
        /// rather than an HTML block.
        const ENABLE_SPAN_TAG_PARAGRAPHS = 1 << 6;
        /// A code block's language from a `language-` class, like
        /// `{: .language-ruby}`, when its fence gives none.
        const ENABLE_LANGUAGE_CLASSES = 1 << 7;
        /// Lists marked `{:toc}` made into tables of contents, linking to
        /// each heading with an id and without the class `no_toc`. The whole
        /// document is read before any event is given out, for them to link
        /// to headings after them.
        const ENABLE_TABLES_OF_CONTENTS = 1 << 8;
    }
}

impl Extensions {
    /// Whether any kind of attribute list is read.
    pub(crate) fn attribute_lists(self) -> bool {
        self.intersects(
            Self::ENABLE_BLOCK_ATTRIBUTES
                | Self::ENABLE_SPAN_ATTRIBUTES
                | Self::ENABLE_ITEM_ATTRIBUTES,
        )
    }
}

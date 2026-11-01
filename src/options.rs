//! What to read: which of kramdown's extensions, and any more of
//! pulldown-cmark's options.

use bitflags::bitflags;

/// How to read a [`Document`](crate::Document): the kramdown extensions,
/// and any more of pulldown-cmark's options.
///
/// Some extensions are syntax pulldown-cmark reads, like tables, and turn
/// on its option for it. [`Options::parser_options`] is what pulldown-cmark
/// is given in the end: the options in `cmark`, and those the extensions
/// need.
///
/// [`Options::new`] names both halves, so everything is
/// `Options::new(Extensions::all(), Cmark::all())` and plain CommonMark is
/// `Options::new(Extensions::empty(), Cmark::empty())`. pulldown-cmark's
/// options and [`Extensions`] each convert into these too. From
/// pulldown-cmark's options, those that read an extension become that
/// extension, and the extensions read here are all on.
///
/// ```
/// use pulldown_kram::{Extensions, Options};
/// use pulldown_kram::pulldown_cmark::Options as Cmark;
///
/// let all = Options::new(Extensions::all(), Cmark::all());
/// assert!(all.parser_options().contains(Cmark::ENABLE_MATH));
///
/// let mut options = Options::from(Cmark::ENABLE_TABLES | Cmark::ENABLE_MATH);
/// assert!(options.kramdown.contains(Extensions::ENABLE_TABLES));
/// assert!(!options.kramdown.contains(Extensions::ENABLE_FOOTNOTES));
/// assert_eq!(options.cmark, Cmark::ENABLE_MATH);
///
/// options.kramdown.remove(Extensions::ENABLE_TABLES);
/// assert!(!options.parser_options().contains(Cmark::ENABLE_TABLES));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The kramdown extensions to read.
    pub kramdown: Extensions,
    /// More of pulldown-cmark's options, beyond those the extensions turn
    /// on.
    pub cmark: pulldown_cmark::Options,
}

impl Options {
    /// The extensions in `kramdown`, read with pulldown-cmark's options in
    /// `cmark` and those the extensions need.
    ///
    /// The options in `cmark` are given to pulldown-cmark as they are.
    /// `Cmark::all()` includes `ENABLE_OLD_FOOTNOTES`, which reads footnotes
    /// the way pulldown-cmark used to rather than as kramdown does.
    pub fn new(kramdown: Extensions, cmark: pulldown_cmark::Options) -> Self {
        Self { kramdown, cmark }
    }

    /// The options pulldown-cmark reads with: those in `cmark` and those the
    /// extensions need, less `ENABLE_HEADING_ATTRIBUTES`, which reads `{…}`
    /// after a heading differently from kramdown.
    pub fn parser_options(&self) -> pulldown_cmark::Options {
        (self.cmark | self.kramdown.cmark_options())
            - pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES
    }
}

impl From<pulldown_cmark::Options> for Options {
    /// `cmark`, with the extensions it reads taken out into `kramdown`,
    /// along with every extension read here.
    fn from(cmark: pulldown_cmark::Options) -> Self {
        let kramdown = Extensions::from_cmark_options(cmark);
        Self {
            kramdown,
            cmark: cmark - kramdown.cmark_options(),
        }
    }
}

impl From<Extensions> for Options {
    /// `kramdown`, with only the options it needs from pulldown-cmark.
    fn from(kramdown: Extensions) -> Self {
        Self {
            kramdown,
            cmark: pulldown_cmark::Options::empty(),
        }
    }
}

bitflags! {
    /// kramdown's syntax beyond CommonMark, each part of which can be left
    /// off. The first few are read by pulldown-cmark, with the option of
    /// the same name, and the rest here.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Extensions: u32 {
        /// Tables, with a header row.
        const ENABLE_TABLES = 1 << 0;
        /// Footnotes, `[^1]`, with blocks in their definitions.
        const ENABLE_FOOTNOTES = 1 << 1;
        /// `~~Strikethrough~~`, from kramdown's GFM parser.
        const ENABLE_STRIKETHROUGH = 1 << 2;
        /// Task list items, `* [x] done`, from kramdown's GFM parser.
        const ENABLE_TASK_LISTS = 1 << 3;
        /// Dashes, ellipses and smart quotes, pulldown-cmark's smart
        /// punctuation. Header ids are made from the text without them, as in
        /// kramdown, so they need this to match kramdown's.
        const ENABLE_TYPOGRAPHY = 1 << 4;
        /// Attribute lists for blocks, like `{:.note}` on the line after a
        /// paragraph, or alone between blocks for the one after.
        const ENABLE_BLOCK_ATTRIBUTES = 1 << 5;
        /// Attribute lists right after a span, like `*a*{:.x}`, and the empty
        /// list, `{::}`, separating spans.
        const ENABLE_SPAN_ATTRIBUTES = 1 << 6;
        /// Attribute lists at the start of a list item, like `* {:.x} a`.
        const ENABLE_ITEM_ATTRIBUTES = 1 << 7;
        /// Header ids written after a heading's text, `# Hello {#hi}`.
        const ENABLE_HEADER_IDS = 1 << 8;
        /// Ids made from the text of headings without one, the way
        /// kramdown's GFM parser makes them. kramdown's `auto_ids` option.
        const ENABLE_AUTO_IDS = 1 << 9;
        /// Markdown inside HTML elements marked `markdown="1"`.
        const ENABLE_MARKDOWN_ATTRIBUTE = 1 << 10;
        /// A line of only a span tag, like `<img>`, starting a paragraph
        /// rather than an HTML block.
        const ENABLE_SPAN_TAG_PARAGRAPHS = 1 << 11;
        /// A code block's language from a `language-` class, like
        /// `{: .language-ruby}`, when its fence gives none. The class comes
        /// from a block attribute list, so this needs those.
        const ENABLE_LANGUAGE_CLASSES = 1 << 12;
        /// Lists marked `{:toc}` made into tables of contents, linking to
        /// each heading with an id and without the class `no_toc`. The mark
        /// is a block attribute list, so this needs those. The whole
        /// document is read before any event is given out, for them to link
        /// to headings after them.
        const ENABLE_TABLES_OF_CONTENTS = 1 << 13;
    }
}

impl Extensions {
    /// The extensions pulldown-cmark reads, and its option for each.
    const CMARK: [(Self, pulldown_cmark::Options); 5] = [
        (Self::ENABLE_TABLES, pulldown_cmark::Options::ENABLE_TABLES),
        (
            Self::ENABLE_FOOTNOTES,
            pulldown_cmark::Options::ENABLE_FOOTNOTES,
        ),
        (
            Self::ENABLE_STRIKETHROUGH,
            pulldown_cmark::Options::ENABLE_STRIKETHROUGH,
        ),
        (
            Self::ENABLE_TASK_LISTS,
            pulldown_cmark::Options::ENABLE_TASKLISTS,
        ),
        (
            Self::ENABLE_TYPOGRAPHY,
            pulldown_cmark::Options::ENABLE_SMART_PUNCTUATION,
        ),
    ];

    /// The extensions `options` reads, and every extension read here.
    pub fn from_cmark_options(options: pulldown_cmark::Options) -> Self {
        Self::CMARK
            .into_iter()
            .fold(Self::all(), |extensions, (extension, option)| {
                if options.contains(option) {
                    extensions
                } else {
                    extensions - extension
                }
            })
    }

    /// The pulldown-cmark options these extensions are read with.
    pub fn cmark_options(self) -> pulldown_cmark::Options {
        Self::CMARK
            .into_iter()
            .filter(|(extension, _)| self.contains(*extension))
            .fold(pulldown_cmark::Options::empty(), |options, (_, option)| {
                options | option
            })
    }

    /// Whether any kind of attribute list is read.
    pub(crate) fn attribute_lists(self) -> bool {
        self.intersects(
            Self::ENABLE_BLOCK_ATTRIBUTES
                | Self::ENABLE_SPAN_ATTRIBUTES
                | Self::ENABLE_ITEM_ATTRIBUTES,
        )
    }
}

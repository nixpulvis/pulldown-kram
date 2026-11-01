//! What to read: which of kramdown's extensions, and any more of
//! pulldown-cmark's options.

use bitflags::bitflags;

/// How to read a [`Document`](crate::Document): the kramdown extensions,
/// and any of pulldown-cmark's options kramdown doesn't have.
///
/// Every part of kramdown's syntax is an [`Extensions`] flag, so
/// `Extensions::all()` reads what kramdown reads, and converts into these
/// options with nothing more from pulldown-cmark. Some extensions are
/// syntax pulldown-cmark reads, like tables, and turn on its option for it.
/// Those options belong to their extensions, and are on exactly when the
/// extension is, whatever `cmark` says. `cmark` adds what kramdown doesn't
/// read, like GitHub's `> [!NOTE]` quotes.
///
/// ```
/// use pulldown_kram::{Extensions, Options};
/// use pulldown_kram::pulldown_cmark::Options as Cmark;
///
/// // What kramdown reads.
/// let kramdown = Options::from(Extensions::all());
/// assert!(kramdown.parser_options().contains(Cmark::ENABLE_TABLES));
///
/// // With GitHub's quotes too.
/// let more = Options::new(Extensions::all(), Cmark::ENABLE_GFM);
/// assert!(more.parser_options().contains(Cmark::ENABLE_GFM));
///
/// // The extension decides, not `cmark`.
/// let options = Options::new(Extensions::all() - Extensions::ENABLE_TABLES, Cmark::ENABLE_TABLES);
/// assert!(!options.parser_options().contains(Cmark::ENABLE_TABLES));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The kramdown extensions to read.
    pub kramdown: Extensions,
    /// pulldown-cmark's options for what kramdown doesn't read. Those that
    /// read an extension are left to it.
    pub cmark: pulldown_cmark::Options,
}

impl Options {
    /// The extensions in `kramdown`, with pulldown-cmark's options in
    /// `cmark` for anything more.
    pub fn new(kramdown: Extensions, cmark: pulldown_cmark::Options) -> Self {
        Self { kramdown, cmark }
    }

    /// The options pulldown-cmark reads with: those the extensions need,
    /// and the rest of `cmark`.
    ///
    /// An option that reads an extension is only on with the extension, so
    /// `cmark` can't turn tables on without [`Extensions::ENABLE_TABLES`].
    /// `ENABLE_HEADING_ATTRIBUTES` is never on, as it reads `{…}` after a
    /// heading differently from kramdown. `ENABLE_OLD_FOOTNOTES` changes
    /// footnotes only with [`Extensions::ENABLE_FOOTNOTES`].
    pub fn parser_options(&self) -> pulldown_cmark::Options {
        let read =
            Extensions::all().cmark_options() | pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES;
        (self.cmark - read) | self.kramdown.cmark_options()
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
    /// the same name, and the rest here. All of them read what kramdown
    /// reads.
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
        /// Definition lists, a term and then `: definition` lines.
        const ENABLE_DEFINITION_LISTS = 1 << 5;
        /// Math between `$$`, a block when alone in its paragraph and inline
        /// otherwise. A single `$` stays text, as in kramdown.
        const ENABLE_MATH = 1 << 6;
        /// Attribute lists for blocks, like `{:.note}` on the line after a
        /// paragraph, or alone between blocks for the one after.
        const ENABLE_BLOCK_ATTRIBUTES = 1 << 7;
        /// Attribute lists right after a span, like `*a*{:.x}`, and the empty
        /// list, `{::}`, separating spans.
        const ENABLE_SPAN_ATTRIBUTES = 1 << 8;
        /// Attribute lists at the start of a list item, like `* {:.x} a`.
        const ENABLE_ITEM_ATTRIBUTES = 1 << 9;
        /// Header ids written after a heading's text, `# Hello {#hi}`.
        const ENABLE_HEADER_IDS = 1 << 10;
        /// Ids made from the text of headings without one, the way
        /// kramdown's GFM parser makes them. kramdown's `auto_ids` option.
        const ENABLE_AUTO_IDS = 1 << 11;
        /// Markdown inside HTML elements marked `markdown="1"`.
        const ENABLE_MARKDOWN_ATTRIBUTE = 1 << 12;
        /// A line of only a span tag, like `<img>`, starting a paragraph
        /// rather than an HTML block.
        const ENABLE_SPAN_TAG_PARAGRAPHS = 1 << 13;
        /// A code block's language from a `language-` class, like
        /// `{: .language-ruby}`, when its fence gives none. The class comes
        /// from a block attribute list, so this needs those.
        const ENABLE_LANGUAGE_CLASSES = 1 << 14;
        /// Lists marked `{:toc}` made into tables of contents, linking to
        /// each heading with an id and without the class `no_toc`. The mark
        /// is a block attribute list, so this needs those. The whole
        /// document is read before any event is given out, for them to link
        /// to headings after them.
        const ENABLE_TABLES_OF_CONTENTS = 1 << 15;
    }
}

impl Extensions {
    /// The extensions pulldown-cmark reads, and its option for each.
    const CMARK: [(Self, pulldown_cmark::Options); 7] = [
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
        (
            Self::ENABLE_DEFINITION_LISTS,
            pulldown_cmark::Options::ENABLE_DEFINITION_LIST,
        ),
        (Self::ENABLE_MATH, pulldown_cmark::Options::ENABLE_MATH),
    ];

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

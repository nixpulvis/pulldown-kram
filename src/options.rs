//! What to read: which of kramdown's extensions, and which more of
//! pulldown-cmark's options.

use bitflags::bitflags;

/// How to read a [`Document`](crate::Document): the kramdown extensions to
/// turn on, and any of pulldown-cmark's options for syntax kramdown doesn't
/// have.
///
/// `Extensions::all()` reads what kramdown reads, and needs nothing from
/// `cmark`. Some extensions are syntax pulldown-cmark reads too, like
/// tables, and those turn on pulldown-cmark's option for it. That option
/// is on exactly when the extension is, whatever `cmark` says. `cmark` is
/// only for what kramdown doesn't read at all, like GitHub's `> [!NOTE]`
/// quotes.
///
/// So `cmark` never turns on `ENABLE_TABLES`, `ENABLE_FOOTNOTES`,
/// `ENABLE_STRIKETHROUGH`, `ENABLE_TASKLISTS`, `ENABLE_SMART_PUNCTUATION`,
/// `ENABLE_DEFINITION_LIST` or `ENABLE_MATH`, which belong to extensions.
/// Nor does it turn on `ENABLE_HEADING_ATTRIBUTES`, which reads `{…}` after
/// a heading differently from kramdown. `ENABLE_OLD_FOOTNOTES` only changes
/// anything with [`Extensions::ENABLE_FOOTNOTES`] on.
///
/// ```
/// use pulldown_kram::{Document, Extensions, Options};
/// use pulldown_kram::pulldown_cmark::Options as Cmark;
///
/// // What kramdown reads.
/// let kramdown = Document::new("a | b\n-|-\n", Extensions::all());
///
/// // With GitHub's `> [!NOTE]` quotes too.
/// let more = Options::new(Extensions::all(), Cmark::ENABLE_GFM);
/// let document = Document::new("> [!NOTE]\n> a\n", more);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Options {
    /// The kramdown extensions to read.
    pub kramdown: Extensions,
    /// pulldown-cmark's options for syntax kramdown doesn't read. Any that
    /// belong to an extension are ignored here.
    pub cmark: pulldown_cmark::Options,
}

impl Options {
    /// The extensions in `kramdown`, and pulldown-cmark's options in `cmark`
    /// for anything more.
    pub fn new(kramdown: Extensions, cmark: pulldown_cmark::Options) -> Self {
        Self { kramdown, cmark }
    }

    /// The options pulldown-cmark parses with: those the extensions turn on,
    /// and whatever else `cmark` asks for.
    pub(crate) fn parser_options(&self) -> pulldown_cmark::Options {
        let read =
            Extensions::all().cmark_options() | pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES;
        (self.cmark - read) | self.kramdown.cmark_options()
    }
}

impl From<Extensions> for Options {
    /// The extensions in `kramdown`, with no more of pulldown-cmark's
    /// options.
    fn from(kramdown: Extensions) -> Self {
        Self {
            kramdown,
            cmark: pulldown_cmark::Options::empty(),
        }
    }
}

impl Default for Options {
    /// What kramdown reads: every extension, and nothing more from
    /// pulldown-cmark.
    fn default() -> Self {
        Self::from(Extensions::all())
    }
}

bitflags! {
    /// The parts of kramdown's syntax to read, each of which can be left
    /// off.
    ///
    /// Some are syntax CommonMark doesn't have, like attribute lists and
    /// abbreviations. The rest are places where kramdown reads CommonMark's
    /// own syntax differently, like where an HTML block ends. Left off,
    /// each reads as pulldown-cmark reads it.
    ///
    /// The flags come in groups:
    ///
    /// - Syntax pulldown-cmark also reads, from tables to math, which turn
    ///   on its option for it.
    /// - Attribute lists, and the ids and classes that come from them.
    /// - kramdown's own block and span syntax.
    /// - Where kramdown reads CommonMark's syntax differently.
    ///
    /// ```
    /// use pulldown_kram::pulldown_cmark::{Event, Tag};
    /// use pulldown_kram::{Document, Extensions};
    ///
    /// // Without tables of contents, a list marked `{:toc}` stays a list.
    /// let extensions = Extensions::all() - Extensions::ENABLE_TABLES_OF_CONTENTS;
    /// let document = Document::new("# a\n\n* x\n{:toc}\n", extensions);
    /// let starts_list = |(event, _): &_| matches!(event, Event::Start(Tag::List(_)));
    /// assert!(document.events().any(|event| starts_list(&event)));
    /// ```
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Extensions: u32 {
        // Syntax pulldown-cmark also reads.

        /// Tables, with a header row.
        const ENABLE_TABLES = 1 << 0;
        /// Footnotes, `[^1]`, with blocks in their definitions.
        const ENABLE_FOOTNOTES = 1 << 1;
        /// `~~Strikethrough~~`, from kramdown's GFM parser. A single `~`
        /// stays text, as in kramdown.
        const ENABLE_STRIKETHROUGH = 1 << 2;
        /// Task list items, `* [x] done`, from kramdown's GFM parser, with
        /// its classes: `task-list` on the list, and `task-list-item` on
        /// each item from the first task on.
        const ENABLE_TASK_LISTS = 1 << 3;
        /// Dashes, ellipses and smart quotes, from pulldown-cmark's smart
        /// punctuation. kramdown makes header ids from the text without
        /// these, so auto ids only match kramdown's with this on.
        const ENABLE_TYPOGRAPHY = 1 << 4;
        /// Definition lists, terms followed by `: definition` lines.
        const ENABLE_DEFINITION_LISTS = 1 << 5;
        /// Math between `$$`. It's a block when it's alone in its
        /// paragraph, and inline otherwise. A single `$` stays text, as in
        /// kramdown.
        const ENABLE_MATH = 1 << 6;

        // Attribute lists, and what comes from them.

        /// Attribute lists for blocks, like `{:.note}` on the line after a
        /// paragraph. One alone between blocks is for the block after it.
        const ENABLE_BLOCK_ATTRIBUTES = 1 << 7;
        /// Attribute lists right after a span, like `*a*{:.x}`, and the
        /// empty list, `{::}`, which separates two spans.
        const ENABLE_SPAN_ATTRIBUTES = 1 << 8;
        /// Attribute lists at the start of a list item, like `* {:.x} a`.
        const ENABLE_ITEM_ATTRIBUTES = 1 << 9;
        /// Header ids written after a heading's text, like `# Hello {#hi}`.
        const ENABLE_HEADER_IDS = 1 << 10;
        /// Ids for headings that have none, made from their text the way
        /// kramdown's GFM parser makes them. This is kramdown's `auto_ids`
        /// option.
        const ENABLE_AUTO_IDS = 1 << 11;
        /// A code block's language from a `language-` class, like
        /// `{: .language-ruby}`, when its fence gives none. The class comes
        /// from a block attribute list, so this needs those.
        const ENABLE_LANGUAGE_CLASSES = 1 << 12;
        /// Tables of contents. A list marked `{:toc}` is replaced with
        /// links to each heading that has an id and no `no_toc` class. The
        /// mark is a block attribute list, so this needs those. A table can
        /// link to headings after it, so the whole document is read before
        /// the first event comes out.
        ///
        /// Each table lists every heading, so many tables in a long
        /// document could grow with the square of its length. The tables
        /// share a budget of 100,000 items, or as many as the document has
        /// if that's more, which is how pulldown-cmark limits link
        /// reference expansion. Once a table doesn't fit in what's left,
        /// its list and every marked list after it stay as written.
        const ENABLE_TABLES_OF_CONTENTS = 1 << 13;

        // kramdown's own syntax.

        /// The end-of-block marker, a line of only `^`. It ends the blocks
        /// open at it, like a list or a code block, and adds no events.
        const ENABLE_END_OF_BLOCK_MARKERS = 1 << 14;
        /// Hard line breaks from two backslashes at the end of a line, as
        /// well as from two spaces.
        const ENABLE_BACKSLASH_LINE_BREAKS = 1 << 15;
        /// Tables without a header row, like a line of only `| a | b |`.
        /// With this, a `|` inside code in a cell also stays in the cell.
        /// pulldown-cmark reads the tables, so this needs `ENABLE_TABLES`.
        const ENABLE_TABLES_WITHOUT_HEADERS = 1 << 16;
        /// Abbreviations. A definition like
        /// `*[HTML]: HyperText Markup Language` makes each use of the word
        /// in the text an `<abbr>` with that title.
        const ENABLE_ABBREVIATIONS = 1 << 17;
        /// Markdown inside an HTML element marked `markdown="1"`. The
        /// attribute itself is taken out of the HTML.
        const ENABLE_MARKDOWN_ATTRIBUTE = 1 << 18;
        /// kramdown's `comment` extension, `{::comment}…{:/}`, as a block
        /// or a span. Its text comes out as an HTML comment.
        const ENABLE_COMMENTS = 1 << 19;
        /// kramdown's `nomarkdown` extension, `{::nomarkdown}…{:/}`, as a
        /// block or a span. Its text comes out as raw HTML, unless its
        /// `type` names another of kramdown's converters, like `latex`.
        const ENABLE_NOMARKDOWN = 1 << 20;

        // Where kramdown reads CommonMark's syntax differently.

        /// HTML blocks end at the end tag of the element that starts them,
        /// and any text after that tag is read as Markdown. CommonMark's
        /// run to the next blank line. A blank line before the end tag
        /// still ends one, as in CommonMark.
        const ENABLE_HTML_BLOCK_ENDS = 1 << 21;
        /// A line of only a span tag, like `<img>`, starts a paragraph that
        /// takes in the text on the next line, rather than an HTML block.
        /// When a blank line follows the tag it stays an HTML block, where
        /// kramdown would make it a paragraph.
        const ENABLE_SPAN_TAG_PARAGRAPHS = 1 << 22;
        /// The text inside the HTML spans kramdown leaves raw, like `<kbd>`,
        /// `<code>` and `<var>`, comes out as it is, with no Markdown or
        /// typography read in it.
        const ENABLE_RAW_HTML_SPANS = 1 << 23;
        /// Whether a list item's first paragraph is bare is decided for each
        /// item, as kramdown does, rather than for the whole list. It's
        /// bare unless a blank line follows it in the item.
        const ENABLE_LIST_ITEM_PARAGRAPHS = 1 << 24;
        /// A list stays one list whichever of `*`, `+` and `-` marks each
        /// item. CommonMark starts a new list when the marker changes.
        const ENABLE_MIXED_LIST_MARKERS = 1 << 25;
        /// Backslash escapes only for the characters kramdown escapes. The
        /// backslash before any other character is kept, like in `\&`,
        /// where CommonMark escapes all ASCII punctuation.
        const ENABLE_KRAMDOWN_ESCAPES = 1 << 26;
    }
}

impl Extensions {
    /// The extensions pulldown-cmark parses, and its option for each.
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

    /// The pulldown-cmark options these extensions turn on.
    pub(crate) fn cmark_options(self) -> pulldown_cmark::Options {
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

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Options as Cmark;

    #[test]
    fn options_parser_options_kramdown_needs_nothing_more() {
        assert_eq!(
            Options::from(Extensions::all()).parser_options(),
            Cmark::ENABLE_TABLES
                | Cmark::ENABLE_FOOTNOTES
                | Cmark::ENABLE_STRIKETHROUGH
                | Cmark::ENABLE_TASKLISTS
                | Cmark::ENABLE_SMART_PUNCTUATION
                | Cmark::ENABLE_DEFINITION_LIST
                | Cmark::ENABLE_MATH
        );
    }

    #[test]
    fn options_parser_options_cmark_adds_only_what_kramdown_lacks() {
        let options = Options::new(Extensions::empty(), Cmark::all()).parser_options();
        assert!(!options.intersects(Extensions::all().cmark_options()));
        assert!(!options.contains(Cmark::ENABLE_HEADING_ATTRIBUTES));
        assert!(options.contains(Cmark::ENABLE_GFM));
    }
}

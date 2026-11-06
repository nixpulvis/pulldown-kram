//! What to read: which of kramdown's extensions, and any more of
//! pulldown-cmark's options.

use bitflags::bitflags;

/// Parsing options for a [`Document`](crate::Document): which kramdown
/// extensions to enable, plus any extra pulldown-cmark options.
///
/// Each part of kramdown's syntax has an [`Extensions`] flag, so
/// [`Extensions::all()`] matches kramdown, and converts into [`Options`]
/// with no extra pulldown-cmark options. Some extensions, like tables, are
/// implemented by pulldown-cmark and turn on its matching option. Those
/// options are controlled by their extension alone, and are on exactly when
/// it is, regardless of [`cmark`]. Use [`cmark`] for syntax kramdown doesn't
/// have, like GitHub's `> [!NOTE]` blockquotes from [`ENABLE_GFM`].
///
/// [`cmark`] can't turn on [`ENABLE_TABLES`], [`ENABLE_FOOTNOTES`],
/// [`ENABLE_STRIKETHROUGH`], [`ENABLE_TASKLISTS`],
/// [`ENABLE_SMART_PUNCTUATION`], [`ENABLE_DEFINITION_LIST`] or
/// [`ENABLE_MATH`], which are controlled by the extensions, or
/// [`ENABLE_HEADING_ATTRIBUTES`], which parses `{…}` after a heading
/// differently from kramdown. [`ENABLE_OLD_FOOTNOTES`] only has an effect
/// with [`Extensions::ENABLE_FOOTNOTES`].
///
/// [`cmark`]: Self::cmark
/// [`ENABLE_GFM`]: pulldown_cmark::Options::ENABLE_GFM
/// [`ENABLE_TABLES`]: pulldown_cmark::Options::ENABLE_TABLES
/// [`ENABLE_FOOTNOTES`]: pulldown_cmark::Options::ENABLE_FOOTNOTES
/// [`ENABLE_STRIKETHROUGH`]: pulldown_cmark::Options::ENABLE_STRIKETHROUGH
/// [`ENABLE_TASKLISTS`]: pulldown_cmark::Options::ENABLE_TASKLISTS
/// [`ENABLE_SMART_PUNCTUATION`]: pulldown_cmark::Options::ENABLE_SMART_PUNCTUATION
/// [`ENABLE_DEFINITION_LIST`]: pulldown_cmark::Options::ENABLE_DEFINITION_LIST
/// [`ENABLE_MATH`]: pulldown_cmark::Options::ENABLE_MATH
/// [`ENABLE_HEADING_ATTRIBUTES`]: pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES
/// [`ENABLE_OLD_FOOTNOTES`]: pulldown_cmark::Options::ENABLE_OLD_FOOTNOTES
///
/// ```
/// use pulldown_kram::{Document, Extensions, Options};
/// use pulldown_kram::pulldown_cmark::Options as Cmark;
///
/// // Plain kramdown.
/// let kramdown = Document::new("a | b\n-|-\n", Extensions::all());
///
/// // Plus GitHub's `> [!NOTE]` blockquotes.
/// let more = Options::new(Extensions::all(), Cmark::ENABLE_GFM);
/// let document = Document::new("> [!NOTE]\n> a\n", more);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Options {
    /// The kramdown extensions to enable.
    pub kramdown: Extensions,
    /// Extra pulldown-cmark options, for syntax kramdown doesn't have.
    /// Options that belong to an extension are ignored.
    pub cmark: pulldown_cmark::Options,
}

impl Options {
    /// Creates options from kramdown extensions and extra pulldown-cmark
    /// options.
    pub fn new(kramdown: Extensions, cmark: pulldown_cmark::Options) -> Self {
        Self { kramdown, cmark }
    }

    /// The options pulldown-cmark reads with: those the extensions need,
    /// and the rest of `cmark`.
    pub(crate) fn parser_options(&self) -> pulldown_cmark::Options {
        let read =
            Extensions::all().cmark_options() | pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES;
        (self.cmark - read) | self.kramdown.cmark_options()
    }
}

impl From<Extensions> for Options {
    /// Enables the `kramdown` extensions, with no extra pulldown-cmark
    /// options.
    fn from(kramdown: Extensions) -> Self {
        Self {
            kramdown,
            cmark: pulldown_cmark::Options::empty(),
        }
    }
}

impl Default for Options {
    /// Every extension, matching kramdown, with no extra pulldown-cmark
    /// options.
    fn default() -> Self {
        Self::from(Extensions::all())
    }
}

bitflags! {
    /// The parts of kramdown's syntax that go beyond CommonMark, each of
    /// which can be turned off.
    ///
    /// With every extension on, as in [`Extensions::all()`], documents are
    /// parsed the way kramdown parses them. Some extensions are parsed by
    /// pulldown-cmark itself, and say which of its options they turn on. See
    /// [`Options`] for how those interact with [`Options::cmark`].
    ///
    /// The flags come in groups:
    ///
    /// - Syntax pulldown-cmark also reads, from tables to math, which turn
    ///   on its option for it.
    /// - Attribute lists, and the ids and classes that come from them.
    /// - kramdown's own block and span syntax.
    /// - Where kramdown reads CommonMark's syntax differently.
    ///
    /// [`ENABLE_TABLES_OF_CONTENTS`](Self::ENABLE_TABLES_OF_CONTENTS) makes
    /// [`Document::events`](crate::Document::events) collect every event
    /// before returning the first, since a table can link to headings after
    /// it. [`Extensions::streamable()`] leaves it out, so events are returned
    /// a block at a time. Any other extension can be turned off by removing
    /// it from [`Extensions::all()`]:
    ///
    /// ```
    /// use pulldown_kram::{Document, Extensions, pulldown_cmark::Event};
    ///
    /// // Everything except typography, so quotes stay straight.
    /// let extensions = Extensions::all() - Extensions::ENABLE_TYPOGRAPHY;
    /// let document = Document::new("\"a\"\n", extensions);
    /// let text: Vec<_> = document
    ///     .events()
    ///     .filter_map(|(event, _)| match event {
    ///         Event::Text(text) => Some(text),
    ///         _ => None,
    ///     })
    ///     .collect();
    /// assert_eq!(text, ["\"a\"".into()]);
    /// ```
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Extensions: u32 {
        // Syntax pulldown-cmark also reads.

        /// Tables with a header row. Turns on pulldown-cmark's
        /// `ENABLE_TABLES`.
        const ENABLE_TABLES = 1 << 0;
        /// Footnotes, `[^1]`, with block content in their definitions. Turns
        /// on pulldown-cmark's `ENABLE_FOOTNOTES`.
        const ENABLE_FOOTNOTES = 1 << 1;
        /// `~~Strikethrough~~`, from kramdown's GFM parser. Turns on
        /// pulldown-cmark's `ENABLE_STRIKETHROUGH`.
        const ENABLE_STRIKETHROUGH = 1 << 2;
        /// Task list items, `* [x] done`, from kramdown's GFM parser, with
        /// its classes: `task-list` on the list, and `task-list-item` on
        /// each item from the first task onward. Turns on pulldown-cmark's
        /// `ENABLE_TASKLISTS`.
        const ENABLE_TASK_LISTS = 1 << 3;
        /// Dashes, ellipses and smart quotes. As in kramdown, header ids
        /// leave these characters out, so ids only match kramdown's with this
        /// on. Turns on pulldown-cmark's `ENABLE_SMART_PUNCTUATION`.
        const ENABLE_TYPOGRAPHY = 1 << 4;
        /// Definition lists: a term followed by `: definition` lines. Turns
        /// on pulldown-cmark's `ENABLE_DEFINITION_LIST`.
        const ENABLE_DEFINITION_LISTS = 1 << 5;
        /// Math between `$$`: a block when it's alone in its paragraph,
        /// inline otherwise. As in kramdown, a single `$` is plain text.
        /// Turns on pulldown-cmark's `ENABLE_MATH`.
        const ENABLE_MATH = 1 << 6;

        // Attribute lists, and what comes from them.

        /// Block attribute lists, like `{:.note}` on the line after a
        /// paragraph, or alone between blocks to apply to the next one.
        const ENABLE_BLOCK_ATTRIBUTES = 1 << 7;
        /// Span attribute lists directly after a span, like `*a*{:.x}`, and
        /// the empty list, `{::}`, as a separator between spans.
        const ENABLE_SPAN_ATTRIBUTES = 1 << 8;
        /// Attribute lists at the start of a list item, like `* {:.x} a`.
        const ENABLE_ITEM_ATTRIBUTES = 1 << 9;
        /// Header ids after a heading's text, `# Hello {#hi}`.
        const ENABLE_HEADER_IDS = 1 << 10;
        /// Automatic ids for headings without one, generated the same way as
        /// kramdown's GFM parser. kramdown's `auto_ids` option.
        const ENABLE_AUTO_IDS = 1 << 11;
        /// A code block's language from a `language-` class, like
        /// `{: .language-ruby}`, when its fence doesn't give one. The class
        /// comes from a block attribute list, so this needs
        /// `ENABLE_BLOCK_ATTRIBUTES`.
        const ENABLE_LANGUAGE_CLASSES = 1 << 12;
        /// Lists marked `{:toc}` turned into tables of contents, linking to
        /// every heading that has an id and doesn't have the class `no_toc`.
        /// The marker is a block attribute list, so this needs
        /// `ENABLE_BLOCK_ATTRIBUTES`.
        ///
        /// Every event is collected before the first is returned,
        /// so tables can link to headings after them. Each table lists every
        /// heading, so output could grow with the square of the document's
        /// size. To bound it, tables of contents produce at most 100,000
        /// events or as many as the document has, whichever is larger; marked
        /// lists past that limit are left as written.
        const ENABLE_TABLES_OF_CONTENTS = 1 << 13;

        // kramdown's own syntax.

        /// The end-of-block marker, a line with only `^`, closing the blocks
        /// open at that point, like a list or a code block. It produces no
        /// events.
        const ENABLE_END_OF_BLOCK_MARKERS = 1 << 14;
        /// Hard line breaks with two backslashes at the end of a line, as
        /// well as with two spaces.
        const ENABLE_BACKSLASH_LINE_BREAKS = 1 << 15;
        /// Tables without a header row, like a lone `| a | b |` line, and a
        /// `|` in code in a table cell kept in that cell instead of starting
        /// a new one. Also accepts a separator line with fewer cells than
        /// the header, or `+` between its cells. pulldown-cmark parses the
        /// tables, so this needs `ENABLE_TABLES`.
        const ENABLE_TABLES_WITHOUT_HEADERS = 1 << 16;
        /// Abbreviations, `*[HTML]: HyperText Markup Language`. Each use in
        /// the text becomes an `<abbr>` with its title.
        const ENABLE_ABBREVIATIONS = 1 << 17;
        /// Markdown inside HTML elements marked `markdown="1"`. The
        /// attribute is removed from the HTML.
        const ENABLE_MARKDOWN_ATTRIBUTE = 1 << 18;
        /// kramdown's `comment` extension, `{::comment}…{:/}`, as a block or
        /// span, emitted as an HTML comment.
        const ENABLE_COMMENTS = 1 << 19;
        /// kramdown's `nomarkdown` extension, `{::nomarkdown}…{:/}`, as a
        /// block or span, emitted as raw HTML unless its `type` names another
        /// of kramdown's converters, like `latex`.
        const ENABLE_NOMARKDOWN = 1 << 20;

        // Where kramdown reads CommonMark's syntax differently.

        /// HTML blocks that end at the end tag of the element that opened
        /// them, as in kramdown, with any text after it parsed as Markdown.
        /// CommonMark's HTML blocks run to the next blank line. A blank line
        /// before the end tag still ends the block, as in CommonMark.
        const ENABLE_HTML_BLOCK_ENDS = 1 << 21;
        /// A line with only a span-level tag, like `<img>`, starts a
        /// paragraph together with the text on the next line, instead of an
        /// HTML block. Followed by a blank line, it's still an HTML block,
        /// where kramdown makes it a paragraph.
        const ENABLE_SPAN_TAG_PARAGRAPHS = 1 << 22;
        /// The text of the HTML spans kramdown leaves raw, like `<kbd>`,
        /// `<code>` and `<var>`, passed through unchanged, without Markdown
        /// or typography applied inside them.
        const ENABLE_RAW_HTML_SPANS = 1 << 23;
        /// Tight and loose list items decided per item, as kramdown does,
        /// rather than per list: an item's first paragraph is only wrapped
        /// in a paragraph if a blank line follows it in the item.
        const ENABLE_LIST_ITEM_PARAGRAPHS = 1 << 24;
        /// One list for items marked with any mix of `*`, `+` and `-`.
        /// CommonMark starts a new list whenever the marker changes.
        const ENABLE_MIXED_LIST_MARKERS = 1 << 25;
        /// Backslash escapes for only the characters kramdown escapes. A
        /// backslash before any other character, like `\&`, is kept, where
        /// CommonMark escapes all ASCII punctuation.
        const ENABLE_KRAMDOWN_ESCAPES = 1 << 26;
    }
}

impl Extensions {
    /// Every extension except
    /// [`ENABLE_TABLES_OF_CONTENTS`](Self::ENABLE_TABLES_OF_CONTENTS), so
    /// [`Document::events`](crate::Document::events) returns events a block at
    /// a time instead of collecting them all first. Lists marked `{:toc}`
    /// stay as written.
    ///
    /// ```
    /// use pulldown_kram::{Document, Extensions};
    ///
    /// let document = Document::new("* x\n{:toc}\n", Extensions::streamable());
    /// assert_eq!(document.events().count(), 5);
    /// ```
    pub const fn streamable() -> Self {
        Self::all().difference(Self::ENABLE_TABLES_OF_CONTENTS)
    }

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

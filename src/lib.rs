//! Reads Markdown written for [kramdown], Jekyll's default, as
//! [pulldown-cmark] events, each with the attributes kramdown's attribute
//! lists give it.
//!
//! ```
//! use pulldown_kram::{Document, Extensions, pulldown_cmark::{Event, Tag}};
//!
//! let document = Document::new("# Intro\n\nSee `x`{:.new}.\n", Extensions::all());
//! let events: Vec<_> = document.events().collect();
//!
//! // Headings get the id kramdown would give them.
//! assert!(matches!(
//!     &events[0].0,
//!     Event::Start(Tag::Heading { id: Some(id), .. }) if id.as_ref() == "intro"
//! ));
//! // Attribute lists go to what they're for, not the text.
//! let (_, attributes) = events
//!     .iter()
//!     .find(|(event, _)| matches!(event, Event::Code(c) if c.as_ref() == "x"))
//!     .unwrap();
//! assert!(attributes.has_class("new"));
//! ```
//!
//! Underneath, pulldown-cmark reads the document as CommonMark, after a
//! little rewriting where kramdown reads a line differently, and the events
//! it gives are worked over for kramdown's additions. So a program that
//! renders pulldown-cmark's events renders these too, and can pick up the
//! attributes where it wants them.
//!
//! For HTML, [`html::push_html`] writes the events out the way kramdown
//! writes the same elements, attributes included.
//!
//! For CommonMark text rather than events, write the events back out with
//! [pulldown-cmark-to-cmark]. The attributes are lost, as CommonMark has no
//! way to write them.
//!
//! [kramdown]: https://kramdown.gettalong.org
//! [pulldown-cmark]: https://docs.rs/pulldown-cmark
//! [pulldown-cmark-to-cmark]: https://docs.rs/pulldown-cmark-to-cmark
//!
//! # Syntax
//!
//! How much of [kramdown's syntax] is understood, following its sections. A
//! checked item reads as kramdown means it. An unchecked one doesn't, and
//! says what happens instead. Much of what's checked is CommonMark, where
//! the two agree, and what's beyond CommonMark depends on the [`Options`]
//! given. Each of kramdown's extensions can be left off with
//! [`Extensions`], and `Extensions::all()` reads them all.
//!
//! [kramdown's syntax]: https://kramdown.gettalong.org/syntax.html
//!
//! ## Source text
//!
//! - [x] Backslash escapes, for every character kramdown escapes.
//! - [x] Tab stops at multiples of four.
//! - [ ] Headers starting only on a block boundary. CommonMark lets an atx
//!   or setext header interrupt a paragraph, so `para\n# Not` has a heading.
//! - [x] The end-of-block marker, `^`, ending the paragraph, list, quote or
//!   code block before it.
//!
//! ## Structural elements
//!
//! - [x] Paragraphs, lazy lines included.
//! - [x] Hard line breaks from two trailing spaces.
//! - [x] Hard line breaks from two trailing backslashes.
//! - [x] Setext and atx headers, the `header\n---` edge case included.
//! - [x] Header ids, `# Hello {#id}`, while `# Hello {: #id}` stays text, as
//!   in kramdown.
//! - [x] Blockquotes, nested and lazy.
//! - [x] Indented code blocks, merged across blank lines.
//! - [ ] Lazy lines wrapped onto an indented code block. They end the code
//!   block and start a paragraph.
//! - [x] Fenced code blocks with `~~~`, and a language after the fence.
//! - [x] A code block's language from an attribute list,
//!   `{: .language-ruby}`, when the fence gives none.
//! - [x] Ordered and unordered lists, nested, compact, or with a code block
//!   first, and `1984\.` escaping a marker.
//! - [x] A list item's first paragraph bare unless a blank line comes after
//!   it, decided for each item, not for the whole list.
//! - [x] Mixing `*`, `+` and `-` in one list.
//! - [ ] Ordered lists numbered from 1 whatever the first number. The first
//!   number is kept.
//! - [x] Definition lists, a term on each line and a definition's first
//!   paragraph bare unless a blank line comes before it. At most one blank
//!   line comes between the terms and their first definition, and any
//!   number between definitions, and a definition with nothing on its
//!   first line is indented by four.
//! - [ ] Terms on separate lines inside one span, like `*a\nb*`. They stay
//!   one term.
//! - [x] Tables with a header row, aligned by the header separator line.
//! - [ ] Tables without a header row. Read as text.
//! - [ ] Separator lines in a table body, and footer separator lines,
//!   `|===`. Each is read as a row of dashes or equal signs.
//! - [x] Horizontal rules.
//! - [x] Math blocks, `$$` alone in a paragraph.
//!
//! ## HTML
//!
//! HTML comes through as pulldown-cmark's HTML events, for the program
//! reading them to make what it will of.
//!
//! - [x] HTML blocks, Markdown in them left as HTML.
//! - [x] `markdown="1"` on a block element, its content read as Markdown
//!   and the attribute left out.
//! - [ ] `markdown="block"`, `markdown="span"` and `markdown="0"`. Their
//!   content is left as HTML.
//! - [ ] The `parse_block_html` option.
//! - [x] A line of only a span tag, like `<img>`, starting a paragraph.
//! - [x] HTML spans, with Markdown in them.
//! - [x] The HTML spans kramdown leaves raw, like `<kbd>` and `<code>`,
//!   their text given out as it is.
//!
//! ## Text markup
//!
//! - [x] Automatic links, `<http://example.com>` and `<me@example.com>`.
//! - [x] Inline links, with titles in either quotes.
//! - [ ] Spaces in an inline link's URL, `[a](b c.html)`. Read as text.
//! - [x] Reference links, `[text][id]`, `[text][]` and `[text]`, ignoring
//!   case.
//! - [ ] Spaces or a line break between a reference link's brackets,
//!   `[text]\n[id]`. Read as two links.
//! - [x] Link definitions, and `\:` to keep one as text.
//! - [x] Images, inline and by reference.
//! - [x] Emphasis and strong emphasis, with `*` inside words but not `_`.
//! - [ ] Emphasis inside the same emphasis, `*a _b_ c*`, staying literal.
//!   It nests.
//! - [x] Code spans.
//! - [x] Footnotes, with blocks in their definitions.
//! - [ ] Abbreviations, `*[HTML]: …`. The definition is read as text.
//! - [x] Dashes, ellipses and smart quotes.
//! - [ ] Guillemets from `<<` and `>>`. Read as text, or as an HTML tag.
//! - [x] Inline math, `$$…$$` in a paragraph with more in it, while `$…$`
//!   stays text.
//! - [x] Strikethrough, `~~…~~`, from kramdown's GFM parser, while `~…~`
//!   stays text.
//!
//! ## Non-content elements
//!
//! - [x] Block attribute lists, on the line after a block or the line
//!   before it, or alone between blocks for the one after.
//! - [x] Block attribute lists on link definitions, for the links and
//!   images that use them.
//! - [ ] A block attribute list after a list item ending the list. It goes
//!   to the list, but the items after it stay in the same list.
//! - [x] Span attribute lists, right after emphasis, a link, an image or a
//!   code span.
//! - [x] List item attribute lists, `* {:.cls} item`.
//! - [ ] Definition attribute lists, `: {:.cls} definition`. Read as text.
//! - [x] The empty span attribute list, `{::}`, separating spans.
//! - [ ] Attribute list definitions, `{:name: .class}`. Dropped, never
//!   applied.
//! - [x] The `comment` extension, a block or a span, given out as an HTML
//!   comment.
//! - [x] The `nomarkdown` extension, a block or a span, given out as HTML
//!   as it is, unless its `type` is another converter's.
//! - [ ] The `options` extension. Dropped.
//! - [x] Tags of extensions kramdown doesn't have, like `{::something}`,
//!   kept as text.
//!
//! ## Converter
//!
//! Not syntax, but what Jekyll sites lean on.
//!
//! - [x] Header ids, made the way kramdown's GFM parser, Jekyll's default,
//!   makes them, from the heading's text without its markup.
//! - [x] `{:toc}` tables of contents, nested by level, leaving out headings
//!   with the class `no_toc`, and links inside headings. With no headings
//!   to list, the marked list is dropped.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

mod attributes;
mod definitions;
mod headings;
pub mod html;
mod lists;
mod options;
mod read;
mod source;
mod toc;

use std::borrow::Cow;
use std::fmt;
use std::iter::FusedIterator;
use std::ops::Range;

pub use attributes::Attributes;
pub use options::{Extensions, Options};
pub use pulldown_cmark;
use pulldown_cmark::Event;
use source::{
    AttributeLines, DefinitionLines, Mark, MarkdownElements, Offsets, SpanTags, StandIns,
};

/// Markdown written for kramdown, ready to read.
#[derive(Debug, Clone)]
pub struct Document<'a> {
    text: Cow<'a, str>,
    /// Where the rewritten text was in the markdown.
    offsets: Offsets,
    /// The stand-ins in the text, for what CommonMark has no way to read.
    marks: Vec<Mark<'a>>,
    options: Options,
}

impl<'a> Document<'a> {
    /// Reads `markdown` with `options`, which may be just pulldown-cmark's,
    /// for every kramdown extension, or just the [`Extensions`].
    pub fn new(markdown: &'a str, options: impl Into<Options>) -> Self {
        let options = options.into();
        let reads = |extension| options.kramdown.contains(extension);
        // Rewrite what CommonMark reads differently, before it's read.
        let mut text = Cow::Borrowed(markdown);
        let mut offsets = Offsets::default();
        let mut marks = Vec::new();
        // First, for what the stand-ins stand for to be in the markdown.
        if reads(Extensions::ENABLE_END_OF_BLOCK_MARKERS)
            || reads(Extensions::ENABLE_COMMENT_EXTENSION)
            || reads(Extensions::ENABLE_NOMARKDOWN_EXTENSION)
        {
            (text, offsets, marks) = StandIns::mark(
                markdown,
                reads(Extensions::ENABLE_END_OF_BLOCK_MARKERS),
                reads(Extensions::ENABLE_COMMENT_EXTENSION),
                reads(Extensions::ENABLE_NOMARKDOWN_EXTENSION),
            );
        }
        // Moves the marks past `edits`, and adds them to the offsets.
        let mut then = |offsets: &mut Offsets, edits: Offsets| {
            for mark in &mut marks {
                mark.at = edits.moved(mark.at);
            }
            *offsets = std::mem::take(offsets).then(edits);
        };
        if reads(Extensions::ENABLE_MARKDOWN_ATTRIBUTE)
            && let (Cow::Owned(separated), edits) = MarkdownElements::separate(&text)
        {
            text = Cow::Owned(separated);
            then(&mut offsets, edits);
        }
        if reads(Extensions::ENABLE_SPAN_TAG_PARAGRAPHS)
            && let (Cow::Owned(joined), edits) = SpanTags::join(&text)
        {
            text = Cow::Owned(joined);
            then(&mut offsets, edits);
        }
        if reads(Extensions::ENABLE_BLOCK_ATTRIBUTES)
            && let (Cow::Owned(separated), edits) = AttributeLines::separate(&text)
        {
            text = Cow::Owned(separated);
            then(&mut offsets, edits);
        }
        if reads(Extensions::ENABLE_DEFINITION_LISTS)
            && let (Cow::Owned(rewritten), edits) = DefinitionLines::rewrite(&text)
        {
            text = Cow::Owned(rewritten);
            then(&mut offsets, edits);
        }
        Self {
            text,
            offsets,
            marks,
            options,
        }
    }

    /// The document's events, each with the attributes kramdown gives it.
    ///
    /// They're read as they're asked for, a block at a time, as attribute
    /// lists after a block are for it. With tables of contents, they're all
    /// read first, for the tables to link to the headings after them.
    pub fn events(&self) -> Events<'_> {
        let reader = read::Reader::new(&self.text, self.options, &self.marks);
        let items = if self
            .options
            .kramdown
            .contains(Extensions::ENABLE_TABLES_OF_CONTENTS)
        {
            Items::Read(toc::Toc::replace_lists(reader.collect()).into_iter())
        } else {
            Items::Streamed(Box::new(reader))
        };
        Events {
            items,
            offsets: &self.offsets,
        }
    }
}

/// The events of a [`Document`], each with its attributes.
///
/// ```
/// use pulldown_kram::{Document, Extensions, pulldown_cmark::Event};
///
/// let document = Document::new("*a*{:.x}\n", Extensions::all());
/// let classes: Vec<_> = document
///     .events()
///     .filter(|(event, _)| matches!(event, Event::Start(_)))
///     .map(|(_, attributes)| attributes.classes)
///     .collect();
/// assert_eq!(classes, [vec![], vec!["x".into()]]);
/// ```
pub struct Events<'a> {
    items: Items<'a>,
    /// Where the items' ranges were in the document's markdown.
    offsets: &'a Offsets,
}

/// Where the events come from.
enum Items<'a> {
    /// Read as they're asked for.
    Streamed(Box<read::Reader<'a>>),
    /// Read all at once.
    Read(std::vec::IntoIter<read::Item<'a>>),
}

impl<'a> Events<'a> {
    /// The events with where each came from in the document's markdown, as
    /// pulldown-cmark's `Parser::into_offset_iter` gives them.
    pub fn into_offset_iter(self) -> OffsetIter<'a> {
        OffsetIter(self)
    }

    /// The next item, its range in the text read.
    fn item(&mut self) -> Option<read::Item<'a>> {
        match &mut self.items {
            Items::Streamed(reader) => reader.next(),
            Items::Read(items) => items.next(),
        }
    }
}

impl<'a> Iterator for Events<'a> {
    type Item = (Event<'a>, Attributes<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.item().map(|item| (item.event, item.lists.attributes))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.items {
            Items::Streamed(reader) => reader.size_hint(),
            Items::Read(items) => items.size_hint(),
        }
    }
}

impl FusedIterator for Events<'_> {}

impl fmt::Debug for Events<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut events = f.debug_struct("Events");
        match &self.items {
            Items::Streamed(reader) => events
                .field("streamed", &true)
                .field("ready", &reader.size_hint().0),
            Items::Read(items) => events
                .field("streamed", &false)
                .field("ready", &items.len()),
        };
        events.finish()
    }
}

/// The events of a [`Document`], each with its attributes and the byte
/// range it came from in the markdown the document was made with.
///
/// The document's markdown may be rewritten a little before it's read, so
/// pulldown-cmark reads it as kramdown does. A range covering any of what
/// was rewritten covers all of it in the markdown, and one inside a line
/// put in is empty, where the line went in. Every range is in order, on
/// char boundaries, and within the markdown.
///
/// ```
/// use pulldown_kram::{Document, Extensions, pulldown_cmark::{Event, Tag}};
///
/// let markdown = "<div markdown=\"1\">\n*a*{:.x} b\n</div>\n";
/// let document = Document::new(markdown, Extensions::all());
/// let (_, attributes, range) = document
///     .events()
///     .into_offset_iter()
///     .find(|(event, _, _)| matches!(event, Event::Start(Tag::Emphasis)))
///     .unwrap();
/// assert_eq!(&markdown[range], "*a*");
/// assert!(attributes.has_class("x"));
/// ```
pub struct OffsetIter<'a>(Events<'a>);

impl<'a> Iterator for OffsetIter<'a> {
    type Item = (Event<'a>, Attributes<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.0.item()?;
        let range = self.0.offsets.original(item.range);
        Some((item.event, item.lists.attributes, range))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl FusedIterator for OffsetIter<'_> {}

impl fmt::Debug for OffsetIter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OffsetIter")
            .field("events", &self.0)
            .field("rewritten", &!self.0.offsets.is_empty())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_events_read_afresh_each_time() {
        let document = Document::new("# A\n# A\n", Extensions::all());
        let ids = || -> Vec<String> {
            document
                .events()
                .filter_map(|(event, _)| match event {
                    Event::Start(pulldown_cmark::Tag::Heading { id, .. }) => {
                        id.map(|id| id.to_string())
                    }
                    _ => None,
                })
                .collect()
        };
        assert_eq!(ids(), ["a", "a-1"]);
        assert_eq!(ids(), ["a", "a-1"]);
    }

    #[test]
    fn document_events_reading_all_first_reads_what_streaming_does() {
        let markdown = "# A {#x}\n\npara\n{:.y}\n\n* *b*{:.z}\n\n> q\n\n{:.w}\n\n---\n";
        let all = Document::new(markdown, Extensions::all());
        let streamed = Document::new(
            markdown,
            Extensions::all() - Extensions::ENABLE_TABLES_OF_CONTENTS,
        );
        assert!(all.events().eq(streamed.events()));
    }

    #[test]
    fn events_into_offset_iter_offsets_are_into_the_markdown() {
        let documents = [
            // Nothing rewritten.
            "# Héading {#h}\n\nSome *tëxt*{:.x} and `code`.\n\n* one\n* two\n{:.list}\n\n> quote\n",
            // Blank lines added inside an element.
            "<div markdown=\"1\">\n# Inside\n\n*ä*{:.x} b\n</div>\nafter\n",
            // A span tag's line joined to the next.
            "<span class=\"g\">  \n*ä* b\n</span>\n\nafter ü\n",
            // Both.
            "<div markdown=\"1\">\n<img src=\"é\" />\nnaïve *text*\nand `code`\n</div>\n\nafter ü\n",
        ];
        let options = [
            Extensions::all(),
            Extensions::all() - Extensions::ENABLE_TABLES_OF_CONTENTS,
        ];
        for markdown in documents {
            for options in options {
                let document = Document::new(markdown, options);
                let mut checked = 0;
                for (event, _, range) in document.events().into_offset_iter() {
                    let Some(source) = markdown.get(range.clone()) else {
                        panic!("{event:?} at {range:?} isn't in {markdown:?}");
                    };
                    let (Event::Text(text) | Event::Code(text) | Event::InlineHtml(text)) = &event
                    else {
                        continue;
                    };
                    if !document.offsets.touches(&range) {
                        assert!(
                            source.contains(text.as_ref()),
                            "{event:?} at {range:?} is {source:?}"
                        );
                        checked += 1;
                    }
                }
                assert!(checked >= 3, "only {checked} checked in {markdown:?}");
                // The offsets don't change the events.
                assert!(
                    document
                        .events()
                        .into_offset_iter()
                        .map(|(event, attributes, _)| (event, attributes))
                        .eq(document.events())
                );
            }
        }
    }

    #[test]
    fn events_into_offset_iter_finds_text_after_rewrites() {
        let markdown = "<div markdown=\"1\">\n<b>  \n*x*{:.y}\n</div>\n\nafter ü\n";
        let document = Document::new(markdown, Extensions::all());
        assert!(!document.offsets.is_empty());
        let ranges: Vec<_> = document
            .events()
            .into_offset_iter()
            .filter_map(|(event, _, range)| match event {
                Event::Text(_) | Event::InlineHtml(_) => Some(&markdown[range]),
                _ => None,
            })
            .collect();
        assert_eq!(ranges, ["<b>", "  \n", "x", "after ü"]);
    }
}

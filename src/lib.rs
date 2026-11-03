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
//! // Attribute lists are attached to the element they follow, not left in the text.
//! let (_, attributes) = events
//!     .iter()
//!     .find(|(event, _)| matches!(event, Event::Code(c) if c.as_ref() == "x"))
//!     .unwrap();
//! assert!(attributes.has_class("new"));
//! ```
//!
//! pulldown-cmark does the parsing, so a program that already renders its
//! events renders these too, and can use the attributes where it wants.
//! [How it works](#how-it-works) says what's done around it.
//!
//! For HTML, [`html::push_html`] writes the events the way kramdown writes
//! the same elements, attributes included.
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
//! How much of [kramdown's syntax] is supported, following the sections of
//! its documentation. A checked item reads as it does in kramdown. An
//! unchecked one doesn't, and says what happens instead. Much of the
//! checked list is plain CommonMark, where the two agree. The rest depends
//! on the [`Extensions`] turned on, and `Extensions::all()` turns on all of
//! them.
//!
//! [kramdown's syntax]: https://kramdown.gettalong.org/syntax.html
//!
//! ## Source text
//!
//! - [x] Backslash escapes for the characters kramdown escapes, and only
//!   those. A backslash before any other character is kept.
//! - [x] Tab stops at multiples of four.
//! - [ ] Headers starting only on a block boundary. CommonMark lets an atx
//!   or setext header interrupt a paragraph, so `para\n# Not` has a heading.
//! - [x] The end-of-block marker, `^`, which ends the paragraph, list,
//!   quote or code block before it.
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
//! - [ ] A fenced code block on the line after a list item's text, not
//!   indented, kept in the item as a lazy line, its lines without the
//!   item's indent. It ends the list.
//! - [ ] A fenced code block in a list item, indented past the item's
//!   content, keeping that indent on its lines. It's taken off.
//! - [ ] Ordered lists numbered from 1 whatever the first number. The first
//!   number is kept.
//! - [x] Definition lists, a term on each line and a definition's first
//!   paragraph bare unless a blank line comes before it. At most one blank
//!   line comes between the terms and their first definition, and any
//!   number between definitions, and a definition with nothing on its
//!   first line is indented by four.
//! - [ ] Terms on separate lines inside one span, like `*a\nb*`. They stay
//!   one term.
//! - [x] Tables with a header row, aligned by the header separator line,
//!   which may have fewer cells than the header, and `+` between them.
//! - [x] Tables without a header row.
//! - [x] A `|` in code in a table's cell, kept in it.
//! - [ ] Separator lines in a table body, and footer separator lines,
//!   `|===`. Each is read as a row of dashes or equal signs.
//! - [ ] A table of only separator lines. Read as a table.
//! - [x] Horizontal rules.
//! - [x] Math blocks, `$$` alone in a paragraph.
//! - [ ] Math blocks only after a blank line, so `$$` right after a fence
//!   or a header is inline math in a paragraph. It's a math block.
//!
//! ## HTML
//!
//! HTML comes through as pulldown-cmark's HTML events, for the program
//! reading them to handle.
//!
//! - [x] HTML blocks, with any Markdown inside left as HTML. A block ends at
//!   the end tag of the element that starts it, and text after that tag is
//!   Markdown, as long as no blank line comes first.
//! - [ ] A blank line in an HTML block. It ends the block, as in
//!   CommonMark, and what follows is read as Markdown.
//! - [x] `markdown="1"` on a block element. Its content is read as
//!   Markdown, and the attribute is left out.
//! - [ ] `markdown="block"`, `markdown="span"` and `markdown="0"`. Their
//!   content is left as HTML.
//! - [ ] The `parse_block_html` option.
//! - [x] A line of only a span tag, like `<img>`, starting a paragraph that
//!   takes in the text on the next line.
//! - [ ] A line of only a span tag with a blank line after it, a paragraph
//!   of its own. It's an HTML block.
//! - [x] HTML spans, with Markdown in them.
//! - [x] The HTML spans kramdown leaves raw, like `<kbd>` and `<code>`, with
//!   their text kept as it is.
//!
//! ## Text markup
//!
//! - [x] Automatic links, `<http://example.com>` and `<me@example.com>`.
//! - [ ] `<mailto:me@example.com>`, its text the address alone. The
//!   `mailto:` stays in its text.
//! - [x] Inline links, with titles in either quotes.
//! - [ ] Spaces in an inline link's URL, `[a](b c.html)`. Read as text.
//! - [x] Reference links, `[text][id]`, `[text][]` and `[text]`, ignoring
//!   case.
//! - [ ] Spaces or a line break between a reference link's brackets,
//!   `[text]\n[id]`. Read as two links.
//! - [ ] Brackets in a reference link's text, `[[1]][1]`. The inner
//!   brackets are read as the link, and the outer as text.
//! - [x] Link definitions, and `\:` to keep one as text.
//! - [x] Images, inline and by reference.
//! - [ ] An image's alt text as written, markup and straight quotes
//!   included. Its markup is read and its quotes made typographic.
//! - [x] Emphasis and strong emphasis, with `*` inside words but not `_`.
//! - [ ] Emphasis inside the same emphasis, `*a _b_ c*` or `_*b*_`,
//!   staying literal. It nests.
//! - [ ] `***a***` as strong emphasis around emphasis. It's emphasis around
//!   strong emphasis.
//! - [x] Code spans.
//! - [x] Footnotes, with blocks in their definitions.
//! - [x] Abbreviations, `*[HTML]: …`. Each use becomes an `<abbr>` in an
//!   HTML event, with the attributes from lists on the lines around the
//!   definition.
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
//! - [x] The `comment` extension, as a block or a span, which comes out as
//!   an HTML comment.
//! - [x] The `nomarkdown` extension, as a block or a span, which comes out
//!   as raw HTML, unless its `type` names another converter.
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
//! - [x] `{:toc}` tables of contents, nested by level. Headings with the
//!   class `no_toc` are left out, and so are links inside headings. With
//!   no headings to list, the marked list is dropped.
//!
//! # How it works
//!
//! Reading a document takes four steps.
//!
//! 1. The Markdown is rewritten wherever kramdown reads a line differently
//!    from CommonMark, so that pulldown-cmark reads it kramdown's way. For
//!    example, a blank line goes in after the end tag of an HTML block,
//!    since kramdown ends the block there and CommonMark doesn't. Syntax
//!    pulldown-cmark can't read at all, like `{::comment}` or the
//!    end-of-block marker, is replaced with a stand-in: a little HTML that
//!    pulldown-cmark reads as a block or a span, and that the reader later
//!    swaps for what it stands for.
//! 2. pulldown-cmark parses the rewritten text.
//! 3. A reader takes the events a block at a time. It takes attribute
//!    lists out of the text and attaches them to the elements they belong
//!    to, puts back what the stand-ins stand for, and corrects the events
//!    where kramdown's structure differs, like in definition lists, list
//!    item paragraphs, header ids and abbreviations.
//! 4. With tables of contents on, all of the events are read first, and
//!    each list marked `{:toc}` is replaced with links to the headings.
//!
//! The ranges from [`Events::into_offset_iter`] are mapped back through the
//! rewrites, so they point into the Markdown as it was given.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

mod abbreviations;
mod attributes;
mod definition_lists;
mod headings;
pub mod html;
mod list_items;
mod options;
mod read;
mod source;
mod text;
mod toc;

use std::borrow::Cow;
use std::fmt;
use std::iter::FusedIterator;
use std::ops::Range;

use abbreviations::Abbreviation;
pub use attributes::Attributes;
pub use options::{Extensions, Options};
pub use pulldown_cmark;
use pulldown_cmark::Event;
use source::{Mark, Marked, Offsets, REWRITES, StandIns};

/// Markdown written for kramdown, ready to read.
#[derive(Debug, Clone)]
pub struct Document<'a> {
    text: Cow<'a, str>,
    /// Where the rewritten text was in the markdown.
    offsets: Offsets,
    /// Where the stand-ins are in the text, and what each stands for.
    marks: Vec<Mark<'a>>,
    abbreviations: Vec<Abbreviation<'a>>,
    options: Options,
}

impl<'a> Document<'a> {
    /// Reads `markdown` with `options`, which are either just the
    /// [`Extensions`] or [`Options`] with more of pulldown-cmark's.
    pub fn new(markdown: &'a str, options: impl Into<Options>) -> Self {
        let options = options.into();
        // The stand-ins go in first, as what they stand for borrows from
        // `markdown`, not from a rewrite of it.
        let Marked {
            mut text,
            mut offsets,
            mut marks,
            abbreviations,
        } = StandIns::rewrite(markdown, options.kramdown);
        for (extension, rewrite) in REWRITES {
            if !options.kramdown.contains(extension) {
                continue;
            }
            let (Cow::Owned(rewritten), edits) = rewrite(&text) else {
                continue;
            };
            for mark in &mut marks {
                mark.at = edits.moved(mark.at);
            }
            offsets = offsets.then(edits);
            text = Cow::Owned(rewritten);
        }
        Self {
            text,
            offsets,
            marks,
            abbreviations,
            options,
        }
    }

    /// The document's events, each with the attributes kramdown gives it.
    ///
    /// They're read as they're asked for, a block at a time, since an
    /// attribute list after a block can still change it. With tables of
    /// contents on, they're all read first, so a table can link to the
    /// headings after it.
    pub fn events(&self) -> Events<'_> {
        let reader = read::Reader::new(&self.text, self.options, &self.marks)
            .abbreviating(&self.abbreviations);
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

    /// The next item, with its range in the rewritten text.
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
        self.item()
            .map(|item| (item.event, item.attribute_lists.attributes))
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
/// The markdown is rewritten in places before pulldown-cmark reads it, so
/// that it reads as kramdown reads it. A range that covers part of a
/// rewritten piece covers all of that piece in the markdown. A range inside
/// a line that was added is empty, at the point where the line went in.
/// Every range is ordered, on char boundaries, and inside the markdown.
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
        Some((item.event, item.attribute_lists.attributes, range))
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

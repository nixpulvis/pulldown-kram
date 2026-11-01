//! Reads Markdown written for [kramdown], Jekyll's default, as
//! [pulldown-cmark] events, each with the attributes kramdown's attribute
//! lists give it.
//!
//! ```
//! use pulldown_kram::{Document, pulldown_cmark::{Event, Options, Tag}};
//!
//! let document = Document::new("# Intro\n\nSee `x`{:.new}.\n", Options::empty());
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
//! the two agree, so it depends on the [`Options`] given: tables, footnotes,
//! math and typography need pulldown-cmark's `ENABLE_` options. Each of
//! kramdown's own additions can be left off with [`Extensions`].
//!
//! [kramdown's syntax]: https://kramdown.gettalong.org/syntax.html
//!
//! ## Source text
//!
//! - [x] Backslash escapes, for every character kramdown escapes.
//! - [x] Tab stops at multiples of four.
//! - [ ] Headers starting only on a block boundary. CommonMark lets an atx
//!   or setext header interrupt a paragraph, so `para\n# Not` has a heading.
//! - [ ] The end-of-block marker, `^`. Read as text.
//!
//! ## Structural elements
//!
//! - [x] Paragraphs, lazy lines included.
//! - [x] Hard line breaks from two trailing spaces.
//! - [ ] Hard line breaks from two trailing backslashes. One backslash is
//!   left in the text.
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
//! - [ ] Mixing `*`, `+` and `-` in one list. Each change of marker starts a
//!   new list.
//! - [ ] Ordered lists numbered from 1 whatever the first number. The first
//!   number is kept.
//! - [ ] Definition lists. Terms and `: definitions` run together into a
//!   paragraph, unless pulldown-cmark's `ENABLE_DEFINITION_LIST` is given,
//!   which reads them its own way.
//! - [x] Tables with a header row, aligned by the header separator line.
//! - [ ] Tables without a header row. Read as text.
//! - [ ] Separator lines in a table body, and footer separator lines,
//!   `|===`. Each is read as a row of dashes or equal signs.
//! - [x] Horizontal rules.
//! - [x] Math blocks, `$$`.
//!
//! ## HTML
//!
//! HTML comes through as pulldown-cmark's HTML events, for the program
//! reading them to make what it will of.
//!
//! - [x] HTML blocks, Markdown in them left as HTML.
//! - [x] `markdown="1"` on a block element, its content read as Markdown.
//! - [ ] `markdown="block"`, `markdown="span"` and `markdown="0"`. Their
//!   content is left as HTML.
//! - [ ] The `parse_block_html` option.
//! - [x] A line of only a span tag, like `<img>`, starting a paragraph.
//! - [x] HTML spans, with Markdown in them.
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
//! - [x] Inline math, `$$…$$`.
//!
//! ## Non-content elements
//!
//! - [x] Block attribute lists, on the line after a block or the line
//!   before it, or alone between blocks for the one after.
//! - [ ] A block attribute list after a list item ending the list. It goes
//!   to the list, but the items after it stay in the same list.
//! - [x] Span attribute lists, right after emphasis, a link, an image or a
//!   code span.
//! - [x] List item attribute lists, `* {:.cls} item`.
//! - [ ] Definition attribute lists, as there are no definition lists.
//! - [x] The empty span attribute list, `{::}`, separating spans.
//! - [ ] Attribute list definitions, `{:name: .class}`. Dropped, never
//!   applied.
//! - [ ] The `comment` extension. Its tags are dropped on lines of their
//!   own, read as text inside a line, and its body is always read.
//! - [ ] The `nomarkdown` extension. Its tags are dropped and its body is
//!   read as Markdown.
//! - [ ] The `options` extension. Dropped.
//!
//! ## Converter
//!
//! Not syntax, but what Jekyll sites lean on.
//!
//! - [x] Header ids, made the way kramdown's GFM parser, Jekyll's default,
//!   makes them, from the heading's text without its markup.
//! - [x] `{:toc}` tables of contents, nested by level, leaving out headings
//!   with the class `no_toc`, and links inside headings.

#![warn(missing_docs)]

mod attributes;
mod headings;
mod options;
mod read;
mod source;
mod toc;

use std::borrow::Cow;
use std::fmt;

pub use attributes::Attributes;
pub use options::{Extensions, Options};
pub use pulldown_cmark;
use pulldown_cmark::Event;

/// Markdown written for kramdown, ready to read.
#[derive(Debug, Clone)]
pub struct Document<'a> {
    text: Cow<'a, str>,
    options: Options,
}

impl<'a> Document<'a> {
    /// Reads `markdown` with `options`, which may be just pulldown-cmark's,
    /// for every kramdown extension.
    pub fn new(markdown: &'a str, options: impl Into<Options>) -> Self {
        let options = options.into();
        Self {
            text: source::prepare(markdown, options.kramdown),
            options,
        }
    }

    /// The document's events, each with the attributes kramdown gives it.
    ///
    /// They're read as they're asked for, a block at a time, as attribute
    /// lists after a block are for it. With tables of contents, they're all
    /// read first, for the tables to link to the headings after them.
    pub fn events(&self) -> Events<'_> {
        let reader = read::Reader::new(&self.text, self.options);
        let items = if self
            .options
            .kramdown
            .contains(Extensions::ENABLE_TABLES_OF_CONTENTS)
        {
            Items::Read(toc::Toc::replace_lists(reader.collect()).into_iter())
        } else {
            Items::Streamed(Box::new(reader))
        };
        Events(items)
    }
}

/// The events of a [`Document`], each with its attributes.
pub struct Events<'a>(Items<'a>);

/// Where the events come from.
enum Items<'a> {
    /// Read as they're asked for.
    Streamed(Box<read::Reader<'a>>),
    /// Read all at once.
    Read(std::vec::IntoIter<read::Item<'a>>),
}

impl<'a> Iterator for Events<'a> {
    type Item = (Event<'a>, Attributes<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let item = match &mut self.0 {
            Items::Streamed(reader) => reader.next(),
            Items::Read(items) => items.next(),
        };
        item.map(|item| (item.event, item.lists.attributes))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            Items::Streamed(reader) => reader.size_hint(),
            Items::Read(items) => items.size_hint(),
        }
    }
}

impl fmt::Debug for Events<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Events").finish_non_exhaustive()
    }
}

//! Writes events out as HTML, the way kramdown's converter writes the
//! elements they stand for, attributes included.
//!
//! ```
//! use pulldown_kram::{Document, Extensions, html};
//!
//! let document = Document::new("# Hi\n\nSee *this*{:.new}.\n", Extensions::all());
//! let mut out = String::new();
//! html::push_html(&mut out, document.events());
//! assert_eq!(out, "<h1 id=\"hi\">Hi</h1>\n<p>See <em class=\"new\">this</em>.</p>\n");
//! ```
//!
//! Like kramdown, footnotes are numbered in the order they're first referred
//! to, and written after the rest, with links back to each reference. Their
//! definitions are held until then, while everything else is written as it
//! comes. A math block alone in its paragraph is written as `\[…\]`, inline
//! math as `\(…\)`, for MathJax to render. Code isn't highlighted: a code
//! block's language is its class, `language-…`, on its `<code>`. An empty
//! table cell holds a no-break space. HTML in the document is written as
//! it is.
//!
//! What kramdown's converter options change isn't followed, like
//! `footnote_nr`, nor are its ids for the links of tables of contents.
//! References to footnotes never defined can only come with pulldown-cmark's
//! `ENABLE_OLD_FOOTNOTES`, and link to nothing.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io;

use pulldown_cmark::{
    Alignment, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Tag, TagEnd,
};

use crate::Attributes;

/// An event, with its attributes.
type Item<'a> = (Event<'a>, Attributes<'a>);

/// Writes the events of `iter` as HTML onto the end of `s`.
pub fn push_html<'a, I>(s: &mut String, iter: I)
where
    I: Iterator<Item = (Event<'a>, Attributes<'a>)>,
{
    write_html_fmt(s, iter).expect("writing to a String to succeed");
}

/// Writes the events of `iter` as HTML to `writer`, failing as it fails.
pub fn write_html_fmt<'a, I, W>(writer: W, iter: I) -> fmt::Result
where
    I: Iterator<Item = (Event<'a>, Attributes<'a>)>,
    W: fmt::Write,
{
    let mut html = HtmlWriter {
        out: writer,
        footnotes: Footnotes::default(),
        table: Table::default(),
    };
    html.write(Lookahead::new(iter))?;
    html.write_footnotes()
}

/// Writes the events of `iter` as HTML to `writer`, returning the first
/// error it gives. The writer isn't buffered here, so a buffered one, like
/// `std::io::BufWriter`, saves many small writes.
pub fn write_html_io<'a, I, W>(writer: W, iter: I) -> io::Result<()>
where
    I: Iterator<Item = (Event<'a>, Attributes<'a>)>,
    W: io::Write,
{
    let mut writer = IoWriter {
        inner: writer,
        error: None,
    };
    write_html_fmt(&mut writer, iter).map_err(|fmt::Error| {
        writer
            .error
            .take()
            .unwrap_or_else(|| io::Error::other(fmt::Error))
    })
}

/// An `io::Write` as a `fmt::Write`, keeping the error it gave.
struct IoWriter<W> {
    inner: W,
    error: Option<io::Error>,
}

impl<W: io::Write> fmt::Write for IoWriter<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.inner.write_all(s.as_bytes()).map_err(|error| {
            self.error = Some(error);
            fmt::Error
        })
    }
}

/// Events, with the next few in view.
struct Lookahead<'a, I> {
    iter: I,
    ahead: VecDeque<Item<'a>>,
}

impl<'a, I: Iterator<Item = Item<'a>>> Lookahead<'a, I> {
    fn new(iter: I) -> Self {
        Self {
            iter,
            ahead: VecDeque::new(),
        }
    }

    fn next(&mut self) -> Option<Item<'a>> {
        self.ahead.pop_front().or_else(|| self.iter.next())
    }

    /// The event `n` after the next one, `0` being the next.
    fn peek(&mut self, n: usize) -> Option<&Event<'a>> {
        while self.ahead.len() <= n {
            self.ahead.push_back(self.iter.next()?);
        }
        Some(&self.ahead[n].0)
    }

    /// The rest of the element whose start was just taken, up to its end,
    /// which is taken but left out.
    fn element(&mut self) -> Vec<Item<'a>> {
        let mut content = Vec::new();
        let mut depth = 1usize;
        while let Some(item) = self.next() {
            match item.0 {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            content.push(item);
        }
        content
    }

    /// If the paragraph just started holds only math, that math, taken
    /// along with the paragraph's end.
    fn math_alone(&mut self) -> Option<CowStr<'a>> {
        let alone = matches!(self.peek(0), Some(Event::DisplayMath(_)))
            && matches!(self.peek(1), Some(Event::End(TagEnd::Paragraph)));
        if !alone {
            return None;
        }
        let (Event::DisplayMath(math), _) = self.next()? else {
            return None;
        };
        self.next();
        Some(math)
    }
}

/// Footnotes, numbered in the order they're first referred to, and what
/// they hold, kept to be written after the rest.
#[derive(Default)]
struct Footnotes<'a> {
    /// Those referred to, in the order of their first references.
    referred: Vec<Referred<'a>>,
    /// The index in `referred` of each, by its label in lowercase, as
    /// pulldown-cmark matches labels ignoring case.
    indices: HashMap<String, usize>,
    /// What each footnote defined but not yet written holds, by its label
    /// in lowercase.
    definitions: HashMap<String, Vec<Item<'a>>>,
    /// How many of those referred to have been looked at for writing.
    looked_at: usize,
}

/// A footnote to write.
struct Footnote<'a> {
    /// Its index among those referred to.
    index: usize,
    /// Its label, as first referred to.
    label: CowStr<'a>,
    /// What it holds.
    content: Vec<Item<'a>>,
}

/// A footnote referred to.
struct Referred<'a> {
    /// Its label, as first referred to.
    label: CowStr<'a>,
    /// How many times it's been referred to.
    references: usize,
}

/// A reference to a footnote.
struct Reference<'a> {
    /// The footnote's label, as first referred to.
    label: CowStr<'a>,
    /// The footnote's number, from 1.
    number: usize,
    /// Which reference to the footnote this is, from 0.
    nth: usize,
}

impl<'a> Footnotes<'a> {
    /// Keeps what the footnote `label` holds, unless it's been defined
    /// already, as the first definition wins.
    fn define(&mut self, label: &str, content: Vec<Item<'a>>) {
        self.definitions
            .entry(label.to_lowercase())
            .or_insert(content);
    }

    /// Counts a reference to the footnote `label`, numbering it if it's the
    /// first.
    fn refer(&mut self, label: CowStr<'a>) -> Reference<'a> {
        let referred = &mut self.referred;
        let index = *self.indices.entry(label.to_lowercase()).or_insert_with(|| {
            referred.push(Referred {
                label,
                references: 0,
            });
            referred.len() - 1
        });
        let referred = &mut self.referred[index];
        referred.references += 1;
        Reference {
            label: referred.label.clone(),
            number: index + 1,
            nth: referred.references - 1,
        }
    }

    /// The next footnote referred to that's defined, to write. Footnotes
    /// first referred to while it's written come after it.
    fn next_to_write(&mut self) -> Option<Footnote<'a>> {
        while let Some(referred) = self.referred.get(self.looked_at) {
            let index = self.looked_at;
            self.looked_at += 1;
            if let Some(content) = self.definitions.remove(&referred.label.to_lowercase()) {
                return Some(Footnote {
                    index,
                    label: referred.label.clone(),
                    content,
                });
            }
        }
        None
    }

    /// How many times the footnote at `index` has been referred to.
    fn references(&self, index: usize) -> usize {
        self.referred[index].references
    }
}

/// The table being written.
#[derive(Default)]
struct Table {
    /// The alignment of its columns.
    alignments: Vec<Alignment>,
    /// The column of the next cell in the row.
    column: usize,
    /// Whether its body has started, after its header row.
    in_body: bool,
}

impl Table {
    /// Starts a row, returning whether it starts the body.
    fn start_row(&mut self) -> bool {
        self.column = 0;
        !std::mem::replace(&mut self.in_body, true)
    }

    /// Starts the header row.
    fn start_header(&mut self) {
        self.column = 0;
    }

    /// The name of a cell in the current row.
    fn cell(&self) -> &'static str {
        if self.in_body { "td" } else { "th" }
    }

    /// The style aligning the next cell, if it's aligned, moving on to the
    /// cell after it.
    fn next_alignment(&mut self) -> Option<&'static str> {
        let style = match self.alignments.get(self.column) {
            Some(Alignment::Left) => Some("text-align: left"),
            Some(Alignment::Center) => Some("text-align: center"),
            Some(Alignment::Right) => Some("text-align: right"),
            Some(Alignment::None) | None => None,
        };
        self.column += 1;
        style
    }
}

/// Writes events as HTML to `out`.
struct HtmlWriter<'a, W> {
    out: W,
    footnotes: Footnotes<'a>,
    table: Table,
}

impl<'a, W: fmt::Write> HtmlWriter<'a, W> {
    /// Writes `events`, keeping footnote definitions for the end.
    fn write<I: Iterator<Item = Item<'a>>>(&mut self, mut events: Lookahead<'a, I>) -> fmt::Result {
        while let Some((event, attributes)) = events.next() {
            match event {
                Event::Start(Tag::FootnoteDefinition(label)) => {
                    self.footnotes.define(&label, events.element());
                }
                Event::Start(Tag::Paragraph) => match events.math_alone() {
                    Some(math) => self.math_block(&math, &attributes)?,
                    None => self.open("p", &attributes, &[])?,
                },
                Event::Start(Tag::Image {
                    dest_url, title, ..
                }) => self.image(&dest_url, &title, &attributes, events.element())?,
                Event::Start(Tag::CodeBlock(kind)) => {
                    self.code_block(&kind, attributes, events.element())?;
                }
                // Metadata isn't content.
                Event::Start(Tag::MetadataBlock(_)) => {
                    events.element();
                }
                // kramdown writes an empty cell with a no-break space.
                Event::Start(Tag::TableCell)
                    if matches!(events.peek(0), Some(Event::End(TagEnd::TableCell))) =>
                {
                    self.event(Event::Start(Tag::TableCell), &attributes)?;
                    self.out.write_char('\u{a0}')?;
                }
                event => self.event(event, &attributes)?,
            }
        }
        Ok(())
    }

    /// Writes `event`, with its `attributes`.
    fn event(&mut self, event: Event<'a>, attributes: &Attributes) -> fmt::Result {
        match event {
            Event::Start(tag) => self.start(tag, attributes),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.escaped(&text),
            Event::Code(code) => {
                self.open("code", attributes, &[])?;
                self.escaped(&code)?;
                self.out.write_str("</code>")
            }
            Event::InlineMath(math) => {
                self.out.write_str("\\(")?;
                self.escaped(&math)?;
                self.out.write_str("\\)")
            }
            Event::DisplayMath(math) => {
                self.out.write_str("\\[")?;
                self.escaped(&math)?;
                self.out.write_str("\\]")
            }
            Event::Html(html) | Event::InlineHtml(html) => self.out.write_str(&html),
            Event::FootnoteReference(label) => self.footnote_reference(label),
            Event::SoftBreak => self.out.write_str("\n"),
            Event::HardBreak => self.out.write_str("<br />\n"),
            Event::Rule => {
                self.empty("hr", attributes, &[])?;
                self.out.write_str("\n")
            }
            Event::TaskListMarker(checked) => {
                let mut pairs = vec![
                    ("type", "checkbox"),
                    ("class", "task-list-item-checkbox"),
                    ("disabled", "disabled"),
                ];
                if checked {
                    pairs.push(("checked", "checked"));
                }
                self.empty("input", &Attributes::default(), &pairs)
            }
        }
    }

    /// Writes the start of `tag`, with its `attributes`.
    fn start(&mut self, tag: Tag<'a>, attributes: &Attributes) -> fmt::Result {
        let name = match tag {
            Tag::Paragraph => "p",
            Tag::BlockQuote(_) => "blockquote",
            Tag::List(None) => "ul",
            Tag::List(Some(1)) => "ol",
            Tag::List(Some(start)) => {
                return self.open("ol", attributes, &[("start", &start.to_string())]);
            }
            Tag::Item => "li",
            Tag::DefinitionList => "dl",
            Tag::DefinitionListTitle => "dt",
            Tag::DefinitionListDefinition => "dd",
            Tag::Emphasis => "em",
            Tag::Strong => "strong",
            Tag::Strikethrough => "del",
            Tag::Superscript => "sup",
            Tag::Subscript => "sub",
            Tag::Heading {
                level,
                id,
                classes,
                attrs,
            } => return self.heading(level, attributes, id, classes, attrs),
            Tag::Link {
                link_type,
                dest_url,
                title,
                ..
            } => return self.link(link_type, &dest_url, &title, attributes),
            Tag::Table(alignments) => {
                self.table = Table {
                    alignments,
                    ..Table::default()
                };
                "table"
            }
            Tag::TableHead => {
                self.table.start_header();
                self.out.write_str("<thead>")?;
                "tr"
            }
            Tag::TableRow => {
                if self.table.start_row() {
                    self.out.write_str("<tbody>")?;
                }
                "tr"
            }
            Tag::TableCell => {
                let cell = self.table.cell();
                return match self.table.next_alignment() {
                    Some(style) => self.open(cell, attributes, &[("style", style)]),
                    None => self.open(cell, attributes, &[]),
                };
            }
            // HTML is written as it comes, and the rest whole by `write`.
            Tag::HtmlBlock
            | Tag::CodeBlock(_)
            | Tag::Image { .. }
            | Tag::FootnoteDefinition(_)
            | Tag::MetadataBlock(_) => return Ok(()),
        };
        self.open(name, attributes, &[])
    }

    /// Writes the end of `tag`.
    fn end(&mut self, tag: TagEnd) -> fmt::Result {
        self.out.write_str(match tag {
            TagEnd::Paragraph => "</p>\n",
            TagEnd::Heading(level) => return writeln!(self.out, "</{level}>"),
            TagEnd::BlockQuote(_) => "</blockquote>\n",
            TagEnd::List(false) => "</ul>\n",
            TagEnd::List(true) => "</ol>\n",
            TagEnd::Item => "</li>\n",
            TagEnd::DefinitionList => "</dl>\n",
            TagEnd::DefinitionListTitle => "</dt>\n",
            TagEnd::DefinitionListDefinition => "</dd>\n",
            TagEnd::Emphasis => "</em>",
            TagEnd::Strong => "</strong>",
            TagEnd::Strikethrough => "</del>",
            TagEnd::Superscript => "</sup>",
            TagEnd::Subscript => "</sub>",
            TagEnd::Link => "</a>",
            TagEnd::Table if self.table.in_body => "</tbody></table>\n",
            TagEnd::Table => "</table>\n",
            TagEnd::TableHead => "</tr></thead>\n",
            TagEnd::TableRow => "</tr>\n",
            TagEnd::TableCell if self.table.in_body => "</td>",
            TagEnd::TableCell => "</th>",
            TagEnd::HtmlBlock
            | TagEnd::CodeBlock
            | TagEnd::Image
            | TagEnd::FootnoteDefinition
            | TagEnd::MetadataBlock(_) => "",
        })
    }

    /// Writes the start of a heading, with pulldown-cmark's own attributes
    /// for it under those of attribute lists.
    fn heading(
        &mut self,
        level: HeadingLevel,
        attributes: &Attributes,
        id: Option<CowStr<'a>>,
        classes: Vec<CowStr<'a>>,
        pairs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
    ) -> fmt::Result {
        let mut all = Attributes {
            id: attributes.id.clone().or(id),
            classes,
            pairs: pairs
                .into_iter()
                .map(|(key, value)| (key, value.unwrap_or_else(|| "".into())))
                .collect(),
        };
        all.classes.extend(attributes.classes.iter().cloned());
        all.pairs.extend(attributes.pairs.iter().cloned());
        self.open(level, &all, &[])
    }

    /// Writes the start of a link, an email address's to `mailto:` it.
    fn link(
        &mut self,
        link_type: LinkType,
        dest_url: &str,
        title: &str,
        attributes: &Attributes,
    ) -> fmt::Result {
        let mailto;
        let href = if link_type == LinkType::Email && !dest_url.starts_with("mailto:") {
            mailto = format!("mailto:{dest_url}");
            &mailto
        } else {
            dest_url
        };
        let mut pairs = vec![("href", href)];
        if !title.is_empty() {
            pairs.push(("title", title));
        }
        self.open("a", attributes, &pairs)
    }

    /// Writes an image, its `content` written as its alternative text.
    fn image(
        &mut self,
        dest_url: &str,
        title: &str,
        attributes: &Attributes,
        content: Vec<Item<'a>>,
    ) -> fmt::Result {
        let mut alt = String::new();
        for (event, _) in content {
            if let Event::Text(text) | Event::Code(text) | Event::InlineMath(text) = event {
                alt.push_str(&text);
            }
        }
        let mut pairs = vec![("src", dest_url), ("alt", &alt)];
        if !title.is_empty() {
            pairs.push(("title", title));
        }
        self.empty("img", attributes, &pairs)
    }

    /// Writes `math` as a block, in a div if it has attributes.
    fn math_block(&mut self, math: &str, attributes: &Attributes) -> fmt::Result {
        let in_div = !attributes.is_empty();
        if in_div {
            self.open("div", attributes, &[])?;
        }
        self.out.write_str("\\[")?;
        self.escaped(math)?;
        self.out.write_str("\\]\n")?;
        if in_div {
            self.out.write_str("</div>\n")?;
        }
        Ok(())
    }

    /// Writes a code block, as kramdown does without a highlighter: its
    /// language is the first `language-` class, the fence's coming first,
    /// written on the `code`, with the rest of the attributes on the `pre`.
    /// The fence's language is without the highlighter's options after a
    /// `?`, like `php?start_inline=1`. One newline ends the code, whatever
    /// it ends with, and the class `show-whitespaces` marks its spaces and
    /// tabs.
    fn code_block(
        &mut self,
        kind: &CodeBlockKind,
        mut attributes: Attributes<'a>,
        content: Vec<Item<'a>>,
    ) -> fmt::Result {
        let mut code = String::new();
        for (event, _) in content {
            if let Event::Text(text) = event {
                code.push_str(&text);
            }
        }
        let code = code.strip_suffix('\n').unwrap_or(&code);
        if let CodeBlockKind::Fenced(info) = kind
            && let Some(language) = info.split_whitespace().next()
        {
            let language = language
                .char_indices()
                .skip(1)
                .find(|&(_, c)| c == '?')
                .map_or(language, |(i, _)| &language[..i]);
            let class = format!("language-{language}");
            if !attributes.has_class(&class) {
                attributes.classes.insert(0, class.into());
            }
        }
        let show_whitespaces = attributes.has_class("show-whitespaces");
        let language = attributes
            .classes
            .iter()
            .position(|class| class.starts_with("language-"))
            .map(|i| attributes.classes.remove(i));

        self.open("pre", &attributes, &[])?;
        match &language {
            Some(language) => self.open("code", &Attributes::default(), &[("class", language)])?,
            None => self.out.write_str("<code>")?,
        }
        if show_whitespaces {
            self.whitespaces(code)?;
        } else {
            self.escaped(code)?;
        }
        self.out.write_str("\n</code></pre>\n")
    }

    /// Writes a reference to the footnote `label`, numbered in the order of
    /// first references, as kramdown does.
    fn footnote_reference(&mut self, label: CowStr<'a>) -> fmt::Result {
        let Reference { label, number, nth } = self.footnotes.refer(label);
        self.out.write_str("<sup id=\"")?;
        self.reference_id(&label, nth)?;
        self.out.write_str("\"><a href=\"#fn:")?;
        self.escaped(&label)?;
        write!(
            self.out,
            "\" class=\"footnote\" rel=\"footnote\" role=\"doc-noteref\">{number}</a></sup>"
        )
    }

    /// Writes the footnotes referred to, after the document, as kramdown
    /// does.
    fn write_footnotes(&mut self) -> fmt::Result {
        if self.footnotes.referred.is_empty() {
            return Ok(());
        }
        self.out
            .write_str("<div class=\"footnotes\" role=\"doc-endnotes\">\n<ol>\n")?;
        while let Some(footnote) = self.footnotes.next_to_write() {
            self.footnote(footnote)?;
        }
        self.out.write_str("</ol>\n</div>\n")
    }

    /// Writes `footnote`, with links back to each reference to it in its
    /// last paragraph, or in one of their own if it doesn't end with one.
    fn footnote(&mut self, footnote: Footnote<'a>) -> fmt::Result {
        let Footnote {
            index,
            label,
            mut content,
        } = footnote;
        self.out.write_str("<li id=\"fn:")?;
        self.escaped(&label)?;
        self.out.write_str("\">\n")?;
        let in_paragraph = matches!(content.last(), Some((Event::End(TagEnd::Paragraph), _)));
        if in_paragraph {
            content.pop();
        }
        self.write(Lookahead::new(content.into_iter()))?;
        if !in_paragraph {
            self.out.write_str("<p>")?;
        }
        for nth in 0..self.footnotes.references(index) {
            if in_paragraph || nth > 0 {
                self.out.write_char('\u{a0}')?;
            }
            self.out.write_str("<a href=\"#")?;
            self.reference_id(&label, nth)?;
            self.out
                .write_str("\" class=\"reversefootnote\" role=\"doc-backlink\">\u{21a9}")?;
            if nth > 0 {
                write!(self.out, "<sup>{}</sup>", nth + 1)?;
            }
            self.out.write_str("</a>")?;
        }
        self.out.write_str("</p>\n</li>\n")
    }

    /// Writes the id of the `nth` reference to the footnote `label`, from 0:
    /// `fnref:label` for the first, and `fnref:label:1` and on after.
    fn reference_id(&mut self, label: &str, nth: usize) -> fmt::Result {
        self.out.write_str("fnref:")?;
        self.escaped(label)?;
        if nth > 0 {
            write!(self.out, ":{nth}")?;
        }
        Ok(())
    }

    /// Writes a start tag, `<name …>`.
    fn open(
        &mut self,
        name: impl fmt::Display,
        attributes: &Attributes,
        pairs: &[(&str, &str)],
    ) -> fmt::Result {
        write!(self.out, "<{name}")?;
        self.attributes(attributes, pairs)?;
        self.out.write_str(">")
    }

    /// Writes an element with no content or end tag, `<name … />`.
    fn empty(
        &mut self,
        name: &str,
        attributes: &Attributes,
        pairs: &[(&str, &str)],
    ) -> fmt::Result {
        write!(self.out, "<{name}")?;
        self.attributes(attributes, pairs)?;
        self.out.write_str(" />")
    }

    /// Writes `attributes`, with the element's own `pairs` before their
    /// pairs, as HTML attributes. A blank id is left out, as kramdown leaves
    /// it out.
    fn attributes(&mut self, attributes: &Attributes, pairs: &[(&str, &str)]) -> fmt::Result {
        if let Some(id) = attributes.id.as_ref().filter(|id| !id.trim().is_empty()) {
            self.attribute("id", id)?;
        }
        if !attributes.classes.is_empty() {
            self.out.write_str(" class=\"")?;
            for (i, class) in attributes.classes.iter().enumerate() {
                if i > 0 {
                    self.out.write_str(" ")?;
                }
                self.escaped(class)?;
            }
            self.out.write_str("\"")?;
        }
        for &(key, value) in pairs {
            self.attribute(key, value)?;
        }
        for (key, value) in &attributes.pairs {
            self.attribute(key, value)?;
        }
        Ok(())
    }

    /// Writes ` key="value"`.
    fn attribute(&mut self, key: &str, value: &str) -> fmt::Result {
        write!(self.out, " {key}=\"")?;
        self.escaped(value)?;
        self.out.write_str("\"")
    }

    /// Writes `text` with HTML's special characters escaped.
    fn escaped(&mut self, text: &str) -> fmt::Result {
        let mut rest = text;
        while let Some(at) = rest.find(['&', '<', '>', '"']) {
            self.out.write_str(&rest[..at])?;
            self.out.write_str(match rest.as_bytes()[at] {
                b'&' => "&amp;",
                b'<' => "&lt;",
                b'>' => "&gt;",
                _ => "&quot;",
            })?;
            rest = &rest[at + 1..];
        }
        self.out.write_str(rest)
    }

    /// Writes `code`, escaped, with each space and tab in a span saying
    /// whether it starts a line, ends one or is in between, as kramdown
    /// marks them.
    fn whitespaces(&mut self, code: &str) -> fmt::Result {
        for (n, line) in code.split('\n').enumerate() {
            if n > 0 {
                self.out.write_char('\n')?;
            }
            let mut rest = line;
            let mut start = true;
            while !rest.is_empty() {
                let text = rest.find([' ', '\t']).unwrap_or(rest.len());
                self.escaped(&rest[..text])?;
                start &= text == 0;
                rest = &rest[text..];
                let run = rest.find(|c| c != ' ' && c != '\t').unwrap_or(rest.len());
                let place = if start {
                    "-l"
                } else if run == rest.len() {
                    "-r"
                } else {
                    ""
                };
                for c in rest[..run].chars() {
                    match c {
                        '\t' => write!(self.out, "<span class=\"ws-tab{place}\">\t</span>")?,
                        _ => write!(self.out, "<span class=\"ws-space{place}\">&#8901;</span>")?,
                    }
                }
                rest = &rest[run..];
                start = false;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Extensions};

    fn html(markdown: &str) -> String {
        let mut out = String::new();
        push_html(
            &mut out,
            Document::new(markdown, Extensions::all()).events(),
        );
        out
    }

    /// The text between each `start` in `html` and the next `end` after it.
    fn between<'h>(html: &'h str, start: &str, end: char) -> Vec<&'h str> {
        html.match_indices(start)
            .map(|(i, _)| {
                let rest = &html[i + start.len()..];
                &rest[..rest.find(end).unwrap()]
            })
            .collect()
    }

    #[test]
    fn push_html_footnotes_are_numbered_by_first_reference_and_written_last() {
        let out = html("a[^b] c[^A] d[^b]\n\n[^a]: x[^c]\n\n[^b]: y\n\n[^c]: z\n");
        assert!(out.starts_with("<p>a<sup"));
        assert!(out.contains("</p>\n<div class=\"footnotes\""));

        // `c` is first referred to inside `a`, so comes after it, and `A`
        // matches `a` whatever its case.
        let numbers = between(&out, "role=\"doc-noteref\">", '<');
        assert_eq!(numbers, ["1", "2", "1", "3"]);
        let footnotes = between(&out, "<li id=\"fn:", '"');
        assert_eq!(footnotes, ["b", "A", "c"]);

        // A footnote's second reference has an id of its own, linked back to.
        let references = between(&out, "<sup id=\"", '"');
        assert_eq!(references, ["fnref:b", "fnref:A", "fnref:b:1", "fnref:c"]);
        let backlinks = between(&out, "<a href=\"#fnref:", '"');
        assert_eq!(backlinks, ["b", "b:1", "A", "c"]);
    }

    #[test]
    fn push_html_a_footnote_not_ending_in_a_paragraph_gets_one_for_its_backlink() {
        let out = html("a[^1]\n\n[^1]: > q\n");
        let footnote = &out[out.find("<li").unwrap()..];
        assert_eq!(
            footnote,
            "<li id=\"fn:1\">\n<blockquote><p>q</p>\n</blockquote>\n\
             <p><a href=\"#fnref:1\" class=\"reversefootnote\" role=\"doc-backlink\">\u{21a9}</a></p>\n\
             </li>\n</ol>\n</div>\n"
        );
    }

    #[test]
    fn push_html_math_alone_in_a_paragraph_is_a_block() {
        assert_eq!(html("$$x$$\n"), "\\[x\\]\n");
        assert_eq!(html("$$x$$\n{:.m}\n"), "<div class=\"m\">\\[x\\]\n</div>\n");
        assert_eq!(html("a $$x$$\n"), "<p>a \\(x\\)</p>\n");
    }

    #[test]
    fn push_html_tables_align_cells_and_start_a_body_after_the_header() {
        assert_eq!(
            html("a | b | c\n:-|:-:|-\n1 | 2 | 3\n"),
            "<table><thead><tr>\
             <th style=\"text-align: left\">a</th>\
             <th style=\"text-align: center\">b</th>\
             <th>c</th></tr></thead>\n\
             <tbody><tr>\
             <td style=\"text-align: left\">1</td>\
             <td style=\"text-align: center\">2</td>\
             <td>3</td></tr>\n\
             </tbody></table>\n"
        );
    }

    #[test]
    fn push_html_attribute_values_are_escaped() {
        assert_eq!(
            html("a\n{: title=\"x\\\"<y>&\"}\n"),
            "<p title=\"x&quot;&lt;y&gt;&amp;\">a</p>\n"
        );
    }

    #[test]
    fn push_html_a_code_blocks_language_goes_on_its_code() {
        // The fence's language first, the other classes on the `pre`.
        assert_eq!(
            html("```rb\nx\n```\n{: .language-py .k}\n"),
            "<pre class=\"language-py k\"><code class=\"language-rb\">x\n</code></pre>\n"
        );
        assert_eq!(
            html("    x\n{: .language-py}\n"),
            "<pre><code class=\"language-py\">x\n</code></pre>\n"
        );
    }

    #[test]
    fn push_html_a_fences_language_is_without_highlighter_options() {
        assert_eq!(
            html("```php?start_inline=1\nx\n```\n"),
            "<pre><code class=\"language-php\">x\n</code></pre>\n"
        );
        // A language of only a `?` and more is all language.
        assert_eq!(
            html("```?x\nx\n```\n"),
            "<pre><code class=\"language-?x\">x\n</code></pre>\n"
        );
    }

    #[test]
    fn write_html_io_writes_what_push_html_does_and_returns_its_errors() {
        let markdown = "# A\n\nb[^1]\n\n[^1]: c\n";
        let document = Document::new(markdown, Extensions::all());
        let mut bytes = Vec::new();
        write_html_io(&mut bytes, document.events()).unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), html(markdown));

        /// Takes as many more bytes as it holds, then fails.
        struct Full(usize);
        impl io::Write for Full {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                if buf.len() > self.0 {
                    return Err(io::ErrorKind::StorageFull.into());
                }
                self.0 -= buf.len();
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let error = write_html_io(Full(8), document.events()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::StorageFull);
    }
}

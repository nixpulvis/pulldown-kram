//! Reads pulldown-cmark's events the way kramdown means them. Attribute
//! lists are taken out of the text and attached to the elements they belong
//! to, stand-ins are swapped for what they stand for, and the structure is
//! corrected where kramdown's differs.

mod item;
mod link_definitions;
mod pending;

use std::collections::VecDeque;
use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{CowStr, Event, LinkType, OffsetIter, Parser, Tag, TagEnd};

use crate::abbreviations::{Abbreviation, Abbreviations};
use crate::attributes::AttributeLists;
use crate::definition_lists::DefinitionLists;
use crate::headings::HeaderIds;
use crate::list_items::ListItems;
use crate::source::{Mark, MarkdownElements, StandIn};
use crate::text::{self, Lines};
use crate::{Extensions, Options};
pub(crate) use item::Item;
use link_definitions::LinkDefinitions;
use pending::{Index, Pending};

/// Reads the events of a document as pulldown-cmark parses them, a block at
/// a time. A block is only emitted once the next one starts, since until
/// then an attribute list after it could still belong to it.
pub(crate) struct Reader<'a> {
    text: &'a str,
    kramdown: Extensions,
    events: Peekable<OffsetIter<'a>>,
    /// Where each line after the first starts.
    lines: Vec<usize>,
    pending: Pending<'a>,
    /// The indices of the last top-level block read and the one before it,
    /// which an attribute list may still belong to.
    block: Option<Index>,
    previous_block: Option<Index>,
    /// The attribute lists found alone between blocks, for the next block.
    next: Option<AttributeLists<'a>>,
    split: Option<Split>,
    /// For each strikethrough open, whether it's kramdown's, `~~a~~`,
    /// rather than `~a~`, which kramdown leaves as text.
    strikethroughs: Vec<bool>,
    ids: HeaderIds,
    link_definitions: LinkDefinitions<'a>,
    /// The stand-ins in the text, and the index of the next one not yet
    /// reached.
    marks: &'a [Mark<'a>],
    next_mark: usize,
    abbreviations: Abbreviations<'a, 'a>,
    /// The items ready to emit.
    ready: VecDeque<Item<'a>>,
}

/// A paragraph that a line of attribute lists has ended, so the lines after
/// that line start a new paragraph.
#[derive(Clone, Copy)]
struct Split {
    /// The index of the paragraph's start.
    paragraph: Index,
    /// Where the paragraph ends in the text, at the attribute lists.
    end: usize,
}

/// The element an attribute list belongs to, found by where it is.
enum Target {
    /// Right after a span, like `*a*{:.x}`: the span at this index.
    Span(Index),
    /// At the start of a list item, like `* {:.x} a`: the item at this index.
    Item(Index),
    /// Alone on its line. `lazy` when the line isn't indented or quoted
    /// like the lines before it.
    Line { lazy: bool },
}

impl Target {
    /// The extension that reads attribute lists in this place.
    fn extension(&self) -> Extensions {
        match self {
            Target::Span(_) => Extensions::ENABLE_SPAN_ATTRIBUTES,
            Target::Item(_) => Extensions::ENABLE_ITEM_ATTRIBUTES,
            Target::Line { .. } => Extensions::ENABLE_BLOCK_ATTRIBUTES,
        }
    }
}

impl<'a> Reader<'a> {
    /// Reads `text` with `options`, swapping each of the stand-ins in `marks`
    /// for what it stands for.
    pub fn new(text: &'a str, options: Options, marks: &'a [Mark<'a>]) -> Self {
        let markdown = options.parser_options();
        let events = Parser::new_ext(text, markdown).into_offset_iter();
        let link_definitions = if options
            .kramdown
            .contains(Extensions::ENABLE_BLOCK_ATTRIBUTES)
            && text.contains("{:")
        {
            LinkDefinitions::read(text, events.reference_definitions())
        } else {
            LinkDefinitions::default()
        };
        Self {
            text,
            kramdown: options.kramdown,
            events: events.peekable(),
            lines: text.match_indices('\n').map(|(i, _)| i + 1).collect(),
            pending: Pending::default(),
            block: None,
            previous_block: None,
            next: None,
            split: None,
            strikethroughs: Vec::new(),
            ids: HeaderIds::default(),
            link_definitions,
            marks,
            next_mark: 0,
            abbreviations: Abbreviations::new(&[]),
            ready: VecDeque::new(),
        }
    }

    /// Makes each use of one of `abbreviations` in the text an `<abbr>`.
    pub fn abbreviating(self, abbreviations: &'a [Abbreviation<'a>]) -> Self {
        Self {
            abbreviations: Abbreviations::new(abbreviations),
            ..self
        }
    }

    fn event(&mut self, event: Event<'a>, range: Range<usize>) {
        let (event, range) = self.as_kramdown(event, range);
        match event {
            Event::Start(Tag::TableHead) if self.take_mark(&range, true).is_some() => {
                self.skip_to(TagEnd::TableHead);
            }
            Event::Start(Tag::HtmlBlock) if let Some(stand_in) = self.take_mark(&range, true) => {
                self.skip_to(TagEnd::HtmlBlock);
                let html = match stand_in {
                    StandIn::Nothing => return,
                    StandIn::Comment(text) => {
                        CowStr::from(format!("<!-- {} -->\n", text.trim_end_matches('\n')))
                    }
                    StandIn::Raw(text) => CowStr::Borrowed(text),
                };
                self.event(Event::Start(Tag::HtmlBlock), range.clone());
                self.event(Event::Html(html), range.clone());
                self.event(Event::End(TagEnd::HtmlBlock), range);
            }
            Event::Start(Tag::Link {
                link_type: LinkType::Autolink,
                ..
            }) if let Some(stand_in) = self.take_mark(&range, false) => {
                self.skip_to(TagEnd::Link);
                let html = match stand_in {
                    StandIn::Nothing => return,
                    StandIn::Comment(text) => CowStr::from(format!("<!-- {text} -->")),
                    StandIn::Raw(text) => CowStr::Borrowed(text),
                };
                self.event(Event::InlineHtml(html), range);
            }
            Event::Start(Tag::DefinitionList)
                if self.continues(TagEnd::DefinitionList, range.start) =>
            {
                let list = self.pending.reopen();
                self.pending.item_mut(list).range.end = range.end;
            }
            Event::Start(Tag::List(None))
                if self
                    .kramdown
                    .contains(Extensions::ENABLE_MIXED_LIST_MARKERS)
                    && self.continues(TagEnd::List(false), range.start) =>
            {
                let list = self.pending.reopen();
                self.pending.item_mut(list).range.end = range.end;
            }
            Event::Start(_) | Event::Rule => {
                let top = self.pending.innermost().is_none();
                if top && let Some(block) = self.block {
                    self.settle(block);
                }
                let next = self.next.take();
                let definition = self.link_definitions.for_link(&event);
                let i = self.push(event, range);
                if let Some(attribute_lists) = definition {
                    self.pending.attach(i, attribute_lists);
                }
                if let Some(attribute_lists) = next {
                    self.pending.attach(i, attribute_lists);
                }
                if top {
                    self.previous_block = self.block;
                    self.block = Some(i);
                }
            }
            Event::End(TagEnd::Paragraph) if self.pending.is_empty_paragraph() => {
                self.next = None;
                let paragraph = self.pending.pop().expect("the paragraph");
                if self.pending.innermost().is_none() {
                    self.block = self.previous_block;
                }
                self.alone(paragraph);
            }
            Event::End(_) => {
                self.next = None;
                self.push(event, range);
            }
            Event::Text(_) if self.kramdown.attribute_lists() && !self.pending.in_code_block() => {
                match AttributeLists::parse(&self.text[range.start..]) {
                    Some((attribute_lists, len)) => {
                        self.attribute_lists(attribute_lists, len, event, range)
                    }
                    None => {
                        self.push(event, range);
                    }
                }
            }
            Event::SoftBreak
                if self
                    .kramdown
                    .contains(Extensions::ENABLE_BACKSLASH_LINE_BREAKS)
                    && self.take_backslash() =>
            {
                self.push(Event::HardBreak, range);
            }
            Event::InlineHtml(tag)
                if self.kramdown.contains(Extensions::ENABLE_RAW_HTML_SPANS)
                    && let Some(name) = Self::raw_span_name(&tag) =>
            {
                self.push(Event::InlineHtml(tag), range.clone());
                self.raw_span(name, range.end);
            }
            _ => {
                self.push(event, range);
            }
        }
    }

    /// The name of the HTML span `tag` opens, if kramdown leaves its text
    /// raw.
    fn raw_span_name(tag: &str) -> Option<&'static str> {
        const RAW: [&str; 10] = [
            "code", "kbd", "math", "option", "pre", "samp", "script", "style", "textarea", "var",
        ];
        let inside = tag.trim_end().strip_prefix('<')?.strip_suffix('>')?;
        if inside.ends_with('/') {
            return None;
        }
        let len = inside
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(inside.len());
        RAW.into_iter()
            .find(|name| name.eq_ignore_ascii_case(&inside[..len]))
    }

    /// Emits the text of the raw HTML span `name`, from `start` up to its
    /// closing tag, as raw HTML. It replaces the events pulldown-cmark read
    /// from that text, as long as they end before the closing tag.
    fn raw_span(&mut self, name: &str, start: usize) {
        let rest = &self.text[start..];
        let Some(close) = rest.match_indices("</").find_map(|(at, _)| {
            let after = &rest[at + "</".len()..];
            let named = after
                .get(..name.len())
                .is_some_and(|tag| tag.eq_ignore_ascii_case(name));
            let ends = after[name.len().min(after.len())..].starts_with(['>', ' ', '\t', '\n']);
            (named && ends).then_some(start + at)
        }) else {
            return;
        };
        // Skip the inline events pulldown-cmark read before the closing tag.
        let mut end = start;
        while let Some((event, next)) = self.events.peek()
            && Item::inline(event)
            && next.end <= close
        {
            end = next.end;
            self.events.next();
        }
        if self
            .events
            .peek()
            .is_some_and(|(_, next)| next.start >= close)
        {
            end = close;
        }
        if start < end {
            let raw = CowStr::Borrowed(&self.text[start..end]);
            self.push(Event::InlineHtml(raw), start..end);
        }
    }

    /// Takes the backslash off the end of the text just read, if the source
    /// line ends in two, which is kramdown's hard line break. Returns
    /// whether it did.
    fn take_backslash(&mut self) -> bool {
        let Some(last) = self.pending.last_index() else {
            return false;
        };
        let item = self.pending.item_mut(last);
        let Event::Text(text) = &item.event else {
            return false;
        };
        if !(text.ends_with('\\') && self.text[..item.range.end].ends_with("\\\\")) {
            return false;
        }
        let kept = text.len() - 1;
        item.event = Event::Text(match text {
            CowStr::Borrowed(text) => CowStr::Borrowed(&text[..kept]),
            text => CowStr::from(text[..kept].to_owned()),
        });
        true
    }

    /// What the stand-in at the start of `range` stands for, if there's one
    /// there of the kind `block` asks for, a block's or a span's. Marks it
    /// as reached.
    fn take_mark(&mut self, range: &Range<usize>, block: bool) -> Option<StandIn<'a>> {
        let marks = self.marks;
        while marks
            .get(self.next_mark)
            .is_some_and(|mark| mark.at < range.start)
        {
            self.next_mark += 1;
        }
        let mark = marks.get(self.next_mark)?;
        let starts = if block {
            mark.at < range.end && text::is_blank(&self.text[range.start..mark.at])
        } else {
            mark.at == range.start
        };
        if mark.block != block || !starts {
            return None;
        }
        self.next_mark += 1;
        Some(mark.stands_for)
    }

    /// Skips the events up to and including `end`.
    fn skip_to(&mut self, end: TagEnd) {
        for (event, _) in self.events.by_ref() {
            if event == Event::End(end) {
                break;
            }
        }
    }

    /// Whether kramdown reads the element starting at `start` as more of the
    /// one that just ended with `end`, with only blank lines between them.
    ///
    /// A definition list goes on past any blank lines, where
    /// pulldown-cmark's ends after a blank line that follows an empty
    /// definition. And a list goes on whichever of `*`, `+` and `-` marks
    /// its items, where pulldown-cmark's ends when the marker changes.
    fn continues(&self, end: TagEnd, start: usize) -> bool {
        self.pending.last().is_some_and(|last| {
            last.event == Event::End(end) && text::is_blank(&self.text[last.range.end..start])
        })
    }

    /// `event` corrected to what kramdown reads from the same source, where
    /// pulldown-cmark reads more into it.
    ///
    /// A single `~` and a single `$` are text. `$$` math in a paragraph
    /// with more in it is inline. Math loses the whitespace around it, and
    /// HTML loses the `markdown="1"` that kramdown reads.
    fn as_kramdown(&mut self, event: Event<'a>, range: Range<usize>) -> (Event<'a>, Range<usize>) {
        let text = |range: Range<usize>| {
            (
                Event::Text(CowStr::Borrowed(&self.text[range.clone()])),
                range,
            )
        };
        if let Event::Html(html) | Event::InlineHtml(html) = &event
            && self
                .kramdown
                .contains(Extensions::ENABLE_MARKDOWN_ATTRIBUTE)
            && let Some(without) = MarkdownElements::without_attribute(html)
        {
            let without = CowStr::from(without);
            let event = match event {
                Event::Html(_) => Event::Html(without),
                _ => Event::InlineHtml(without),
            };
            return (event, range);
        }
        match event {
            Event::Start(Tag::Strikethrough) => {
                let doubled = self.text[range.clone()].starts_with("~~");
                self.strikethroughs.push(doubled);
                if !doubled {
                    return text(range.start..range.start + 1);
                }
            }
            Event::End(TagEnd::Strikethrough) => {
                let doubled = self.strikethroughs.pop().expect("a strikethrough open");
                if !doubled {
                    return text(range.end - 1..range.end);
                }
            }
            Event::InlineMath(_) => return text(range),
            Event::DisplayMath(math) => {
                let math = Self::trim(math);
                let event = if self.alone_in_paragraph(range.end) {
                    Event::DisplayMath(math)
                } else {
                    Event::InlineMath(math)
                };
                return (event, range);
            }
            Event::Text(text)
                if self.kramdown.contains(Extensions::ENABLE_KRAMDOWN_ESCAPES)
                    && !self.pending.in_code_block()
                    && self.escaped_by_commonmark_only(&text, range.start) =>
            {
                // The text, from the backslash before it in the source.
                let source = &self.text[range.start - 1..range.end];
                let text = if source[1..] == *text {
                    CowStr::Borrowed(source)
                } else {
                    CowStr::from(format!("\\{text}"))
                };
                return (Event::Text(text), range.start - 1..range.end);
            }
            _ => {}
        }
        (event, range)
    }

    /// Whether `text`, read at `start`, starts with a character escaped by
    /// a backslash, which CommonMark escapes but kramdown doesn't.
    fn escaped_by_commonmark_only(&self, text: &str, start: usize) -> bool {
        let Some(c) = text.chars().next() else {
            return false;
        };
        let before = &self.text[..start];
        let backslashes = before.len() - before.trim_end_matches('\\').len();
        matches!(c, '&' | ',' | '/' | ';' | '?' | '@' | '^' | '%')
            && self.text[start..].starts_with(c)
            && !backslashes.is_multiple_of(2)
    }

    /// `text` without the whitespace around it.
    fn trim(text: CowStr<'a>) -> CowStr<'a> {
        match text {
            CowStr::Borrowed(text) => CowStr::Borrowed(text.trim()),
            text if text.trim().len() == text.len() => text,
            text => CowStr::from(text.trim().to_owned()),
        }
    }

    /// Whether the event about to be read, which ends at `end`, is all there
    /// is in its paragraph. That includes when a line of attribute lists
    /// right after it ends the paragraph.
    fn alone_in_paragraph(&mut self, end: usize) -> bool {
        self.pending
            .last()
            .is_some_and(|last| matches!(last.event, Event::Start(Tag::Paragraph)))
            && (matches!(self.events.peek(), Some((Event::End(TagEnd::Paragraph), _)))
                || self.attribute_lists_end_line(end))
    }

    /// Whether nothing follows `end` on its line, and the next line is only
    /// attribute lists, which belong to the block ending there.
    fn attribute_lists_end_line(&self, end: usize) -> bool {
        if !self.kramdown.contains(Extensions::ENABLE_BLOCK_ATTRIBUTES) {
            return false;
        }
        let mut lines = self.text[end..].split('\n');
        let (Some(rest), Some(next)) = (lines.next(), lines.next()) else {
            return false;
        };
        let next = next
            .trim_start_matches(|c: char| c.is_whitespace() || c == '>')
            .trim_end();
        rest.trim().is_empty() && AttributeLists::whole(next).is_some()
    }

    /// Attaches `attribute_lists`, the first `len` bytes of `event`'s text,
    /// to the element they belong to. If they belong to nothing, they stay
    /// in the text.
    fn attribute_lists(
        &mut self,
        attribute_lists: AttributeLists<'a>,
        len: usize,
        event: Event<'a>,
        range: Range<usize>,
    ) {
        let start = range.start;
        let end = start + len;
        let Some(target) = self.target(start, end) else {
            self.push(event, range);
            return;
        };
        let rest = self.consume(range, end);
        match target {
            Target::Span(i) => {
                self.pending.attach(i, attribute_lists);
                if let Some(rest) = rest {
                    self.push(
                        Event::Text(CowStr::Borrowed(&self.text[rest.clone()])),
                        rest,
                    );
                }
            }
            Target::Item(i) => {
                self.pending.attach(i, attribute_lists);
                if let Some(rest) = rest {
                    let text = self.text[rest.clone()].trim_start();
                    if !text.is_empty() {
                        let rest = rest.end - text.len()..rest.end;
                        self.push(Event::Text(CowStr::Borrowed(text)), rest);
                    }
                }
            }
            Target::Line { lazy } => self.line(attribute_lists, lazy, start),
        }
    }

    /// The element the attribute lists between `start` and `end` belong to,
    /// if any.
    fn target(&self, start: usize, end: usize) -> Option<Target> {
        let last = self.pending.last_index()?;
        let target = self
            .span_before(last, start)
            .or_else(|| self.alone_on_line(start, end))
            .or_else(|| self.item_starting(last))?;
        self.kramdown.contains(target.extension()).then_some(target)
    }

    /// The span that the attribute lists at `start` come right after, if
    /// `last` ends one there.
    fn span_before(&self, last: Index, start: usize) -> Option<Target> {
        let item = self.pending.item(last);
        if item.range.end != start {
            return None;
        }
        match item.event {
            Event::Code(_) => Some(Target::Span(last)),
            Event::End(
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Superscript
                | TagEnd::Subscript
                | TagEnd::Link
                | TagEnd::Image,
            ) => Some(Target::Span(self.pending.start_of(last))),
            _ => None,
        }
    }

    /// A line of their own, if the attribute lists between `start` and `end`
    /// have only indentation or quote markers before them on their line,
    /// and nothing after.
    fn alone_on_line(&self, start: usize, end: usize) -> Option<Target> {
        let lines = Lines(self.text);
        let lead = &self.text[lines.start(start)..start];
        let after = lines.rest_of_line(end);
        (text::is_blank(lead) && after.trim().is_empty()).then_some(Target::Line {
            lazy: lead.is_empty(),
        })
    }

    /// The list item at `last`, or the one whose first paragraph starts at
    /// `last`.
    fn item_starting(&self, last: Index) -> Option<Target> {
        let item = match self.pending.item(last).event {
            Event::Start(Tag::Item) => last,
            Event::Start(Tag::Paragraph) => last.previous()?,
            _ => return None,
        };
        self.pending
            .get(item)
            .is_some_and(|item| matches!(item.event, Event::Start(Tag::Item)))
            .then_some(Target::Item(item))
    }

    /// The definition list whose first term starts at `title`, if any.
    fn starts_definition_list(&self, title: Index) -> Option<Index> {
        let list = title.previous()?;
        self.pending
            .get(list)
            .is_some_and(|item| matches!(item.event, Event::Start(Tag::DefinitionList)))
            .then_some(list)
    }

    /// Attaches `attribute_lists`, alone on the line at `start`, to the
    /// element they belong to. That's one of these:
    ///
    /// - the paragraph they start,
    /// - the paragraph they end, with the lines after them split off into a
    ///   new one,
    /// - the definition list whose first term comes on the next line,
    /// - for a lazy line, the outermost list, quote, definition list or
    ///   footnote definition they're in.
    ///
    /// Lists on the lines of a link definition already belong to its links,
    /// so they're skipped.
    fn line(&mut self, attribute_lists: AttributeLists<'a>, lazy: bool, start: usize) {
        if self.link_definitions.on_line(start) {
            if matches!(self.pending.last(), Some(item) if matches!(item.event, Event::Start(Tag::Paragraph)))
            {
                self.skip_soft_break();
            }
            return;
        }
        if let Some(split) = self.split {
            // More lines of lists after a paragraph's belong to it too.
            self.pending.attach(split.paragraph, attribute_lists);
            self.skip_soft_break();
            return;
        }
        let Some(last) = self.pending.last_index() else {
            return;
        };
        match self.pending.item(last).event {
            Event::Start(Tag::TableCell) => self.table_row(last, attribute_lists),
            Event::Start(Tag::Paragraph) => {
                self.pending.attach(last, attribute_lists);
                self.skip_soft_break();
            }
            Event::Start(Tag::DefinitionListTitle)
                if let Some(list) = self.starts_definition_list(last)
                    && let Some((Event::SoftBreak, line)) = self.events.peek() =>
            {
                // The term starts on the next line.
                let term = line.end;
                self.events.next();
                self.pending.attach(list, attribute_lists);
                self.pending.item_mut(last).range.start = term;
            }
            Event::SoftBreak => {
                self.pending.pop();
                // Between terms, they belong to the term before, not the list.
                let in_term = self.pending.innermost().is_some_and(|i| {
                    matches!(
                        self.pending.item(i).event,
                        Event::Start(Tag::DefinitionListTitle)
                    )
                });
                let container = self
                    .pending
                    .open()
                    .find(|&i| match self.pending.item(i).event {
                        Event::Start(
                            Tag::List(_) | Tag::BlockQuote(_) | Tag::FootnoteDefinition(_),
                        ) => true,
                        Event::Start(Tag::DefinitionList) => !in_term,
                        _ => false,
                    });
                if let Some(container) = container.filter(|_| lazy) {
                    self.pending.attach(container, attribute_lists);
                    return;
                }
                let Some(block) = self.pending.innermost() else {
                    return;
                };
                self.pending.attach(block, attribute_lists);
                let paragraph =
                    matches!(self.pending.item(block).event, Event::Start(Tag::Paragraph));
                if paragraph && self.skip_soft_break() {
                    self.split = Some(Split {
                        paragraph: block,
                        end: start,
                    });
                }
            }
            _ => {
                if let Some(block) = self.pending.innermost() {
                    self.pending.attach(block, attribute_lists);
                }
            }
        }
    }

    /// Attaches `attribute_lists`, alone in the table cell `cell`, to the
    /// element they belong to. On the line right after a table they belong
    /// to the table, but pulldown-cmark reads that line as one more row, so
    /// the row is dropped. Anywhere else they belong to the cell.
    fn table_row(&mut self, cell: Index, attribute_lists: AttributeLists<'a>) {
        let mut rest = Vec::new();
        for (event, range) in self.events.by_ref() {
            let end = matches!(event, Event::End(TagEnd::TableRow));
            let empty = matches!(
                event,
                Event::Start(Tag::TableCell) | Event::End(TagEnd::TableCell)
            );
            rest.push((event, range));
            if end || !empty {
                break;
            }
        }
        let row = cell.previous();
        let first_cell = row
            .and_then(|row| self.pending.get(row))
            .is_some_and(|row| matches!(row.event, Event::Start(Tag::TableRow)));
        let row_ended = matches!(rest.last(), Some((Event::End(TagEnd::TableRow), _)));
        let table_ends = matches!(self.events.peek(), Some((Event::End(TagEnd::Table), _)));
        if let Some(row) = row
            && first_cell
            && row_ended
            && table_ends
        {
            self.pending.truncate(row);
            if let Some(table) = self.pending.innermost() {
                self.pending.attach(table, attribute_lists);
            }
            return;
        }
        self.pending.attach(cell, attribute_lists);
        for (event, range) in rest {
            self.event(event, range);
        }
    }

    /// Attaches the attribute lists of a paragraph made of nothing else to
    /// the block before it, if no blank line comes between them, or else to
    /// the block after it.
    fn alone(&mut self, paragraph: Item<'a>) {
        let Item {
            attribute_lists,
            range,
            ..
        } = paragraph;
        if let Some(last) = self.pending.last_index() {
            let block = match self.pending.item(last).event {
                Event::End(_) => Some(self.pending.start_of(last)),
                Event::Rule => Some(last),
                _ => None,
            };
            if let Some(block) = block
                && self.next_to(&self.pending.item(last).range, range.start)
            {
                self.pending.attach(block, attribute_lists);
                return;
            }
        }
        self.next = Some(attribute_lists);
    }

    /// Skips the events pulldown-cmark read from the attribute lists that
    /// start at `range` and end at `end`. Returns the range of any text left
    /// after them in the last event.
    fn consume(&mut self, mut range: Range<usize>, end: usize) -> Option<Range<usize>> {
        loop {
            if range.end > end {
                return Some(end..range.end);
            }
            match self.events.peek() {
                Some((
                    Event::Text(_) | Event::Code(_) | Event::InlineHtml(_) | Event::InlineMath(_),
                    next,
                )) if next.start < end => range = self.events.next()?.1,
                _ => return None,
            }
        }
    }

    /// Skips a soft break if it's next, returning whether there was one.
    fn skip_soft_break(&mut self) -> bool {
        let soft_break = matches!(self.events.peek(), Some((Event::SoftBreak, _)));
        if soft_break {
            self.events.next();
        }
        soft_break
    }

    /// Pushes `event`, first starting a new paragraph if a line of attribute
    /// lists ended the one it's in.
    fn push(&mut self, event: Event<'a>, range: Range<usize>) -> Index {
        if let Some(split) = self.split.take()
            && !matches!(event, Event::End(TagEnd::Paragraph))
        {
            let first = self.pending.item(split.paragraph).range.start..split.end;
            self.pending.push(Event::End(TagEnd::Paragraph), first);
            self.pending
                .push(Event::Start(Tag::Paragraph), range.start..range.start);
        }
        self.pending.push(event, range)
    }

    /// Makes the items before `end` ready to emit, after the passes that
    /// work on whole blocks: header ids, definition lists, list item
    /// paragraphs, abbreviations, task list classes and code languages.
    fn settle(&mut self, end: Index) {
        let mut items = self.pending.take_before(end);
        self.ids.assign(&mut items, self.text, self.kramdown);
        if self.kramdown.contains(Extensions::ENABLE_DEFINITION_LISTS) {
            DefinitionLists::new(self.text).rewrite(&mut items);
        }
        if self
            .kramdown
            .contains(Extensions::ENABLE_LIST_ITEM_PARAGRAPHS)
        {
            ListItems::new(self.text).rewrite(&mut items);
        }
        self.abbreviations.replace(&mut items);
        if self.kramdown.contains(Extensions::ENABLE_TASK_LISTS) {
            Item::mark_task_lists(&mut items);
        }
        if self.kramdown.contains(Extensions::ENABLE_LANGUAGE_CLASSES) {
            for item in &mut items {
                item.take_language_from_classes();
            }
        }
        // Taking `{#id}` off a heading can leave its text empty.
        self.ready.extend(
            items
                .into_iter()
                .filter(|item| !matches!(&item.event, Event::Text(t) if t.is_empty())),
        );
    }

    /// Whether the line at `start` comes right after the last line of `block`.
    fn next_to(&self, block: &Range<usize>, start: usize) -> bool {
        let end = block.start + self.text[block.clone()].trim_end().len();
        end > 0 && self.line_of(end - 1) + 1 == self.line_of(start)
    }

    fn line_of(&self, offset: usize) -> usize {
        self.lines.partition_point(|&start| start <= offset)
    }
}

impl<'a> Iterator for Reader<'a> {
    type Item = Item<'a>;

    fn next(&mut self) -> Option<Item<'a>> {
        loop {
            if let Some(item) = self.ready.pop_front() {
                return Some(item);
            }
            match self.events.next() {
                Some((event, range)) => self.event(event, range),
                None if !self.pending.is_empty() => self.settle(self.pending.end()),
                None => return None,
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.ready.len(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The items `text` reads as with every extension.
    fn read(text: &str) -> Vec<Item<'_>> {
        Reader::new(text, Options::from(Extensions::all()), &[]).collect()
    }

    /// The elements given classes, by tag, with their classes.
    fn classed(items: &[Item<'_>]) -> Vec<(&'static str, String)> {
        items
            .iter()
            .filter(|item| !item.attribute_lists.attributes.classes.is_empty())
            .map(|item| {
                let tag = match &item.event {
                    Event::Start(Tag::Paragraph) => "p",
                    Event::Start(Tag::Item) => "li",
                    Event::Start(Tag::Emphasis) => "em",
                    Event::Start(Tag::Strong) => "strong",
                    Event::Start(Tag::Strikethrough) => "del",
                    Event::Start(Tag::Link { .. }) => "a",
                    Event::Start(Tag::Image { .. }) => "img",
                    event => panic!("unexpected {event:?}"),
                };
                (tag, item.attribute_lists.attributes.classes.join(" "))
            })
            .collect()
    }

    /// The text of `items`, without markup.
    fn text(items: &[Item<'_>]) -> String {
        items
            .iter()
            .filter_map(|item| match &item.event {
                Event::Text(text) => Some(text.as_ref()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn reader_span_attribute_lists_go_to_the_span_they_follow() {
        let items = read("~~a~~{:.s} **b**{:.t} [c](d){:.u} ![e](f){:.v}\n");
        assert_eq!(
            classed(&items),
            [
                ("del", "s".into()),
                ("strong", "t".into()),
                ("a", "u".into()),
                ("img", "v".into())
            ]
        );
        assert_eq!(text(&items), "a b c e");
    }

    #[test]
    fn reader_text_after_span_attribute_lists_stays() {
        let items = read("*a*{:.x}b c\n");
        assert_eq!(classed(&items), [("em", "x".into())]);
        assert_eq!(text(&items), "ab c");
    }

    #[test]
    fn reader_a_single_tilde_ends_no_span() {
        let items = read("~a~{:.x}\n");
        assert!(classed(&items).is_empty());
        assert_eq!(text(&items), "~a~{:.x}");
    }

    #[test]
    fn reader_item_attribute_lists_in_a_loose_list() {
        let items = read("* {:.x}   a\n\n* b\n");
        assert_eq!(classed(&items), [("li", "x".into())]);
        assert_eq!(text(&items), "ab");
    }
}

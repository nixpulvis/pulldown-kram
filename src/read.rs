//! Reads pulldown-cmark's events, taking kramdown's attribute lists out of
//! the text and giving them to the elements they're for.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, CowStr, Event, OffsetIter, Options, Parser, Tag, TagEnd};

use crate::attributes::Lists;
use crate::headings::HeaderIds;

/// An event, where in the source it was read, and what kramdown's
/// attribute lists give it.
#[derive(Debug, Clone)]
pub(crate) struct Item<'a> {
    pub event: Event<'a>,
    pub range: Range<usize>,
    pub lists: Lists<'a>,
}

impl<'a> Item<'a> {
    pub fn new(event: Event<'a>, range: Range<usize>) -> Self {
        Self {
            event,
            range,
            lists: Lists::default(),
        }
    }

    /// Gives a code block without a language the one in a `language-` class.
    fn take_language_from_classes(&mut self) {
        let Event::Start(Tag::CodeBlock(kind)) = &mut self.event else {
            return;
        };
        if matches!(kind, CodeBlockKind::Fenced(info) if !info.trim().is_empty()) {
            return;
        }
        let language = self.lists.attributes.classes.iter().find_map(|class| {
            let language = class.strip_prefix("language-")?;
            Some(match class {
                Cow::Borrowed(class) => CowStr::Borrowed(&class["language-".len()..]),
                Cow::Owned(_) => CowStr::from(language.to_owned()),
            })
        });
        if let Some(language) = language {
            *kind = CodeBlockKind::Fenced(language);
        }
    }

    /// The index of the end of the element starting at `start` in `items`.
    pub fn end_of(items: &[Self], start: usize) -> usize {
        let mut depth = 0usize;
        for (i, item) in items.iter().enumerate().skip(start) {
            match item.event {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    depth -= 1;
                    if depth == 0 {
                        return i;
                    }
                }
                _ => {}
            }
        }
        items.len() - 1
    }
}

/// Reads the events of a document as pulldown-cmark gives them, a block at
/// a time: a block is given out once the next one starts, as only then is
/// it known that no attribute list after it is for it.
pub(crate) struct Reader<'a> {
    text: &'a str,
    events: Peekable<OffsetIter<'a>>,
    /// Where each line after the first starts.
    lines: Vec<usize>,
    /// The items read but not yet given out. Indices into them count every
    /// item read, starting from the first, which was `base`.
    pending: Vec<Pending<'a>>,
    base: usize,
    /// The indices of the elements open, innermost last.
    open: Vec<usize>,
    /// The indices of the last top-level block read, and the one before it,
    /// which an attribute list may still be for.
    block: Option<usize>,
    previous_block: Option<usize>,
    /// What lists alone between blocks give the next block.
    next: Option<Lists<'a>>,
    split: Option<Split>,
    ids: HeaderIds,
    /// The items ready to give out.
    ready: VecDeque<Item<'a>>,
}

/// An item read, but not given out until no more lists can be for it.
struct Pending<'a> {
    item: Item<'a>,
    /// For an end, the index of its start.
    start: Option<usize>,
}

/// A paragraph that lists on lines of their own have ended, for the lines
/// after them to start a new one.
#[derive(Clone, Copy)]
struct Split {
    /// The index of the paragraph's start.
    paragraph: usize,
    /// Where in the source the paragraph ends, at the lists.
    end: usize,
}

/// What an attribute list is for, by where it is.
enum Target {
    /// Right after a span, like `*a*{:.x}`: the span at this index.
    Span(usize),
    /// At the start of a list item, like `* {:.x} a`: the item at this index.
    Item(usize),
    /// Alone on its line, `lazy` if it isn't indented or quoted like the
    /// lines before it.
    Line { lazy: bool },
}

impl<'a> Reader<'a> {
    /// Reads `text` with `options`, less heading attributes, which read `{…}`
    /// after a heading differently from kramdown.
    pub fn new(text: &'a str, options: Options) -> Self {
        let options = options - Options::ENABLE_HEADING_ATTRIBUTES;
        Self {
            text,
            events: Parser::new_ext(text, options).into_offset_iter().peekable(),
            lines: text.match_indices('\n').map(|(i, _)| i + 1).collect(),
            pending: Vec::new(),
            base: 0,
            open: Vec::new(),
            block: None,
            previous_block: None,
            next: None,
            split: None,
            ids: HeaderIds::default(),
            ready: VecDeque::new(),
        }
    }

    fn event(&mut self, event: Event<'a>, range: Range<usize>) {
        match event {
            Event::Start(_) | Event::Rule => {
                let top = self.open.is_empty();
                if top && let Some(block) = self.block {
                    self.settle(block);
                }
                let next = self.next.take();
                let i = self.push(event, range);
                if let Some(lists) = next {
                    self.attach(i, lists);
                }
                if top {
                    self.previous_block = self.block;
                    self.block = Some(i);
                }
            }
            Event::End(TagEnd::Paragraph) if self.is_empty_paragraph() => {
                self.next = None;
                self.open.pop();
                let paragraph = self.pending.pop().expect("the paragraph").item;
                if self.open.is_empty() {
                    self.block = self.previous_block;
                }
                self.alone(paragraph);
            }
            Event::End(_) => {
                self.next = None;
                self.push(event, range);
            }
            Event::Text(_) if !self.in_code_block() => {
                match Lists::parse(&self.text[range.start..]) {
                    Some((lists, len)) => self.lists(lists, len, event, range),
                    None => {
                        self.push(event, range);
                    }
                }
            }
            _ => {
                self.push(event, range);
            }
        }
    }

    /// Gives `lists`, `len` bytes at the start of `event`'s text, to what
    /// they're for, or leaves them as text if they're for nothing.
    fn lists(&mut self, lists: Lists<'a>, len: usize, event: Event<'a>, range: Range<usize>) {
        let start = range.start;
        let end = start + len;
        let Some(target) = self.target(start, end) else {
            self.push(event, range);
            return;
        };
        let rest = self.consume(range, end);
        match target {
            Target::Span(i) => {
                self.attach(i, lists);
                if let Some(rest) = rest {
                    self.push(
                        Event::Text(CowStr::Borrowed(&self.text[rest.clone()])),
                        rest,
                    );
                }
            }
            Target::Item(i) => {
                self.attach(i, lists);
                if let Some(rest) = rest {
                    let text = self.text[rest.clone()].trim_start();
                    if !text.is_empty() {
                        let rest = rest.end - text.len()..rest.end;
                        self.push(Event::Text(CowStr::Borrowed(text)), rest);
                    }
                }
            }
            Target::Line { lazy } => self.line(lists, lazy, start),
        }
    }

    /// What the lists between `start` and `end` are for, if anything.
    fn target(&self, start: usize, end: usize) -> Option<Target> {
        let last = self.last_index()?;
        self.span_before(last, start)
            .or_else(|| self.alone_on_line(start, end))
            .or_else(|| self.item_starting(last))
    }

    /// The span the lists at `start` come right after, if `last` ends one
    /// there.
    fn span_before(&self, last: usize, start: usize) -> Option<Target> {
        let item = self.item(last);
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
            ) => Some(Target::Span(self.start_of(last))),
            _ => None,
        }
    }

    /// A line of its own, if nothing but indentation or quote markers comes
    /// before the lists between `start` and `end` on their line, and nothing
    /// after.
    fn alone_on_line(&self, start: usize, end: usize) -> Option<Target> {
        let line_start = self.text[..start].rfind('\n').map_or(0, |i| i + 1);
        let lead = &self.text[line_start..start];
        let line_end = self.text[end..]
            .find('\n')
            .map_or(self.text.len(), |i| end + i);
        let quoted = lead.chars().all(|c| c.is_whitespace() || c == '>');
        (quoted && self.text[end..line_end].trim().is_empty()).then_some(Target::Line {
            lazy: lead.is_empty(),
        })
    }

    /// The list item `last` starts, or starts the paragraph of.
    fn item_starting(&self, last: usize) -> Option<Target> {
        let item = match self.item(last).event {
            Event::Start(Tag::Item) => last,
            Event::Start(Tag::Paragraph) => last.checked_sub(1)?,
            _ => return None,
        };
        self.get(item)
            .is_some_and(|item| matches!(item.event, Event::Start(Tag::Item)))
            .then_some(Target::Item(item))
    }

    /// Gives `lists` alone on their line, starting at `start`, to what
    /// they're for: the paragraph they start, the paragraph they end,
    /// splitting off the lines after them, or for a lazy line, the
    /// outermost list or quote they're in.
    fn line(&mut self, lists: Lists<'a>, lazy: bool, start: usize) {
        if let Some(split) = self.split {
            // Lists on the lines after a paragraph's lists are for it too.
            self.attach(split.paragraph, lists);
            self.skip_soft_break();
            return;
        }
        let Some(last) = self.last_index() else {
            return;
        };
        match self.item(last).event {
            Event::Start(Tag::Paragraph) => {
                self.attach(last, lists);
                self.skip_soft_break();
            }
            Event::SoftBreak => {
                self.pending.pop();
                let container = self.open.iter().copied().find(|&i| {
                    matches!(
                        self.item(i).event,
                        Event::Start(Tag::List(_) | Tag::BlockQuote(_))
                    )
                });
                if let Some(container) = container.filter(|_| lazy) {
                    self.attach(container, lists);
                    return;
                }
                let Some(&block) = self.open.last() else {
                    return;
                };
                self.attach(block, lists);
                let paragraph = matches!(self.item(block).event, Event::Start(Tag::Paragraph));
                if paragraph && self.skip_soft_break() {
                    self.split = Some(Split {
                        paragraph: block,
                        end: start,
                    });
                }
            }
            _ => {
                if let Some(&block) = self.open.last() {
                    self.attach(block, lists);
                }
            }
        }
    }

    /// Gives what's in a paragraph of only lists to the block before it, if
    /// no blank line comes between them, or else the block after it.
    fn alone(&mut self, paragraph: Item<'a>) {
        let Item { lists, range, .. } = paragraph;
        if let Some(last) = self.last_index() {
            let block = match self.item(last).event {
                Event::End(_) => Some(self.start_of(last)),
                Event::Rule => Some(last),
                _ => None,
            };
            if let Some(block) = block
                && self.next_to(&self.item(last).range, range.start)
            {
                self.attach(block, lists);
                return;
            }
        }
        self.next = Some(lists);
    }

    /// Skips the events read from the lists ending at `end`, which start at
    /// `range`, returning the range of any text left after them.
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

    /// Skips a soft break next, returning whether there was one.
    fn skip_soft_break(&mut self) -> bool {
        let soft_break = matches!(self.events.peek(), Some((Event::SoftBreak, _)));
        if soft_break {
            self.events.next();
        }
        soft_break
    }

    fn attach(&mut self, i: usize, lists: Lists<'a>) {
        self.item_mut(i).lists.extend(lists);
    }

    /// Pushes `event`, starting a new paragraph first if lists ended the
    /// one it's in.
    fn push(&mut self, event: Event<'a>, range: Range<usize>) -> usize {
        if let Some(split) = self.split.take()
            && !matches!(event, Event::End(TagEnd::Paragraph))
        {
            let first = self.item(split.paragraph).range.start..split.end;
            self.push_item(Event::End(TagEnd::Paragraph), first);
            self.push_item(Event::Start(Tag::Paragraph), range.start..range.start);
        }
        self.push_item(event, range)
    }

    /// Pushes `event`, opening the element it starts or closing the one it
    /// ends.
    fn push_item(&mut self, event: Event<'a>, range: Range<usize>) -> usize {
        let i = self.base + self.pending.len();
        let start = match event {
            Event::Start(_) => {
                self.open.push(i);
                None
            }
            Event::End(_) => Some(
                self.open
                    .pop()
                    .expect("pulldown-cmark to balance its events"),
            ),
            _ => None,
        };
        let item = Item::new(event, range);
        self.pending.push(Pending { item, start });
        i
    }

    /// Gives out the items before `end`, with their ids and languages.
    fn settle(&mut self, end: usize) {
        if end <= self.base {
            return;
        }
        let mut items: Vec<Item<'a>> = self
            .pending
            .drain(..end - self.base)
            .map(|pending| pending.item)
            .collect();
        self.ids.assign(&mut items, self.text);
        for item in &mut items {
            item.take_language_from_classes();
        }
        // Taking `{#id}` off a heading can leave its text empty.
        self.ready.extend(
            items
                .into_iter()
                .filter(|item| !matches!(&item.event, Event::Text(t) if t.is_empty())),
        );
        self.base = end;
    }

    /// The index of the last item read and not given out.
    fn last_index(&self) -> Option<usize> {
        self.pending.len().checked_sub(1).map(|i| self.base + i)
    }

    fn get(&self, i: usize) -> Option<&Item<'a>> {
        let pending = self.pending.get(i.checked_sub(self.base)?)?;
        Some(&pending.item)
    }

    fn item(&self, i: usize) -> &Item<'a> {
        &self.pending[i - self.base].item
    }

    fn item_mut(&mut self, i: usize) -> &mut Item<'a> {
        &mut self.pending[i - self.base].item
    }

    /// The index of the start of the end at `i`.
    fn start_of(&self, i: usize) -> usize {
        self.pending[i - self.base].start.expect("an end")
    }

    /// Whether the innermost open element is a paragraph with nothing in it.
    fn is_empty_paragraph(&self) -> bool {
        self.open.last().is_some_and(|&i| {
            self.last_index() == Some(i)
                && matches!(self.item(i).event, Event::Start(Tag::Paragraph))
        })
    }

    fn in_code_block(&self) -> bool {
        self.open
            .last()
            .is_some_and(|&i| matches!(self.item(i).event, Event::Start(Tag::CodeBlock(_))))
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
                None if !self.pending.is_empty() => self.settle(self.base + self.pending.len()),
                None => return None,
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.ready.len(), None)
    }
}

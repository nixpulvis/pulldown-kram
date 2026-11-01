//! Reads pulldown-cmark's events, taking kramdown's attribute lists out of
//! the text and giving them to the elements they're for.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, CowStr, Event, OffsetIter, Options, Parser, Tag, TagEnd};

use crate::attributes::{Attributes, Lists};
use crate::headings::{self, HeaderIds};

/// An event, where in the source it was read, and what kramdown adds to it.
#[derive(Debug, Clone)]
pub(crate) struct Item<'a> {
    pub event: Event<'a>,
    pub range: Range<usize>,
    pub attributes: Attributes<'a>,
    /// Whether `{:toc}` asks for this list to become a table of contents.
    #[cfg_attr(not(feature = "toc"), allow(dead_code))]
    pub toc: bool,
}

impl<'a> Item<'a> {
    pub fn new(event: Event<'a>, range: Range<usize>) -> Self {
        Self {
            event,
            range,
            attributes: Attributes::default(),
            toc: false,
        }
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
    out: Vec<Item<'a>>,
    base: usize,
    /// For each item in `out` that's an end, the index of its start.
    starts: Vec<usize>,
    /// The indices of the elements open, innermost last.
    open: Vec<usize>,
    /// The indices of the last top-level block read, and the one before it,
    /// which an attribute list may still be for.
    block: Option<usize>,
    previous_block: Option<usize>,
    /// What a list alone between blocks gives the next block.
    next: Option<(Attributes<'a>, bool)>,
    /// A paragraph that lists on lines of their own have ended, and where,
    /// for the lines after them to start a new one.
    split: Option<(usize, usize)>,
    ids: HeaderIds,
    /// The items ready to give out.
    ready: VecDeque<Item<'a>>,
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
            out: Vec::new(),
            base: 0,
            starts: Vec::new(),
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
                let start = matches!(event, Event::Start(_));
                let i = self.push(event, range);
                if let Some((attributes, toc)) = next {
                    self.attach(i, attributes, toc);
                }
                if start {
                    self.open.push(i);
                }
                if top {
                    self.previous_block = self.block;
                    self.block = Some(i);
                }
            }
            Event::End(TagEnd::Paragraph) if self.is_empty_paragraph() => {
                self.next = None;
                self.open.pop();
                let paragraph = self.pop();
                if self.open.is_empty() {
                    self.block = self.previous_block;
                }
                self.alone(paragraph);
            }
            Event::End(_) => {
                self.next = None;
                let start = self.open.pop().unwrap_or_default();
                let i = self.push(event, range);
                *self.start_of_mut(i) = start;
            }
            Event::Text(_) if !self.in_code_block() => {
                match Lists::parse(&self.text[range.start..]) {
                    Some(lists) => self.lists(lists, event, range),
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

    /// Gives `lists`, which start `event`'s text, to what they're for, or
    /// leaves them as text if they're for nothing.
    fn lists(&mut self, lists: Lists<'a>, event: Event<'a>, range: Range<usize>) {
        let start = range.start;
        let end = start + lists.len;
        let Some(target) = self.target(start, end) else {
            self.push(event, range);
            return;
        };
        let rest = self.consume(range, end);
        let Lists {
            attributes, toc, ..
        } = lists;
        match target {
            Target::Span(i) => {
                self.attach(i, attributes, toc);
                if let Some(rest) = rest {
                    self.push(
                        Event::Text(CowStr::Borrowed(&self.text[rest.clone()])),
                        rest,
                    );
                }
            }
            Target::Item(i) => {
                self.attach(i, attributes, toc);
                if let Some(rest) = rest {
                    let text = self.text[rest.clone()].trim_start();
                    if !text.is_empty() {
                        let rest = rest.end - text.len()..rest.end;
                        self.push(Event::Text(CowStr::Borrowed(text)), rest);
                    }
                }
            }
            Target::Line { lazy } => self.line(attributes, toc, lazy, start..end),
        }
    }

    /// What the lists between `start` and `end` are for, if anything.
    fn target(&self, start: usize, end: usize) -> Option<Target> {
        let last = self.last_index()?;
        let prev = self.item(last);
        if prev.range.end == start {
            match prev.event {
                Event::Code(_) => return Some(Target::Span(last)),
                Event::End(
                    TagEnd::Emphasis
                    | TagEnd::Strong
                    | TagEnd::Strikethrough
                    | TagEnd::Superscript
                    | TagEnd::Subscript
                    | TagEnd::Link
                    | TagEnd::Image,
                ) => return Some(Target::Span(self.start_of(last))),
                _ => {}
            }
        }
        let line_start = self.text[..start].rfind('\n').map_or(0, |i| i + 1);
        let lead = &self.text[line_start..start];
        let line_end = self.text[end..]
            .find('\n')
            .map_or(self.text.len(), |i| end + i);
        let quoted = lead.chars().all(|c| c.is_whitespace() || c == '>');
        if quoted && self.text[end..line_end].trim().is_empty() {
            return Some(Target::Line {
                lazy: lead.is_empty(),
            });
        }
        let item = match prev.event {
            Event::Start(Tag::Item) => last,
            Event::Start(Tag::Paragraph) => last.checked_sub(1)?,
            _ => return None,
        };
        self.get(item)
            .is_some_and(|item| matches!(item.event, Event::Start(Tag::Item)))
            .then_some(Target::Item(item))
    }

    /// Gives what's in a list alone on its line to what it's for: the
    /// paragraph it starts, the paragraph it ends, splitting off the lines
    /// after it, or for a lazy line, the outermost list or quote it's in.
    fn line(&mut self, attributes: Attributes<'a>, toc: bool, lazy: bool, list: Range<usize>) {
        if let Some((paragraph, _)) = self.split {
            // Lists on the lines after a paragraph's lists are for it too.
            self.attach(paragraph, attributes, toc);
            self.skip_soft_break();
            return;
        }
        let Some(last) = self.last_index() else {
            return;
        };
        match self.item(last).event {
            Event::Start(Tag::Paragraph) => {
                self.attach(last, attributes, toc);
                self.skip_soft_break();
            }
            Event::SoftBreak => {
                self.pop();
                let container = self.open.iter().copied().find(|&i| {
                    matches!(
                        self.item(i).event,
                        Event::Start(Tag::List(_) | Tag::BlockQuote(_))
                    )
                });
                if let Some(container) = container.filter(|_| lazy) {
                    self.attach(container, attributes, toc);
                    return;
                }
                let Some(&block) = self.open.last() else {
                    return;
                };
                self.attach(block, attributes, toc);
                let paragraph = matches!(self.item(block).event, Event::Start(Tag::Paragraph));
                if paragraph && self.skip_soft_break() {
                    self.split = Some((block, list.start));
                }
            }
            _ => {
                if let Some(&block) = self.open.last() {
                    self.attach(block, attributes, toc);
                }
            }
        }
    }

    /// Gives what's in a paragraph of only lists to the block before it, if
    /// no blank line comes between them, or else the block after it.
    fn alone(&mut self, paragraph: Item<'a>) {
        let Item {
            attributes,
            toc,
            range,
            ..
        } = paragraph;
        if let Some(last) = self.last_index() {
            let block = match self.item(last).event {
                Event::End(_) => Some(self.start_of(last)),
                Event::Rule => Some(last),
                _ => None,
            };
            if let Some(block) = block
                && self.next_to(&self.item(last).range, range.start)
            {
                self.attach(block, attributes, toc);
                return;
            }
        }
        self.next = Some((attributes, toc));
    }

    /// Skips the events read from the lists ending at `end`, which start at
    /// `range`, returning the range of any text left after them.
    fn consume(&mut self, range: Range<usize>, end: usize) -> Option<Range<usize>> {
        let mut range = range;
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

    fn attach(&mut self, i: usize, attributes: Attributes<'a>, toc: bool) {
        let item = self.item_mut(i);
        item.attributes.extend(attributes);
        item.toc |= toc;
    }

    /// Pushes `event`, starting a new paragraph first if lists ended the
    /// one it's in.
    fn push(&mut self, event: Event<'a>, range: Range<usize>) -> usize {
        if let Some((paragraph, end)) = self.split.take()
            && !matches!(event, Event::End(TagEnd::Paragraph))
        {
            self.open.pop();
            let first = self.item(paragraph).range.start..end;
            let close = self.push_item(Event::End(TagEnd::Paragraph), first);
            *self.start_of_mut(close) = paragraph;
            let open = self.push_item(Event::Start(Tag::Paragraph), range.start..range.start);
            self.open.push(open);
        }
        self.push_item(event, range)
    }

    fn push_item(&mut self, event: Event<'a>, range: Range<usize>) -> usize {
        self.out.push(Item::new(event, range));
        self.starts.push(0);
        self.base + self.out.len() - 1
    }

    fn pop(&mut self) -> Item<'a> {
        self.starts.pop();
        self.out.pop().expect("an item to pop")
    }

    /// Gives out the items before `end`, with their ids and languages.
    fn settle(&mut self, end: usize) {
        let Some(len) = end.checked_sub(self.base).filter(|&len| len > 0) else {
            return;
        };
        let items = &mut self.out[..len];
        headings::ids(items, self.text, &mut self.ids);
        languages(items);
        self.ready.extend(
            self.out
                .drain(..len)
                .filter(|item| !matches!(&item.event, Event::Text(t) if t.is_empty())),
        );
        self.starts.drain(..len);
        self.base = end;
    }

    /// The index of the last item read and not given out.
    fn last_index(&self) -> Option<usize> {
        self.out.len().checked_sub(1).map(|i| self.base + i)
    }

    fn get(&self, i: usize) -> Option<&Item<'a>> {
        self.out.get(i.checked_sub(self.base)?)
    }

    fn item(&self, i: usize) -> &Item<'a> {
        &self.out[i - self.base]
    }

    fn item_mut(&mut self, i: usize) -> &mut Item<'a> {
        &mut self.out[i - self.base]
    }

    fn start_of(&self, i: usize) -> usize {
        self.starts[i - self.base]
    }

    fn start_of_mut(&mut self, i: usize) -> &mut usize {
        &mut self.starts[i - self.base]
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
                None if !self.out.is_empty() => self.settle(self.base + self.out.len()),
                None => return None,
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.ready.len(), None)
    }
}

/// Gives a code block without a language the one in a `language-` class.
fn languages(items: &mut [Item<'_>]) {
    for item in items {
        let Event::Start(Tag::CodeBlock(kind)) = &mut item.event else {
            continue;
        };
        if matches!(kind, CodeBlockKind::Fenced(info) if !info.trim().is_empty()) {
            continue;
        }
        let language = item.attributes.classes.iter().find_map(|class| {
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
}

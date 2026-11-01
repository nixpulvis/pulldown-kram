//! Reads pulldown-cmark's events, taking kramdown's attribute lists out of
//! the text and giving them to the elements they're for.

use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{CowStr, Event, OffsetIter, Options, Parser, Tag, TagEnd};

use crate::attributes::{Attributes, Lists};

/// An event, where in the source it was read, and what kramdown adds to it.
#[derive(Debug, Clone)]
pub(crate) struct Item<'a> {
    pub event: Event<'a>,
    pub range: Range<usize>,
    pub attributes: Attributes<'a>,
    /// Whether `{:toc}` asks for this list to become a table of contents.
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

type Events<'a> = Peekable<OffsetIter<'a>>;

/// Reads `text` with `options`, less heading attributes, which read `{…}`
/// after a heading differently from kramdown.
pub(crate) fn read(text: &str, options: Options) -> Vec<Item<'_>> {
    let options = options - Options::ENABLE_HEADING_ATTRIBUTES;
    let mut reader = Reader {
        text,
        lines: text.match_indices('\n').map(|(i, _)| i + 1).collect(),
        out: Vec::new(),
        starts: Vec::new(),
        open: Vec::new(),
        next: None,
        split: None,
    };
    let mut events = Parser::new_ext(text, options).into_offset_iter().peekable();
    while let Some((event, range)) = events.next() {
        reader.event(event, range, &mut events);
    }
    reader.out
}

struct Reader<'a> {
    text: &'a str,
    /// Where each line after the first starts.
    lines: Vec<usize>,
    out: Vec<Item<'a>>,
    /// For each item that's an end, the index of its start.
    starts: Vec<usize>,
    /// The indices of the elements open, innermost last.
    open: Vec<usize>,
    /// What a list alone between blocks gives the next block.
    next: Option<(Attributes<'a>, bool)>,
    /// A paragraph that lists on lines of their own have ended, and where,
    /// for the lines after them to start a new one.
    split: Option<(usize, usize)>,
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
    fn event(&mut self, event: Event<'a>, range: Range<usize>, events: &mut Events<'a>) {
        match event {
            Event::Start(_) | Event::Rule => {
                let next = self.next.take();
                let start = matches!(event, Event::Start(_));
                let i = self.push(event, range);
                if let Some((attributes, toc)) = next {
                    self.attach(i, attributes, toc);
                }
                if start {
                    self.open.push(i);
                }
            }
            Event::End(TagEnd::Paragraph) if self.is_empty_paragraph() => {
                self.next = None;
                self.open.pop();
                let paragraph = self.pop();
                self.alone(paragraph);
            }
            Event::End(_) => {
                self.next = None;
                let start = self.open.pop().unwrap_or_default();
                let i = self.push(event, range);
                self.starts[i] = start;
            }
            Event::Text(_) if !self.in_code_block() => {
                match Lists::parse(&self.text[range.start..]) {
                    Some(lists) => self.lists(lists, event, range, events),
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
    fn lists(
        &mut self,
        lists: Lists<'a>,
        event: Event<'a>,
        range: Range<usize>,
        events: &mut Events<'a>,
    ) {
        let start = range.start;
        let end = start + lists.len;
        let Some(target) = self.target(start, end) else {
            self.push(event, range);
            return;
        };
        let rest = self.consume(range, end, events);
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
            Target::Line { lazy } => self.line(attributes, toc, lazy, start..end, events),
        }
    }

    /// What the lists between `start` and `end` are for, if anything.
    fn target(&self, start: usize, end: usize) -> Option<Target> {
        let last = self.out.len().checked_sub(1)?;
        let prev = &self.out[last];
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
                ) => return Some(Target::Span(self.starts[last])),
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
        matches!(self.out[item].event, Event::Start(Tag::Item)).then_some(Target::Item(item))
    }

    /// Gives what's in a list alone on its line to what it's for: the
    /// paragraph it starts, the paragraph it ends, splitting off the lines
    /// after it, or for a lazy line, the outermost list or quote it's in.
    fn line(
        &mut self,
        attributes: Attributes<'a>,
        toc: bool,
        lazy: bool,
        list: Range<usize>,
        events: &mut Events<'a>,
    ) {
        if let Some((paragraph, _)) = self.split {
            // Lists on the lines after a paragraph's lists are for it too.
            self.attach(paragraph, attributes, toc);
            if matches!(events.peek(), Some((Event::SoftBreak, _))) {
                events.next();
            }
            return;
        }
        let Some(last) = self.out.len().checked_sub(1) else {
            return;
        };
        match self.out[last].event {
            Event::Start(Tag::Paragraph) => {
                self.attach(last, attributes, toc);
                if matches!(events.peek(), Some((Event::SoftBreak, _))) {
                    events.next();
                }
            }
            Event::SoftBreak => {
                self.pop();
                let container = self.open.iter().copied().find(|&i| {
                    matches!(
                        self.out[i].event,
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
                let paragraph = matches!(self.out[block].event, Event::Start(Tag::Paragraph));
                if paragraph && matches!(events.peek(), Some((Event::SoftBreak, _))) {
                    events.next();
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
        if let Some(last) = self.out.len().checked_sub(1) {
            let block = match self.out[last].event {
                Event::End(_) => Some(self.starts[last]),
                Event::Rule => Some(last),
                _ => None,
            };
            if let Some(block) = block
                && self.next_to(&self.out[last].range, range.start)
            {
                self.attach(block, attributes, toc);
                return;
            }
        }
        self.next = Some((attributes, toc));
    }

    /// Skips the events read from the lists ending at `end`, which start at
    /// `range`, returning the range of any text left after them.
    fn consume(
        &mut self,
        range: Range<usize>,
        end: usize,
        events: &mut Events<'a>,
    ) -> Option<Range<usize>> {
        let mut range = range;
        loop {
            if range.end > end {
                return Some(end..range.end);
            }
            match events.peek() {
                Some((
                    Event::Text(_) | Event::Code(_) | Event::InlineHtml(_) | Event::InlineMath(_),
                    next,
                )) if next.start < end => range = events.next()?.1,
                _ => return None,
            }
        }
    }

    fn attach(&mut self, i: usize, attributes: Attributes<'a>, toc: bool) {
        let item = &mut self.out[i];
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
            let first = self.out[paragraph].range.start..end;
            let close = self.push_item(Event::End(TagEnd::Paragraph), first);
            self.starts[close] = paragraph;
            let open = self.push_item(Event::Start(Tag::Paragraph), range.start..range.start);
            self.open.push(open);
        }
        self.push_item(event, range)
    }

    fn push_item(&mut self, event: Event<'a>, range: Range<usize>) -> usize {
        self.out.push(Item::new(event, range));
        self.starts.push(0);
        self.out.len() - 1
    }

    fn pop(&mut self) -> Item<'a> {
        self.starts.pop();
        self.out.pop().expect("an item to pop")
    }

    /// Whether the innermost open element is a paragraph with nothing in it.
    fn is_empty_paragraph(&self) -> bool {
        self.open.last().is_some_and(|&i| {
            i + 1 == self.out.len() && matches!(self.out[i].event, Event::Start(Tag::Paragraph))
        })
    }

    fn in_code_block(&self) -> bool {
        self.open
            .last()
            .is_some_and(|&i| matches!(self.out[i].event, Event::Start(Tag::CodeBlock(_))))
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

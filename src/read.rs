//! Reads pulldown-cmark's events, taking kramdown's attribute lists out of
//! the text and giving them to the elements they're for.

use std::collections::VecDeque;
use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{
    CodeBlockKind, CowStr, Event, LinkType, OffsetIter, Parser, RefDefs, Tag, TagEnd,
};

use crate::attributes::Lists;
use crate::definitions::DefinitionLists;
use crate::headings::HeaderIds;
use crate::lists::ListItems;
use crate::source::{Mark, MarkdownElements, StandIn};
use crate::{Extensions, Options};

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
                CowStr::Borrowed(class) => CowStr::Borrowed(&class["language-".len()..]),
                _ => CowStr::from(language.to_owned()),
            })
        });
        if let Some(language) = language {
            *kind = CodeBlockKind::Fenced(language);
        }
    }

    /// Whether `event` is part of a paragraph's text, not a block.
    pub fn inline(event: &Event<'_>) -> bool {
        match event {
            Event::Start(tag) => matches!(
                tag,
                Tag::Emphasis
                    | Tag::Strong
                    | Tag::Strikethrough
                    | Tag::Superscript
                    | Tag::Subscript
                    | Tag::Link { .. }
                    | Tag::Image { .. }
            ),
            Event::End(tag) => matches!(
                tag,
                TagEnd::Emphasis
                    | TagEnd::Strong
                    | TagEnd::Strikethrough
                    | TagEnd::Superscript
                    | TagEnd::Subscript
                    | TagEnd::Link
                    | TagEnd::Image
            ),
            Event::Text(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::TaskListMarker(_) => true,
            Event::Html(_) | Event::Rule => false,
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

    /// Gives the lists in `items` the classes kramdown's GFM parser gives
    /// task lists: `task-list` to a list with a task in it, and
    /// `task-list-item` to each of its items from its first task on, before
    /// any classes from attribute lists.
    fn mark_task_lists(items: &mut [Self]) {
        for list in 0..items.len() {
            if !matches!(items[list].event, Event::Start(Tag::List(_))) {
                continue;
            }
            let mut tasks = false;
            let mut depth = 0usize;
            for i in list + 1..Self::end_of(items, list) {
                if depth == 0 && matches!(items[i].event, Event::Start(Tag::Item)) {
                    tasks |= Self::starts_with_task(&items[i + 1..]);
                    if tasks {
                        items[i]
                            .lists
                            .attributes
                            .classes
                            .insert(0, "task-list-item".into());
                    }
                }
                match items[i].event {
                    Event::Start(_) => depth += 1,
                    Event::End(_) => depth -= 1,
                    _ => {}
                }
            }
            if tasks {
                items[list]
                    .lists
                    .attributes
                    .classes
                    .insert(0, "task-list".into());
            }
        }
    }

    /// Whether the content of an item, `rest`, starts with a task's box, as
    /// it does in its first paragraph if its list is loose.
    fn starts_with_task(rest: &[Self]) -> bool {
        let mut events = rest.iter().map(|item| &item.event);
        match events.next() {
            Some(Event::TaskListMarker(_)) => true,
            Some(Event::Start(Tag::Paragraph)) => {
                matches!(events.next(), Some(Event::TaskListMarker(_)))
            }
            _ => false,
        }
    }
}

/// Reads the events of a document as pulldown-cmark gives them, a block at
/// a time: a block is given out once the next one starts, as only then is
/// it known that no attribute list after it is for it.
pub(crate) struct Reader<'a> {
    text: &'a str,
    kramdown: Extensions,
    events: Peekable<OffsetIter<'a>>,
    /// Where each line after the first starts.
    lines: Vec<usize>,
    pending: Pending<'a>,
    /// The indices of the last top-level block read, and the one before it,
    /// which an attribute list may still be for.
    block: Option<Index>,
    previous_block: Option<Index>,
    /// What lists alone between blocks give the next block.
    next: Option<Lists<'a>>,
    split: Option<Split>,
    /// For each strikethrough open, whether it's kramdown's, `~~a~~`,
    /// rather than `~a~`, which kramdown leaves as text.
    strikethroughs: Vec<bool>,
    ids: HeaderIds,
    definitions: Definitions<'a>,
    /// The stand-ins in the text, and the index of the next not yet read.
    marks: &'a [Mark<'a>],
    next_mark: usize,
    /// The items ready to give out.
    ready: VecDeque<Item<'a>>,
}

/// The items read but not yet given out, and the elements open among them.
/// Indices count every item read, starting from the first, so an index
/// holds as the items before it are given out.
#[derive(Default)]
struct Pending<'a> {
    slots: Vec<Slot<'a>>,
    /// The index of the first slot.
    base: Index,
    /// The indices of the elements open, innermost last.
    open: Vec<Index>,
}

/// Where an item is in `Pending`, counting every item read from the first,
/// so it holds as the items before it are given out. A position in a slice
/// of items is a plain `usize`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Index(usize);

impl Index {
    /// The index `by` items after this one.
    fn advance(self, by: usize) -> Index {
        Index(self.0 + by)
    }

    /// The index of the item right before this one, unless it's the first.
    fn previous(self) -> Option<Index> {
        self.0.checked_sub(1).map(Index)
    }
}

/// An item read, but not given out until no more lists can be for it.
struct Slot<'a> {
    item: Item<'a>,
    /// For an end, the index of its start.
    start: Option<Index>,
}

/// The attribute lists kramdown gives link definitions, for the links that
/// use them: on the lines right after one, or before it, right before or
/// alone between blocks.
#[derive(Default)]
struct Definitions<'a> {
    /// The lists for each definition with any, by its label in lowercase.
    lists: Vec<(String, Lists<'a>)>,
    /// Where each line of lists for a definition starts, in order.
    lines: Vec<usize>,
}

/// A paragraph that lists on lines of their own have ended, for the lines
/// after them to start a new one.
#[derive(Clone, Copy)]
struct Split {
    /// The index of the paragraph's start.
    paragraph: Index,
    /// Where in the source the paragraph ends, at the lists.
    end: usize,
}

/// What an attribute list is for, by where it is.
enum Target {
    /// Right after a span, like `*a*{:.x}`: the span at this index.
    Span(Index),
    /// At the start of a list item, like `* {:.x} a`: the item at this index.
    Item(Index),
    /// Alone on its line, `lazy` if it isn't indented or quoted like the
    /// lines before it.
    Line { lazy: bool },
}

impl Target {
    /// The extension that reads lists here.
    fn extension(&self) -> Extensions {
        match self {
            Target::Span(_) => Extensions::ENABLE_SPAN_ATTRIBUTES,
            Target::Item(_) => Extensions::ENABLE_ITEM_ATTRIBUTES,
            Target::Line { .. } => Extensions::ENABLE_BLOCK_ATTRIBUTES,
        }
    }
}

impl<'a> Reader<'a> {
    /// Reads `text` with `options`, giving out what each of `marks` stands
    /// for in place of the stand-in.
    pub fn new(text: &'a str, options: Options, marks: &'a [Mark<'a>]) -> Self {
        let markdown = options.parser_options();
        let events = Parser::new_ext(text, markdown).into_offset_iter();
        let definitions = if options
            .kramdown
            .contains(Extensions::ENABLE_BLOCK_ATTRIBUTES)
            && text.contains("{:")
        {
            Definitions::read(text, events.reference_definitions())
        } else {
            Definitions::default()
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
            definitions,
            marks,
            next_mark: 0,
            ready: VecDeque::new(),
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
                let definition = self.definition_lists(&event);
                let i = self.push(event, range);
                if let Some(lists) = definition {
                    self.pending.attach(i, lists);
                }
                if let Some(lists) = next {
                    self.pending.attach(i, lists);
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
                match Lists::parse(&self.text[range.start..]) {
                    Some((lists, len)) => self.lists(lists, len, event, range),
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

    /// Gives out the text of the raw HTML span `name`, from `start` to its
    /// closing tag, as it is, in place of what pulldown-cmark read of it,
    /// as far as what it read ends before the closing tag.
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
        // As far as what pulldown-cmark read is text, ending before the
        // closing tag.
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

    /// Takes the backslash off the end of the text just read, if two in the
    /// source end it, kramdown's hard line break, returning whether it did.
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

    /// What the stand-in at the start of `range` stands for, if a block's
    /// starts it, or a span's, `block` says, marking it read.
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
            mark.at < range.end
                && self.text[range.start..mark.at]
                    .trim_matches(|c: char| c.is_whitespace() || c == '>')
                    .is_empty()
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

    /// The lists for the link definition `event` uses, if it's the start of
    /// a link or image by reference to one with any.
    fn definition_lists(&self, event: &Event<'a>) -> Option<Lists<'a>> {
        if self.definitions.lists.is_empty() {
            return None;
        }
        let Event::Start(Tag::Link { link_type, id, .. } | Tag::Image { link_type, id, .. }) =
            event
        else {
            return None;
        };
        if !matches!(
            link_type,
            LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
        ) {
            return None;
        }
        let label = id.to_lowercase();
        self.definitions
            .lists
            .iter()
            .find(|(other, _)| *other == label)
            .map(|(_, lists)| lists.clone())
    }

    /// Whether the element starting at `start` goes on with the one just
    /// ended with `end`, with only blank lines between, as kramdown reads
    /// it. A definition list goes on past any blank lines, while
    /// pulldown-cmark's ends after a blank line following an empty
    /// definition. And a list goes on whichever of `*`, `+` and `-` marks
    /// its items, while pulldown-cmark's ends at each change.
    fn continues(&self, end: TagEnd, start: usize) -> bool {
        self.pending.last().is_some_and(|last| {
            last.event == Event::End(end)
                && self.text[last.range.end..start]
                    .trim_matches(|c: char| c.is_whitespace() || c == '>')
                    .is_empty()
        })
    }

    /// `event` as kramdown reads its source, where pulldown-cmark reads more
    /// into it: a single `~` and a single `$` are text, `$$` math in a
    /// paragraph with more in it is inline, math is without the whitespace
    /// around it, and HTML is without the `markdown="1"` kramdown reads.
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
            _ => {}
        }
        (event, range)
    }

    /// `text` without the whitespace around it.
    fn trim(text: CowStr<'a>) -> CowStr<'a> {
        match text {
            CowStr::Borrowed(text) => CowStr::Borrowed(text.trim()),
            text if text.trim().len() == text.len() => text,
            text => CowStr::from(text.trim().to_owned()),
        }
    }

    /// Whether the event about to be read, ending at `end`, is all there is
    /// in its paragraph, as it is when attribute lists on the next line end
    /// the paragraph after it.
    fn alone_in_paragraph(&mut self, end: usize) -> bool {
        self.pending
            .last()
            .is_some_and(|last| matches!(last.event, Event::Start(Tag::Paragraph)))
            && (matches!(self.events.peek(), Some((Event::End(TagEnd::Paragraph), _)))
                || self.lists_end_line(end))
    }

    /// Whether nothing follows `end` on its line, and the next line is
    /// attribute lists alone, for the block it ends.
    fn lists_end_line(&self, end: usize) -> bool {
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
        rest.trim().is_empty()
            && Lists::parse(next).is_some_and(|(_, len)| len > 0 && len == next.len())
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
                self.pending.attach(i, lists);
                if let Some(rest) = rest {
                    self.push(
                        Event::Text(CowStr::Borrowed(&self.text[rest.clone()])),
                        rest,
                    );
                }
            }
            Target::Item(i) => {
                self.pending.attach(i, lists);
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
        let last = self.pending.last_index()?;
        let target = self
            .span_before(last, start)
            .or_else(|| self.alone_on_line(start, end))
            .or_else(|| self.item_starting(last))?;
        self.kramdown.contains(target.extension()).then_some(target)
    }

    /// The span the lists at `start` come right after, if `last` ends one
    /// there.
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

    /// The definition list `title` starts the first term of, if any.
    fn starts_definition_list(&self, title: Index) -> Option<Index> {
        let list = title.previous()?;
        self.pending
            .get(list)
            .is_some_and(|item| matches!(item.event, Event::Start(Tag::DefinitionList)))
            .then_some(list)
    }

    /// Gives `lists` alone on their line, starting at `start`, to what
    /// they're for: the paragraph they start, the paragraph they end,
    /// splitting off the lines after them, the definition list whose first
    /// term's line they come before, or for a lazy line, the outermost
    /// list, quote or definition list they're in. Those for a link
    /// definition are already its links'.
    fn line(&mut self, lists: Lists<'a>, lazy: bool, start: usize) {
        if self.definitions.lines.binary_search(&start).is_ok() {
            if matches!(self.pending.last(), Some(item) if matches!(item.event, Event::Start(Tag::Paragraph)))
            {
                self.skip_soft_break();
            }
            return;
        }
        if let Some(split) = self.split {
            // Lists on the lines after a paragraph's lists are for it too.
            self.pending.attach(split.paragraph, lists);
            self.skip_soft_break();
            return;
        }
        let Some(last) = self.pending.last_index() else {
            return;
        };
        match self.pending.item(last).event {
            Event::Start(Tag::TableCell) => self.table_row(last, lists),
            Event::Start(Tag::Paragraph) => {
                self.pending.attach(last, lists);
                self.skip_soft_break();
            }
            Event::Start(Tag::DefinitionListTitle)
                if let Some(list) = self.starts_definition_list(last)
                    && let Some((Event::SoftBreak, line)) = self.events.peek() =>
            {
                // The term starts on the next line.
                let term = line.end;
                self.events.next();
                self.pending.attach(list, lists);
                self.pending.item_mut(last).range.start = term;
            }
            Event::SoftBreak => {
                self.pending.pop();
                // Between terms, they're for the term before, not the list.
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
                        Event::Start(Tag::List(_) | Tag::BlockQuote(_)) => true,
                        Event::Start(Tag::DefinitionList) => !in_term,
                        _ => false,
                    });
                if let Some(container) = container.filter(|_| lazy) {
                    self.pending.attach(container, lists);
                    return;
                }
                let Some(block) = self.pending.innermost() else {
                    return;
                };
                self.pending.attach(block, lists);
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
                    self.pending.attach(block, lists);
                }
            }
        }
    }

    /// Gives `lists`, alone in the table cell `cell`, to what they're for.
    /// On the line right after a table, they're for the table, but
    /// pulldown-cmark reads the line as one more row, so the row is left
    /// out. Anywhere else, they're for the cell.
    fn table_row(&mut self, cell: Index, lists: Lists<'a>) {
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
                self.pending.attach(table, lists);
            }
            return;
        }
        self.pending.attach(cell, lists);
        for (event, range) in rest {
            self.event(event, range);
        }
    }

    /// Gives what's in a paragraph of only lists to the block before it, if
    /// no blank line comes between them, or else the block after it.
    fn alone(&mut self, paragraph: Item<'a>) {
        let Item { lists, range, .. } = paragraph;
        if let Some(last) = self.pending.last_index() {
            let block = match self.pending.item(last).event {
                Event::End(_) => Some(self.pending.start_of(last)),
                Event::Rule => Some(last),
                _ => None,
            };
            if let Some(block) = block
                && self.next_to(&self.pending.item(last).range, range.start)
            {
                self.pending.attach(block, lists);
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

    /// Pushes `event`, starting a new paragraph first if lists ended the
    /// one it's in.
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

    /// Gives out the items before `end`, with their ids, classes and
    /// languages.
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

impl<'a> Pending<'a> {
    /// Pushes `event`, opening the element it starts or closing the one it
    /// ends, and returns its index.
    fn push(&mut self, event: Event<'a>, range: Range<usize>) -> Index {
        let i = self.end();
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
        self.slots.push(Slot { item, start });
        i
    }

    /// Takes off the last item, which mustn't be an end, closing it if it's
    /// open.
    fn pop(&mut self) -> Option<Item<'a>> {
        let slot = self.slots.pop()?;
        debug_assert!(slot.start.is_none(), "an end popped");
        if self.open.last() == Some(&self.end()) {
            self.open.pop();
        }
        Some(slot.item)
    }

    /// Takes off the last item, an end, opening again the element it
    /// closed, and returns the index of the element's start.
    fn reopen(&mut self) -> Index {
        let slot = self.slots.pop().expect("an end");
        let start = slot.start.expect("an end");
        self.open.push(start);
        start
    }

    /// Drops the item at `i` and every one after it, closing those open.
    fn truncate(&mut self, i: Index) {
        self.slots.truncate(self.offset(i));
        self.open.retain(|&open| open < i);
    }

    /// Takes out the items before `end`, for giving out.
    fn take_before(&mut self, end: Index) -> Vec<Item<'a>> {
        if end <= self.base {
            return Vec::new();
        }
        let items = self
            .slots
            .drain(..self.offset(end))
            .map(|slot| slot.item)
            .collect();
        self.base = end;
        items
    }

    /// Where the item at `i` is in `slots`. `i` mustn't be given out.
    fn offset(&self, i: Index) -> usize {
        i.0.checked_sub(self.base.0).expect("an item not given out")
    }

    fn attach(&mut self, i: Index, lists: Lists<'a>) {
        self.item_mut(i).lists.extend(lists);
    }

    fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The index the next item pushed gets.
    fn end(&self) -> Index {
        self.base.advance(self.slots.len())
    }

    fn last_index(&self) -> Option<Index> {
        self.end().previous().filter(|_| !self.slots.is_empty())
    }

    fn last(&self) -> Option<&Item<'a>> {
        self.slots.last().map(|slot| &slot.item)
    }

    fn get(&self, i: Index) -> Option<&Item<'a>> {
        if i < self.base {
            return None;
        }
        let slot = self.slots.get(self.offset(i))?;
        Some(&slot.item)
    }

    fn item(&self, i: Index) -> &Item<'a> {
        &self.slots[self.offset(i)].item
    }

    fn item_mut(&mut self, i: Index) -> &mut Item<'a> {
        let offset = self.offset(i);
        &mut self.slots[offset].item
    }

    /// The index of the start of the end at `i`.
    fn start_of(&self, i: Index) -> Index {
        self.slots[self.offset(i)].start.expect("an end")
    }

    /// The indices of the elements open, outermost first.
    fn open(&self) -> impl Iterator<Item = Index> + '_ {
        self.open.iter().copied()
    }

    /// The index of the innermost element open.
    fn innermost(&self) -> Option<Index> {
        self.open.last().copied()
    }

    /// Whether the innermost open element is a paragraph with nothing in it.
    fn is_empty_paragraph(&self) -> bool {
        self.innermost().is_some_and(|i| {
            self.last_index() == Some(i)
                && matches!(self.item(i).event, Event::Start(Tag::Paragraph))
        })
    }

    fn in_code_block(&self) -> bool {
        self.innermost()
            .is_some_and(|i| matches!(self.item(i).event, Event::Start(Tag::CodeBlock(_))))
    }
}

impl<'a> Definitions<'a> {
    /// The lists for the link `definitions` read from `text`.
    fn read(text: &'a str, definitions: &RefDefs<'_>) -> Self {
        let mut spans: Vec<_> = definitions
            .iter()
            .map(|(label, definition)| (label, definition.span.clone()))
            .collect();
        spans.sort_by_key(|(_, span)| span.start);
        let mut found = Self::default();
        // Where the lines given to a definition so far end.
        let mut taken = 0;
        for (label, span) in spans {
            let lines = found.lines.len();
            let mut lists = Lists::default();
            for (start, before) in Self::before(text, span.start, taken) {
                found.lines.push(start);
                lists.extend(before);
            }
            let mut line = Self::line_end(text, span.end.saturating_sub(1));
            while let Some((start, after, end)) = Self::lists_on(text, line) {
                found.lines.push(start);
                lists.extend(after);
                line = end;
            }
            taken = taken.max(line);
            if found.lines.len() > lines {
                found.lists.push((label.to_lowercase(), lists));
            }
        }
        found
    }

    /// The lists before the definition at `start`, on the lines right
    /// before it, or alone between blocks before it, from the first, each
    /// with where it starts. Lines before `taken` are another's.
    fn before(text: &'a str, start: usize, taken: usize) -> Vec<(usize, Lists<'a>)> {
        let mut lists = Vec::new();
        let mut above = Self::line_before(text, Self::line_start(text, start));
        while let Some(line) = above
            && text[line..Self::line_end(text, line)].trim().is_empty()
        {
            above = Self::line_before(text, line);
        }
        while let Some(line) = above
            && line >= taken
            && let Some((start, found, _)) = Self::lists_on(text, line)
        {
            lists.push((start, found));
            above = Self::line_before(text, line);
        }
        // Right after another block, they're for it.
        if above.is_some_and(|line| !text[line..Self::line_end(text, line)].trim().is_empty()) {
            return Vec::new();
        }
        lists.reverse();
        lists
    }

    /// The lists alone on the line starting at `line`, if any, with where
    /// they start and where the next line starts.
    fn lists_on(text: &'a str, line: usize) -> Option<(usize, Lists<'a>, usize)> {
        let end = Self::line_end(text, line);
        let content = text.get(line..end)?;
        let trimmed = content.trim();
        let indent = content.len() - content.trim_start().len();
        let (lists, len) = Lists::parse(trimmed).filter(|_| indent < 4)?;
        (len == trimmed.len()).then_some((line + indent, lists, end))
    }

    fn line_start(text: &str, at: usize) -> usize {
        text[..at].rfind('\n').map_or(0, |i| i + 1)
    }

    /// Where the line after the one `at` is on starts, or the text ends.
    fn line_end(text: &str, at: usize) -> usize {
        text[at..].find('\n').map_or(text.len(), |i| at + i + 1)
    }

    fn line_before(text: &str, line: usize) -> Option<usize> {
        (line > 0).then(|| Self::line_start(text, line - 1))
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
            .filter(|item| !item.lists.attributes.classes.is_empty())
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
                (tag, item.lists.attributes.classes.join(" "))
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
    fn reader_span_lists_go_to_the_span_they_follow() {
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
    fn reader_text_after_span_lists_stays() {
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
    fn reader_item_lists_in_a_loose_list() {
        let items = read("* {:.x}   a\n\n* b\n");
        assert_eq!(classed(&items), [("li", "x".into())]);
        assert_eq!(text(&items), "ab");
    }

    #[test]
    fn pending_indices_hold_as_items_are_given_out() {
        let mut pending = Pending::default();
        let start = pending.push(Event::Start(Tag::Paragraph), 0..2);
        let text = pending.push(Event::Text("a".into()), 0..1);
        let end = pending.push(Event::End(TagEnd::Paragraph), 0..2);
        assert_eq!(pending.take_before(text).len(), 1);
        assert!(pending.get(start).is_none());
        assert_eq!(pending.item(text).event, Event::Text("a".into()));
        assert_eq!(pending.start_of(end), start);
        assert_eq!(pending.last_index(), Some(end));
        // What's already given out can't be given again.
        assert!(pending.take_before(start).is_empty());
        assert_eq!(pending.push(Event::Rule, 2..3), Index(3));
    }

    #[test]
    fn pending_closes_what_it_drops() {
        let mut pending = Pending::default();
        let table = pending.push(Event::Start(Tag::Table(Vec::new())), 0..1);
        let row = pending.push(Event::Start(Tag::TableRow), 0..1);
        pending.push(Event::Start(Tag::TableCell), 0..1);
        pending.truncate(row);
        assert_eq!(pending.innermost(), Some(table));
        assert_eq!(pending.last_index(), Some(table));

        pending.push(Event::Start(Tag::Paragraph), 0..1);
        assert!(pending.is_empty_paragraph());
        pending.pop();
        assert_eq!(pending.innermost(), Some(table));
        // Popping what isn't open leaves the open elements be.
        pending.push(Event::SoftBreak, 0..1);
        pending.pop();
        assert_eq!(pending.innermost(), Some(table));
    }

    #[test]
    fn item_end_of_finds_the_matching_end() {
        let items: Vec<_> = [
            Event::Start(Tag::List(None)),
            Event::Start(Tag::Item),
            Event::Text("a".into()),
            Event::End(TagEnd::Item),
            Event::Start(Tag::Item),
            Event::End(TagEnd::Item),
            Event::End(TagEnd::List(false)),
        ]
        .into_iter()
        .map(|event| Item::new(event, 0..0))
        .collect();
        assert_eq!(Item::end_of(&items, 0), 6);
        assert_eq!(Item::end_of(&items, 1), 3);
        assert_eq!(Item::end_of(&items, 4), 5);
        // Cut off before its end, the element runs to the last item.
        assert_eq!(Item::end_of(&items[..3], 0), 2);
    }

    #[test]
    fn item_take_language_from_classes_when_the_fence_has_none() {
        let language = |kind| {
            let mut item = Item::new(Event::Start(Tag::CodeBlock(kind)), 0..0);
            item.lists.attributes.classes =
                vec!["x".into(), "language-rb".into(), "language-py".into()];
            item.take_language_from_classes();
            item.event
        };
        let fenced =
            |info: &'static str| Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info.into())));
        assert_eq!(language(CodeBlockKind::Indented), fenced("rb"));
        assert_eq!(language(CodeBlockKind::Fenced(" ".into())), fenced("rb"));
        assert_eq!(language(CodeBlockKind::Fenced("c".into())), fenced("c"));
    }
}

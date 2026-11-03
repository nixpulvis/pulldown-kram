use std::ops::Range;

use pulldown_cmark::{Event, Tag};

use super::Item;
use crate::attributes::AttributeLists;

/// The items read but not emitted yet, and the elements open among them.
/// Indices count every item read, from the first, so an index stays valid
/// as the items before it are emitted.
#[derive(Default)]
pub(super) struct Pending<'a> {
    slots: Vec<Slot<'a>>,
    /// The index of the first slot.
    base: Index,
    /// The indices of the elements open, innermost last.
    open: Vec<Index>,
}

/// Where an item is in `Pending`, counting every item read from the first,
/// so it stays valid as the items before it are emitted. A position in a
/// plain slice of items is a `usize` instead.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Index(usize);

impl Index {
    /// The index `by` items after this one.
    pub(super) fn advance(self, by: usize) -> Index {
        Index(self.0 + by)
    }

    /// The index of the item right before this one, unless it's the first.
    pub(super) fn previous(self) -> Option<Index> {
        self.0.checked_sub(1).map(Index)
    }
}

/// An item read, and held until no more attribute lists can belong to it.
pub(super) struct Slot<'a> {
    item: Item<'a>,
    /// For an end, the index of its start.
    start: Option<Index>,
}

impl<'a> Pending<'a> {
    /// Pushes `event`, opening the element it starts or closing the one it
    /// ends, and returns its index.
    pub(super) fn push(&mut self, event: Event<'a>, range: Range<usize>) -> Index {
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

    /// Removes the last item, which mustn't be an end. If it started an
    /// element, that element is no longer open.
    pub(super) fn pop(&mut self) -> Option<Item<'a>> {
        let slot = self.slots.pop()?;
        debug_assert!(slot.start.is_none(), "an end popped");
        if self.open.last() == Some(&self.end()) {
            self.open.pop();
        }
        Some(slot.item)
    }

    /// Removes the last item, an end, so the element it closed is open
    /// again. Returns the index of the element's start.
    pub(super) fn reopen(&mut self) -> Index {
        let slot = self.slots.pop().expect("an end");
        let start = slot.start.expect("an end");
        self.open.push(start);
        start
    }

    /// Drops the item at `i` and every one after it, along with any elements
    /// they left open.
    pub(super) fn truncate(&mut self, i: Index) {
        self.slots.truncate(self.offset(i));
        self.open.retain(|&open| open < i);
    }

    /// Removes the items before `end`, to emit them.
    pub(super) fn take_before(&mut self, end: Index) -> Vec<Item<'a>> {
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

    /// Where the item at `i` is in `slots`. It mustn't be emitted yet.
    pub(super) fn offset(&self, i: Index) -> usize {
        i.0.checked_sub(self.base.0)
            .expect("an item not yet emitted")
    }

    pub(super) fn attach(&mut self, i: Index, attribute_lists: AttributeLists<'a>) {
        self.item_mut(i).attribute_lists.extend(attribute_lists);
    }

    pub(super) fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The index the next item pushed gets.
    pub(super) fn end(&self) -> Index {
        self.base.advance(self.slots.len())
    }

    pub(super) fn last_index(&self) -> Option<Index> {
        self.end().previous().filter(|_| !self.slots.is_empty())
    }

    pub(super) fn last(&self) -> Option<&Item<'a>> {
        self.slots.last().map(|slot| &slot.item)
    }

    pub(super) fn get(&self, i: Index) -> Option<&Item<'a>> {
        if i < self.base {
            return None;
        }
        let slot = self.slots.get(self.offset(i))?;
        Some(&slot.item)
    }

    pub(super) fn item(&self, i: Index) -> &Item<'a> {
        &self.slots[self.offset(i)].item
    }

    pub(super) fn item_mut(&mut self, i: Index) -> &mut Item<'a> {
        let offset = self.offset(i);
        &mut self.slots[offset].item
    }

    /// The index of the start that matches the end at `i`.
    pub(super) fn start_of(&self, i: Index) -> Index {
        self.slots[self.offset(i)].start.expect("an end")
    }

    /// The indices of the elements open, outermost first.
    pub(super) fn open(&self) -> impl Iterator<Item = Index> + '_ {
        self.open.iter().copied()
    }

    /// The index of the innermost element open.
    pub(super) fn innermost(&self) -> Option<Index> {
        self.open.last().copied()
    }

    /// Whether the innermost open element is a paragraph with nothing in it.
    pub(super) fn is_empty_paragraph(&self) -> bool {
        self.innermost().is_some_and(|i| {
            self.last_index() == Some(i)
                && matches!(self.item(i).event, Event::Start(Tag::Paragraph))
        })
    }

    pub(super) fn in_code_block(&self) -> bool {
        self.innermost()
            .is_some_and(|i| matches!(self.item(i).event, Event::Start(Tag::CodeBlock(_))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::TagEnd;

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
        // What's already emitted can't be taken again.
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
        // Popping an item that opened nothing leaves the open elements alone.
        pending.push(Event::SoftBreak, 0..1);
        pending.pop();
        assert_eq!(pending.innermost(), Some(table));
    }
}

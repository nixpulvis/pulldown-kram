//! kramdown's list items, from pulldown-cmark's: whether an item's first
//! paragraph is bare is decided for each item, not for the whole list.

use std::ops::Range;

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::read::Item;

/// The lists of a document, read as kramdown reads them.
pub(crate) struct ListItems<'t> {
    text: &'t str,
}

/// The paragraph an item starts with.
struct First {
    /// The index of its first item, and of its last.
    items: Range<usize>,
    /// Whether it's wrapped in a paragraph's start and end.
    wrapped: bool,
    /// Where it is in the text.
    range: Range<usize>,
}

impl<'t> ListItems<'t> {
    pub fn new(text: &'t str) -> Self {
        Self { text }
    }

    /// Wraps the first paragraph of each item in `items` that kramdown
    /// wraps, and unwraps each it leaves bare, giving the item the
    /// paragraph's attributes.
    pub fn rewrite(&self, items: &mut Vec<Item<'_>>) {
        // Inner lists first, so the indices of those around them hold.
        for list in (0..items.len()).rev() {
            if matches!(items[list].event, Event::Start(Tag::List(_))) {
                self.rewrite_list(items, list);
            }
        }
    }

    fn rewrite_list(&self, items: &mut Vec<Item<'_>>, list: usize) {
        let end = Item::end_of(items, list);
        let mut entries = Vec::new();
        let mut i = list + 1;
        while i < end {
            let item_end = Item::end_of(items, i);
            entries.push((i, item_end));
            i = item_end + 1;
        }
        // For each item, its first paragraph, and whether kramdown leaves it
        // bare.
        let mut firsts: Vec<Option<(First, bool)>> = Vec::with_capacity(entries.len());
        for (n, &(item, item_end)) in entries.iter().enumerate() {
            let Some(first) = Self::first(items, item, item_end) else {
                firsts.push(None);
                continue;
            };
            let last = n + 1 == entries.len();
            let only = first.items.end + 1 + usize::from(first.wrapped) == item_end;
            let blank = self.blank_line_after(first.range.end);
            // kramdown's rule: bare unless a blank line follows, but for the
            // last item's only paragraph, and then only if an item before
            // it is bare too, or doesn't start with a paragraph.
            let first_bare = !blank || last && only;
            let after_others = !last
                || entries.len() == 1
                || firsts
                    .iter()
                    .any(|other| !matches!(other, Some((_, false))));
            firsts.push(Some((first, first_bare && after_others)));
        }
        for (first, bare) in firsts.into_iter().rev().flatten() {
            match (first.wrapped, bare) {
                (true, true) => {
                    let paragraph = first.items.start - 1;
                    let attribute_lists = std::mem::take(&mut items[paragraph].attribute_lists);
                    items[paragraph - 1].attribute_lists.extend(attribute_lists);
                    items.remove(first.items.end + 1);
                    items.remove(paragraph);
                }
                (false, false) => {
                    let range = first.range;
                    items.insert(
                        first.items.end + 1,
                        Item::new(Event::End(TagEnd::Paragraph), range.clone()),
                    );
                    items.insert(
                        first.items.start,
                        Item::new(Event::Start(Tag::Paragraph), range),
                    );
                }
                _ => {}
            }
        }
    }

    /// The paragraph the item from `item` to `item_end` starts with, if it
    /// starts with one: wrapped, as in a loose list, or bare, as in a tight
    /// one.
    fn first(items: &[Item<'_>], item: usize, item_end: usize) -> Option<First> {
        let start = item + 1;
        if matches!(items[start].event, Event::Start(Tag::Paragraph)) {
            let end = Item::end_of(items, start);
            return Some(First {
                items: start + 1..end - 1,
                wrapped: true,
                range: items[start].range.clone(),
            });
        }
        let len = items[start..item_end]
            .iter()
            .position(|item| !Item::inline(&item.event))
            .unwrap_or(item_end - start);
        (len > 0).then(|| First {
            items: start..start + len - 1,
            wrapped: false,
            range: items[start].range.start..items[start + len - 1].range.end,
        })
    }

    /// Whether the line after the one `end` ends is blank, or blank but for
    /// quote markers. The text's end isn't a blank line.
    fn blank_line_after(&self, end: usize) -> bool {
        let next = if self.text[..end].ends_with('\n') {
            end
        } else {
            match self.text[end..].find('\n') {
                Some(i) => end + i + 1,
                None => return false,
            }
        };
        let rest = &self.text[next..];
        let line = rest.split('\n').next().unwrap_or(rest);
        !rest.is_empty()
            && line
                .trim_matches(|c: char| c.is_whitespace() || c == '>')
                .is_empty()
    }
}

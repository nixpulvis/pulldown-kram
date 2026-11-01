//! kramdown's tables of contents, lists marked `{:toc}` made into links to
//! the document's headings.

use std::ops::Range;

use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};

use crate::read::Item;

/// Replaces each list marked `{:toc}` with a list of links to the
/// headings, nested by level, leaving out those with the class `no_toc`.
pub(crate) fn tables_of_contents(items: Vec<Item<'_>>) -> Vec<Item<'_>> {
    if !items.iter().any(|item| item.lists.toc) {
        return items;
    }
    let entries = entries(&items);
    let mut tocs = Vec::new();
    let mut i = 0;
    while i < items.len() {
        if items[i].lists.toc && matches!(items[i].event, Event::Start(Tag::List(_))) {
            let end = Item::end_of(&items, i);
            let mut toc = Vec::new();
            Toc::new(&entries).write(&items, i, &mut toc);
            tocs.push((i..end + 1, toc));
            i = end;
        }
        i += 1;
    }
    let mut out = Vec::with_capacity(items.len());
    let mut tocs = tocs.into_iter().peekable();
    for (i, item) in items.into_iter().enumerate() {
        match tocs.peek_mut() {
            Some((range, toc)) if range.contains(&i) => {
                out.append(toc);
                if i + 1 == range.end {
                    tocs.next();
                }
            }
            _ => out.push(item),
        }
    }
    out
}

/// A heading listed in a table of contents.
struct Entry<'a> {
    level: usize,
    id: CowStr<'a>,
    /// Where its content is.
    content: Range<usize>,
}

fn entries<'a>(items: &[Item<'a>]) -> Vec<Entry<'a>> {
    let mut entries = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if let Event::Start(Tag::Heading { level, id, .. }) = &item.event
            && !item.lists.attributes.has_class("no_toc")
        {
            entries.push(Entry {
                level: *level as usize,
                id: id.clone().unwrap_or(CowStr::Borrowed("")),
                content: i + 1..Item::end_of(items, i),
            });
        }
    }
    entries
}

/// The headings of a table of contents, as a tree: each one under the last
/// before it of a higher level.
struct Toc<'e, 'a> {
    entries: &'e [Entry<'a>],
    /// The entries under each entry.
    children: Vec<Vec<usize>>,
    /// The entries under none.
    roots: Vec<usize>,
}

impl<'e, 'a> Toc<'e, 'a> {
    fn new(entries: &'e [Entry<'a>]) -> Self {
        let mut children = vec![Vec::new(); entries.len()];
        let mut roots = Vec::new();
        let mut path: Vec<usize> = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            while path
                .last()
                .is_some_and(|&p| entries[p].level >= entry.level)
            {
                path.pop();
            }
            match path.last() {
                Some(&parent) => children[parent].push(i),
                None => roots.push(i),
            }
            path.push(i);
        }
        Self {
            entries,
            children,
            roots,
        }
    }

    /// Writes the table of contents standing in for the list at `list`.
    fn write(&self, items: &[Item<'a>], list: usize, out: &mut Vec<Item<'a>>) {
        let Event::Start(Tag::List(first)) = items[list].event else {
            return;
        };
        let range = items[list].range.clone();
        let mut start = Item::new(Event::Start(Tag::List(first)), range.clone());
        start.lists.attributes = items[list].lists.attributes.clone();
        out.push(start);
        self.entries(&self.roots, first, items, &range, out);
        out.push(Item::new(Event::End(TagEnd::List(first.is_some())), range));
    }

    fn entries(
        &self,
        entries: &[usize],
        first: Option<u64>,
        items: &[Item<'a>],
        range: &Range<usize>,
        out: &mut Vec<Item<'a>>,
    ) {
        let item = |event| Item::new(event, range.clone());
        for &i in entries {
            let entry = &self.entries[i];
            out.push(item(Event::Start(Tag::Item)));
            out.push(item(Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: format!("#{}", entry.id).into(),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            })));
            // Links within the heading would nest, and footnotes don't
            // belong in it.
            out.extend(
                items[entry.content.clone()]
                    .iter()
                    .filter(|item| {
                        !matches!(
                            item.event,
                            Event::Start(Tag::Link { .. })
                                | Event::End(TagEnd::Link)
                                | Event::FootnoteReference(_)
                        )
                    })
                    .cloned(),
            );
            out.push(item(Event::End(TagEnd::Link)));
            if !self.children[i].is_empty() {
                out.push(item(Event::Start(Tag::List(first))));
                self.entries(&self.children[i], first, items, range, out);
                out.push(item(Event::End(TagEnd::List(first.is_some()))));
            }
            out.push(item(Event::End(TagEnd::Item)));
        }
    }
}

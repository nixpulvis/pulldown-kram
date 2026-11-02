//! kramdown's tables of contents, lists marked `{:toc}` made into links to
//! the document's headings.
//!
//! Every table lists every heading, so a document of many tables and many
//! headings would grow with the square of its size. The tables share a
//! budget of items, the larger of the document's items and 100,000, as
//! pulldown-cmark limits link reference expansion. Once a table doesn't fit
//! in what's left, its list and every later marked list stay as written.

use std::ops::Range;

use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};

use crate::read::Item;

/// A document's headings, as a tree for its tables of contents: each one
/// under the last before it of a higher level.
pub(crate) struct Toc<'a> {
    entries: Vec<Entry<'a>>,
    /// The entries under each entry.
    children: Vec<Vec<usize>>,
    /// The entries under none.
    roots: Vec<usize>,
}

/// A heading listed in a table of contents.
struct Entry<'a> {
    level: usize,
    id: CowStr<'a>,
    /// Its text and markup, less links, which would nest in the entry's
    /// own, and footnotes, which don't belong in it.
    content: Vec<Item<'a>>,
}

impl<'a> Toc<'a> {
    /// Replaces each list in `items` marked `{:toc}` with a list of links to
    /// the headings, nested by level, leaving out those with the class
    /// `no_toc`. The tables give out at most the larger of `items.len()`
    /// and 100,000 items; from the first that doesn't fit, lists stay as
    /// written.
    pub fn replace_lists(items: Vec<Item<'a>>) -> Vec<Item<'a>> {
        let budget = items.len().max(100_000);
        Self::replace_lists_within(items, budget)
    }

    /// Replaces the lists marked `{:toc}` as long as the tables fit in
    /// `budget` items, in one pass.
    fn replace_lists_within(items: Vec<Item<'a>>, mut budget: usize) -> Vec<Item<'a>> {
        let is_mark =
            |item: &Item<'a>| item.lists.toc && matches!(item.event, Event::Start(Tag::List(_)));
        if !items.iter().any(is_mark) {
            return items;
        }
        let toc = Self::new(&items);
        let size = toc.size();
        // The lists made into tables, as their first and last items.
        let mut lists = Vec::new();
        let mut i = 0;
        while i < items.len() {
            if is_mark(&items[i]) {
                if size > budget {
                    // This and every later list stay as written.
                    break;
                }
                budget -= size;
                let end = Item::end_of(&items, i);
                lists.push((i, end));
                i = end;
            }
            i += 1;
        }
        let mut out = Vec::with_capacity(items.len() + lists.len() * size);
        let mut lists = lists.into_iter().peekable();
        let mut skip_to = None;
        for (i, item) in items.into_iter().enumerate() {
            if let Some(end) = skip_to {
                if i == end {
                    skip_to = None;
                }
                continue;
            }
            if let Some(&(start, end)) = lists.peek()
                && start == i
            {
                lists.next();
                toc.write_list(&item, &mut out);
                if end != i {
                    skip_to = Some(end);
                }
                continue;
            }
            out.push(item);
        }
        out
    }

    /// The number of items in each table of contents, none if there are no
    /// headings to list.
    fn size(&self) -> usize {
        if self.entries.is_empty() {
            return 0;
        }
        2 + (0..self.entries.len())
            .map(|i| {
                4 + self.entries[i].content.len() + if self.children[i].is_empty() { 0 } else { 2 }
            })
            .sum::<usize>()
    }

    fn new(items: &[Item<'a>]) -> Self {
        let entries = Entry::all(items);
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

    /// Writes the table of contents standing in for the list `list` starts,
    /// or nothing, as kramdown does, if there are no headings to list.
    fn write_list(&self, list: &Item<'a>, out: &mut Vec<Item<'a>>) {
        let Event::Start(Tag::List(start)) = list.event else {
            return;
        };
        if self.roots.is_empty() {
            return;
        }
        out.push(Item {
            lists: list.lists.clone(),
            ..Item::new(Event::Start(Tag::List(start)), list.range.clone())
        });
        self.write_entries(&self.roots, start, &list.range, out);
        out.push(Item::new(
            Event::End(TagEnd::List(start.is_some())),
            list.range.clone(),
        ));
    }

    /// Writes an item linking to each of `entries`, with a list of the
    /// entries under it, numbered from `start` if the table is numbered.
    fn write_entries(
        &self,
        entries: &[usize],
        start: Option<u64>,
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
            out.extend(entry.content.iter().cloned());
            out.push(item(Event::End(TagEnd::Link)));
            if !self.children[i].is_empty() {
                out.push(item(Event::Start(Tag::List(start))));
                self.write_entries(&self.children[i], start, range, out);
                out.push(item(Event::End(TagEnd::List(start.is_some()))));
            }
            out.push(item(Event::End(TagEnd::Item)));
        }
    }
}

impl<'a> Entry<'a> {
    /// The headings in `items` with an id and without the class `no_toc`.
    fn all(items: &[Item<'a>]) -> Vec<Self> {
        let mut entries = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let Event::Start(Tag::Heading {
                level,
                id: Some(id),
                ..
            }) = &item.event
            else {
                continue;
            };
            if item.lists.attributes.has_class("no_toc") {
                continue;
            }
            let content = items[i + 1..Item::end_of(items, i)]
                .iter()
                .filter(|item| {
                    !matches!(
                        item.event,
                        Event::Start(Tag::Link { .. })
                            | Event::End(TagEnd::Link)
                            | Event::FootnoteReference(_)
                    )
                })
                .cloned()
                .collect();
            entries.push(Self {
                level: *level as usize,
                id: id.clone(),
                content,
            });
        }
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::Reader;
    use crate::{Extensions, Options};

    /// The items `text` reads as with `extensions`, tables of contents
    /// made.
    fn toc(text: &str, extensions: Extensions) -> Vec<Item<'_>> {
        Toc::replace_lists(Reader::new(text, Options::from(extensions), &[]).collect())
    }

    /// The lists and links in `items` before the first heading, as
    /// HTML-like text.
    fn outline(items: &[Item<'_>]) -> String {
        items
            .iter()
            .take_while(|item| !matches!(item.event, Event::Start(Tag::Heading { .. })))
            .map(|item| match &item.event {
                Event::Start(Tag::List(None)) => "<ul>".into(),
                Event::End(TagEnd::List(false)) => "</ul>".into(),
                Event::Start(Tag::List(Some(start))) => format!("<ol start={start}>"),
                Event::End(TagEnd::List(true)) => "</ol>".into(),
                Event::Start(Tag::Item) => "<li>".into(),
                Event::End(TagEnd::Item) => "</li>".into(),
                Event::Start(Tag::Link { dest_url, .. }) => format!("<a {dest_url}>"),
                Event::End(TagEnd::Link) => "</a>".into(),
                Event::Start(Tag::Paragraph) => "<p>".into(),
                Event::End(TagEnd::Paragraph) => "</p>".into(),
                Event::Text(text) => text.to_string(),
                event => panic!("unexpected {event:?}"),
            })
            .collect()
    }

    #[test]
    fn toc_replace_lists_puts_entries_under_the_last_heading_of_a_higher_level() {
        let items = toc("* x\n{:toc}\n\n## A\n# B\n### C\n## D\n", Extensions::all());
        assert_eq!(
            outline(&items),
            "<ul><li><a #a>A</a></li><li><a #b>B</a><ul><li><a #c>C</a></li><li><a #d>D</a></li></ul></li></ul>"
        );
    }

    #[test]
    fn toc_replace_lists_keeps_the_lists_numbering_and_attributes() {
        let items = toc("3. x\n{:toc .t}\n\n# A\n", Extensions::all());
        assert_eq!(outline(&items), "<ol start=3><li><a #a>A</a></li></ol>");
        assert!(items[0].lists.attributes.has_class("t"));
    }

    #[test]
    fn toc_replace_lists_leaves_out_headings_without_ids() {
        let items = toc(
            "* x\n{:toc}\n\n# A\n# B {#b}\n",
            Extensions::all() - Extensions::ENABLE_AUTO_IDS,
        );
        assert_eq!(outline(&items), "<ul><li><a #b>B</a></li></ul>");
        // With no headings, there's no table, nor the list.
        let items = toc("* x\n{:toc}\n\np\n", Extensions::all());
        assert_eq!(outline(&items), "<p>p</p>");
    }

    #[test]
    fn toc_replace_lists_leaves_unmarked_lists() {
        let items = toc("* x\n\n# A\n", Extensions::all());
        assert_eq!(outline(&items), "<ul><li>x</li></ul>");
    }

    #[test]
    fn toc_replace_lists_within_leaves_tables_past_the_budget_as_written() {
        let text = "* x\n{:toc}\n\np\n\n* x\n{:toc}\n\np\n\n* x\n{:toc}\n\n# A\n";
        let read = || Reader::new(text, Options::from(Extensions::all()), &[]).collect();
        let made = "<ul><li><a #a>A</a></li></ul>";
        let written = "<ul><li>x</li></ul>";
        let p = "<p>p</p>";
        // Each table is 7 items: the list, the item, the link and its text.
        let items = Toc::replace_lists_within(read(), 14);
        assert_eq!(outline(&items), [made, p, made, p, written].concat());
        // One short of the second table leaves it, and the third, alone.
        let items = Toc::replace_lists_within(read(), 13);
        assert_eq!(outline(&items), [made, p, written, p, written].concat());
        let items = Toc::replace_lists_within(read(), 6);
        assert_eq!(outline(&items), [written, p, written, p, written].concat());
        // The heading after the lists is kept either way.
        assert!(matches!(
            &items[items.len() - 2].event,
            Event::Text(text) if &**text == "A"
        ));
    }

    #[test]
    fn toc_replace_lists_budget_is_at_least_100_000_items() {
        // 300 tables of 300 headings would be 450,600 items, far more than
        // the document's, so only as many tables as fit in 100,000 are made.
        let text = "* x\n{:toc}\n\n# h\n\n".repeat(300);
        let items = toc(&text, Extensions::all());
        let links = items
            .iter()
            .filter(|item| matches!(item.event, Event::Start(Tag::Link { .. })))
            .count();
        let size = 2 + 300 * 5;
        assert_eq!(links, 100_000 / size * 300);
    }
}

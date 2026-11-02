//! kramdown's tables of contents, lists marked `{:toc}` made into links to
//! the document's headings.

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
    /// `no_toc`.
    pub fn replace_lists(mut items: Vec<Item<'a>>) -> Vec<Item<'a>> {
        let lists: Vec<usize> = (0..items.len())
            .filter(|&i| items[i].lists.toc && matches!(items[i].event, Event::Start(Tag::List(_))))
            .collect();
        if lists.is_empty() {
            return items;
        }
        let toc = Self::new(&items);
        // From the back, so the lists before each one stay where they are.
        for &list in lists.iter().rev() {
            let end = Item::end_of(&items, list);
            let links = toc.list(&items[list]);
            items.splice(list..=end, links);
        }
        items
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

    /// The table of contents standing in for the list `list` starts.
    fn list(&self, list: &Item<'a>) -> Vec<Item<'a>> {
        let Event::Start(Tag::List(start)) = list.event else {
            return Vec::new();
        };
        let mut out = vec![Item {
            lists: list.lists.clone(),
            ..Item::new(Event::Start(Tag::List(start)), list.range.clone())
        }];
        self.write_entries(&self.roots, start, &list.range, &mut out);
        out.push(Item::new(
            Event::End(TagEnd::List(start.is_some())),
            list.range.clone(),
        ));
        out
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
        Toc::replace_lists(Reader::new(text, Options::from(extensions)).collect())
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
                Event::Text(text) => text.to_string(),
                event => panic!("unexpected {event:?}"),
            })
            .collect()
    }

    #[test]
    fn entries_go_under_the_last_heading_of_a_higher_level() {
        let items = toc("* x\n{:toc}\n\n## A\n# B\n### C\n## D\n", Extensions::all());
        assert_eq!(
            outline(&items),
            "<ul><li><a #a>A</a></li><li><a #b>B</a><ul><li><a #c>C</a></li><li><a #d>D</a></li></ul></li></ul>"
        );
    }

    #[test]
    fn the_list_keeps_its_numbering_and_attributes() {
        let items = toc("3. x\n{:toc .t}\n\n# A\n", Extensions::all());
        assert_eq!(outline(&items), "<ol start=3><li><a #a>A</a></li></ol>");
        assert!(items[0].lists.attributes.has_class("t"));
    }

    #[test]
    fn headings_without_ids_are_left_out() {
        let items = toc(
            "* x\n{:toc}\n\n# A\n# B {#b}\n",
            Extensions::all() - Extensions::ENABLE_AUTO_IDS,
        );
        assert_eq!(outline(&items), "<ul><li><a #b>B</a></li></ul>");
        // With no headings, the list is left empty.
        let items = toc("* x\n{:toc}\n", Extensions::all());
        assert_eq!(outline(&items), "<ul></ul>");
    }

    #[test]
    fn unmarked_lists_stay() {
        let items = toc("* x\n\n# A\n", Extensions::all());
        assert_eq!(outline(&items), "<ul><li>x</li></ul>");
    }
}

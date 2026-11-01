//! Heading ids, and the tables of contents linking to them.

use std::collections::HashMap;
use std::ops::Range;

use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};

use crate::read::Item;

/// Gives every heading an id: its own, from `{#id}` after its text or an
/// attribute list, or else one made from its text.
pub(crate) fn ids<'a>(items: &mut [Item<'a>], text: &'a str) {
    let mut ids = HeaderIds::default();
    let mut i = 0;
    while i < items.len() {
        if !matches!(items[i].event, Event::Start(Tag::Heading { .. })) {
            i += 1;
            continue;
        }
        let end = end_of(items, i);
        let heading = items[i].range.start;
        let explicit = explicit_id(&mut items[i + 1..end], text, heading);
        // An attribute list's id wins over `{#id}`, coming after it.
        let id = items[i]
            .attributes
            .id
            .clone()
            .map(CowStr::from)
            .or(explicit);
        let id = id.unwrap_or_else(|| CowStr::from(ids.next(&raw_text(&items[i + 1..end]))));
        if let Event::Start(Tag::Heading { id: slot, .. }) = &mut items[i].event {
            *slot = Some(id);
        }
        i = end + 1;
    }
}

/// Replaces each list marked `{:toc}` with a list of links to the
/// headings, nested by level, leaving out those with the class `no_toc`.
pub(crate) fn tables_of_contents(items: Vec<Item<'_>>) -> Vec<Item<'_>> {
    if !items.iter().any(|item| item.toc) {
        return items;
    }
    let entries = entries(&items);
    let mut tocs = Vec::new();
    let mut i = 0;
    while i < items.len() {
        if items[i].toc && matches!(items[i].event, Event::Start(Tag::List(_))) {
            let end = end_of(&items, i);
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

/// Hands out ids the way kramdown's GFM parser does: the text in lowercase,
/// keeping only word characters, dashes, spaces and tabs, with each space or
/// tab made a dash, and a count added to repeats, like `intro-1`.
#[derive(Debug, Default)]
struct HeaderIds {
    counts: HashMap<String, usize>,
}

impl HeaderIds {
    fn next(&mut self, text: &str) -> String {
        let id: String = text
            .chars()
            .flat_map(char::to_lowercase)
            .filter(|&c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' ' | '\t'))
            .map(|c| if matches!(c, ' ' | '\t') { '-' } else { c })
            .collect();
        match self.counts.get_mut(&id) {
            Some(count) => {
                *count += 1;
                format!("{id}-{count}")
            }
            None => {
                self.counts.insert(id.clone(), 0);
                id
            }
        }
    }
}

/// Takes a `{#id}` off the end of a heading's text, along with closing `#`s
/// before it in an atx heading, returning the id.
fn explicit_id<'a>(content: &mut [Item<'a>], text: &'a str, heading: usize) -> Option<CowStr<'a>> {
    let last = content
        .iter_mut()
        .rev()
        .find(|item| matches!(item.event, Event::Text(_)))?;
    let Event::Text(words) = &last.event else {
        return None;
    };
    let body = words.trim_end().strip_suffix('}')?;
    let open = body.rfind("{#")?;
    let id = &body[open + 2..];
    let valid = id.starts_with(|c: char| c.is_ascii_alphabetic())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '-'));
    let before = &body[..open];
    if !valid || !before.ends_with([' ', '\t']) {
        return None;
    }
    let mut kept = before.trim_end();
    let unhashed = kept.trim_end_matches('#');
    let atx = text[heading..].starts_with('#');
    if atx
        && unhashed.len() < kept.len()
        && (unhashed.is_empty() || unhashed.ends_with([' ', '\t']))
    {
        kept = unhashed.trim_end();
    }
    let id = open + 2..body.len();
    let source = text.get(last.range.clone()).filter(|s| *s == &**words);
    let part = |part: Range<usize>| match source {
        Some(_) => {
            CowStr::Borrowed(&text[last.range.start + part.start..last.range.start + part.end])
        }
        None => CowStr::from(words[part].to_owned()),
    };
    let id = part(id);
    last.event = Event::Text(part(0..kept.len()));
    Some(id)
}

/// The text kramdown makes a heading's id from: its text, code and math,
/// without the rest of the markup.
fn raw_text(content: &[Item<'_>]) -> String {
    let mut text = String::new();
    for item in content {
        if let Event::Text(t) | Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) =
            &item.event
        {
            text.push_str(t);
        }
    }
    text
}

/// The index of the end of the element starting at `start`.
fn end_of(items: &[Item<'_>], start: usize) -> usize {
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
            && !item.attributes.has_class("no_toc")
        {
            entries.push(Entry {
                level: *level as usize,
                id: id.clone().unwrap_or(CowStr::Borrowed("")),
                content: i + 1..end_of(items, i),
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
        start.attributes = items[list].attributes.clone();
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

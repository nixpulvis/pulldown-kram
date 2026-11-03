//! Heading ids, written after a heading's text or made from it.

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

use pulldown_cmark::{CowStr, Event, Tag};

use crate::Extensions;
use crate::read::Item;

/// Hands out ids the way kramdown's GFM parser does: the text in lowercase,
/// keeping only word characters, dashes, spaces and tabs, with each space or
/// tab made a dash, and a count added to repeats, like `intro-1`.
#[derive(Debug, Default)]
pub(crate) struct HeaderIds {
    counts: HashMap<String, usize>,
}

impl HeaderIds {
    /// Gives every heading in `items` an id, as far as `extensions` allow:
    /// its own, from `{#id}` after its text or an attribute list, or else
    /// one made from its text.
    pub fn assign<'a>(&mut self, items: &mut [Item<'a>], text: &'a str, extensions: Extensions) {
        let mut i = 0;
        while i < items.len() {
            if !matches!(items[i].event, Event::Start(Tag::Heading { .. })) {
                i += 1;
                continue;
            }
            let end = Item::end_of(items, i);
            let mut heading = Heading::new(&mut items[i..end]);
            let explicit = extensions
                .contains(Extensions::ENABLE_HEADER_IDS)
                .then(|| heading.take_explicit_id(text))
                .flatten();
            // An attribute list's id wins over `{#id}`, coming after it.
            let id = heading
                .start
                .attribute_lists
                .attributes
                .id
                .clone()
                .or(explicit);
            let id = id.or_else(|| {
                extensions
                    .contains(Extensions::ENABLE_AUTO_IDS)
                    .then(|| CowStr::from(self.unique(&heading.raw_text())))
            });
            heading.set_id(id);
            i = end + 1;
        }
    }

    /// The id made from `text`, counted if it was made before.
    fn unique(&mut self, text: &str) -> String {
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

/// A heading's start and what's in it, up to its end.
struct Heading<'h, 'a> {
    start: &'h mut Item<'a>,
    content: &'h mut [Item<'a>],
}

impl<'h, 'a> Heading<'h, 'a> {
    /// The heading `items` hold, from its start to just before its end.
    fn new(items: &'h mut [Item<'a>]) -> Self {
        let (start, content) = items.split_first_mut().expect("a heading's start");
        Self { start, content }
    }

    fn set_id(&mut self, id: Option<CowStr<'a>>) {
        if let Event::Start(Tag::Heading { id: slot, .. }) = &mut self.start.event {
            *slot = id;
        }
    }

    /// Takes a `{#id}` off the end of the heading's text, read from `text`,
    /// along with closing `#`s before it in an atx heading, returning the id.
    /// The text may be in more than one event, as pulldown-cmark ends one
    /// at each `_` or `*` that could have been emphasis.
    fn take_explicit_id(&mut self, text: &'a str) -> Option<CowStr<'a>> {
        let atx = text[self.start.range.start..].starts_with('#');
        let last = self
            .content
            .iter()
            .rposition(|item| matches!(item.event, Event::Text(_)))?;
        let mut first = last;
        while first > 0
            && matches!(self.content[first - 1].event, Event::Text(_))
            && self.content[first - 1].range.end == self.content[first].range.start
        {
            first -= 1;
        }
        let run = &self.content[first..=last];
        let words: Cow<'_, str> = match run {
            [only] => Cow::Borrowed(Self::text_of(only)),
            _ => Cow::Owned(run.iter().map(Self::text_of).collect()),
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
        if atx
            && unhashed.len() < kept.len()
            && (unhashed.is_empty() || unhashed.ends_with([' ', '\t']))
        {
            kept = unhashed.trim_end();
        }
        let start = run[0].range.start;
        let source = text
            .get(start..run[run.len() - 1].range.end)
            .is_some_and(|source| source == words);
        let part = |part: Range<usize>| {
            if source {
                CowStr::Borrowed(&text[start + part.start..start + part.end])
            } else {
                CowStr::from(words[part].to_owned())
            }
        };
        let id = part(open + 2..body.len());
        let kept = part(0..kept.len());
        self.content[first].event = Event::Text(kept);
        for item in &mut self.content[first + 1..=last] {
            item.event = Event::Text(CowStr::Borrowed(""));
        }
        Some(id)
    }

    /// The text of `item`, a text event.
    fn text_of<'i>(item: &'i Item<'a>) -> &'i str {
        match &item.event {
            Event::Text(text) => text,
            _ => unreachable!("a text event"),
        }
    }

    /// The text kramdown makes the heading's id from: its text, code and
    /// math, without the rest of the markup.
    fn raw_text(&self) -> String {
        let mut text = String::new();
        for item in self.content.iter() {
            if let Event::Text(t) | Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) =
                &item.event
            {
                text.push_str(t);
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Parser;

    /// The items `text` reads as, given ids with every extension, after
    /// `before` has changed them.
    fn assign<'a>(text: &'a str, before: impl FnOnce(&mut [Item<'a>])) -> Vec<Item<'a>> {
        let mut items: Vec<_> = Parser::new(text)
            .into_offset_iter()
            .map(|(event, range)| Item::new(event, range))
            .collect();
        before(&mut items);
        HeaderIds::default().assign(&mut items, text, Extensions::all());
        items
    }

    /// Each heading's id and text.
    fn headings(items: &[Item<'_>]) -> Vec<(String, String)> {
        let mut headings = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if let Event::Start(Tag::Heading { id, .. }) = &item.event {
                let text = items[i + 1..Item::end_of(items, i)]
                    .iter()
                    .filter_map(|item| match &item.event {
                        Event::Text(text) => Some(text.as_ref()),
                        _ => None,
                    })
                    .collect();
                headings.push((id.as_deref().unwrap_or_default().to_owned(), text));
            }
        }
        headings
    }

    fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|&(id, text)| (id.to_owned(), text.to_owned()))
            .collect()
    }

    #[test]
    fn header_ids_assign_explicit_ids_need_a_space_and_a_name() {
        let text = "# A {#b}\n# C{#d}\n# E {#1f}\n# G {#h i}\n# J {#k} *l*\n# M {#n_:o}\n";
        assert_eq!(
            headings(&assign(text, |_| {})),
            pairs(&[
                ("b", "A"),
                ("cd", "C{#d}"),
                ("e-1f", "E {#1f}"),
                ("g-h-i", "G {#h i}"),
                ("j-k-l", "J {#k} l"),
                ("n_:o", "M"),
            ])
        );
    }

    #[test]
    fn header_ids_assign_closing_hashes_go_only_in_atx_headings() {
        let text = "# A # {#b}\n\nC # {#d}\n===\n\n# E# {#f}\n";
        assert_eq!(
            headings(&assign(text, |_| {})),
            pairs(&[("b", "A"), ("d", "C #"), ("f", "E#")])
        );
    }

    #[test]
    fn header_ids_assign_an_attribute_lists_id_wins() {
        let items = assign("# A {#b}\n", |items| {
            items[0].attribute_lists.attributes.id = Some("c".into());
        });
        assert_eq!(headings(&items), pairs(&[("c", "A")]));
    }

    #[test]
    fn header_ids_unique_keeps_word_characters_and_counts_repeats() {
        let mut ids = HeaderIds::default();
        assert_eq!(ids.unique("Ünï Code!\tx_y-z"), "ünï-code-x_y-z");
        assert_eq!(ids.unique("A"), "a");
        assert_eq!(ids.unique("a"), "a-1");
        assert_eq!(ids.unique("A!"), "a-2");
    }
}

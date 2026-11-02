//! Heading ids, written after a heading's text or made from it.

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
            let id = heading.start.lists.attributes.id.clone().or(explicit);
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
    fn take_explicit_id(&mut self, text: &'a str) -> Option<CowStr<'a>> {
        let atx = text[self.start.range.start..].starts_with('#');
        let last = self
            .content
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

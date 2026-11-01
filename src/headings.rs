//! Heading ids, and the tables of contents linking to them.

use std::collections::HashMap;
use std::ops::Range;

use pulldown_cmark::{CowStr, Event, Tag};

use crate::read::Item;

/// Gives every heading an id: its own, from `{#id}` after its text or an
/// attribute list, or else one made from its text.
pub(crate) fn ids<'a>(items: &mut [Item<'a>], text: &'a str, ids: &mut HeaderIds) {
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

/// Hands out ids the way kramdown's GFM parser does: the text in lowercase,
/// keeping only word characters, dashes, spaces and tabs, with each space or
/// tab made a dash, and a count added to repeats, like `intro-1`.
#[derive(Debug, Default)]
pub(crate) struct HeaderIds {
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
pub(crate) fn end_of(items: &[Item<'_>], start: usize) -> usize {
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

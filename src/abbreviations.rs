//! kramdown's abbreviations: each definition, `*[HTML]: HyperText Markup
//! Language`, makes every use of its abbreviation in the document's text
//! an `<abbr>` with its title.

use pulldown_cmark::{CowStr, Event, Tag, TagEnd};

use crate::attributes::{AttributeLists, Attributes};
use crate::read::Item;

/// An abbreviation defined in a document.
#[derive(Debug, Clone)]
pub(crate) struct Abbreviation<'a> {
    /// The abbreviation, as written.
    pub abbreviation: &'a str,
    /// What it stands for, empty for no title.
    pub title: &'a str,
    /// What attribute lists on the lines around its definition give it.
    pub attribute_lists: AttributeLists<'a>,
}

impl<'a> Abbreviation<'a> {
    /// The definition `line` holds, if it's one, like `*[HTML]: title`.
    pub fn defined(line: &'a str) -> Option<Self> {
        let trimmed = line.trim_end();
        let indent = trimmed.len() - trimmed.trim_start_matches(' ').len();
        let inside = trimmed.trim_start().strip_prefix("*[")?;
        let close = inside.find("]:")?;
        (indent < 4 && close > 0).then(|| Abbreviation {
            abbreviation: &inside[..close],
            title: inside[close + "]:".len()..].trim(),
            attribute_lists: AttributeLists::default(),
        })
    }

    /// The opening tag of an `<abbr>` for this abbreviation.
    fn tag(&self) -> String {
        let mut attributes = Attributes::default();
        attributes.extend(self.attribute_lists.attributes.clone());
        let mut tag = String::from("<abbr");
        if let Some(id) = &attributes.id {
            Self::attribute(&mut tag, "id", id);
        }
        if !attributes.classes.is_empty() {
            Self::attribute(&mut tag, "class", &attributes.classes.join(" "));
        }
        for (key, value) in &attributes.pairs {
            Self::attribute(&mut tag, key, value);
        }
        if !self.title.is_empty() {
            Self::attribute(&mut tag, "title", self.title);
        }
        tag.push('>');
        tag
    }

    fn attribute(tag: &mut String, key: &str, value: &str) {
        tag.push(' ');
        tag.push_str(key);
        tag.push_str("=\"");
        for c in value.chars() {
            match c {
                '&' => tag.push_str("&amp;"),
                '<' => tag.push_str("&lt;"),
                '>' => tag.push_str("&gt;"),
                '"' => tag.push_str("&quot;"),
                c => tag.push(c),
            }
        }
        tag.push('"');
    }
}

/// The abbreviations of a document, made `<abbr>`s in its text.
pub(crate) struct Abbreviations<'s, 'a> {
    /// The abbreviations, the last defined of each, longest first.
    defined: Vec<&'s Abbreviation<'a>>,
}

impl<'s, 'a> Abbreviations<'s, 'a> {
    pub fn new(abbreviations: &'s [Abbreviation<'a>]) -> Self {
        let mut defined: Vec<&Abbreviation> = Vec::new();
        for abbreviation in abbreviations.iter().rev() {
            if !defined
                .iter()
                .any(|other| other.abbreviation == abbreviation.abbreviation)
            {
                defined.push(abbreviation);
            }
        }
        defined.sort_by_key(|abbreviation| std::cmp::Reverse(abbreviation.abbreviation.len()));
        Self { defined }
    }

    /// Puts each use of an abbreviation in the text of `items` in an
    /// `<abbr>`, across the lines of a paragraph, but not in code.
    pub fn replace<'i>(&self, items: &mut Vec<Item<'i>>)
    where
        'a: 'i,
    {
        if self.defined.is_empty() {
            return;
        }
        let mut out = Vec::with_capacity(items.len());
        let mut code = 0usize;
        let mut run = Vec::new();
        for item in items.drain(..) {
            match item.event {
                Event::Start(Tag::CodeBlock(_)) => code += 1,
                Event::End(TagEnd::CodeBlock) => code -= 1,
                Event::Text(_) | Event::SoftBreak if code == 0 => {
                    run.push(item);
                    continue;
                }
                _ => {}
            }
            self.replace_in(std::mem::take(&mut run), &mut out);
            out.push(item);
        }
        self.replace_in(run, &mut out);
        *items = out;
    }

    /// Pushes `run`, text and soft breaks, to `out`, each use of an
    /// abbreviation in it in an `<abbr>`.
    fn replace_in<'i>(&self, run: Vec<Item<'i>>, out: &mut Vec<Item<'i>>)
    where
        'a: 'i,
    {
        if run.is_empty() {
            return;
        }
        let mut text = String::new();
        let mut starts = Vec::with_capacity(run.len());
        for item in &run {
            starts.push(text.len());
            match &item.event {
                Event::Text(t) => text.push_str(t),
                _ => text.push('\n'),
            }
        }
        let uses = self.uses(&text);
        if uses.is_empty() {
            out.extend(run);
            return;
        }
        // Where an `<abbr>` opens or closes, in order, with the tag.
        let mut tags: Vec<(usize, Option<&Abbreviation>)> = Vec::new();
        for (start, end, abbreviation) in uses {
            tags.push((start, Some(abbreviation)));
            tags.push((end, None));
        }
        let mut tags = tags.into_iter().peekable();
        let mut last = 0..0;
        for (item, start) in run.into_iter().zip(starts) {
            last = item.range.clone();
            let len = match &item.event {
                Event::Text(t) => t.len(),
                _ => 1,
            };
            let end = start + len;
            let mut from = start;
            // The tags inside the item, or at its start.
            while let Some(&(at, abbreviation)) = tags.peek().filter(|(at, _)| *at < end) {
                if let Event::Text(t) = &item.event
                    && at > from
                {
                    out.push(Self::text(&item, t, from - start..at - start));
                }
                let tag = match abbreviation {
                    Some(abbreviation) => CowStr::from(abbreviation.tag()),
                    None => CowStr::Borrowed("</abbr>"),
                };
                out.push(Item::new(Event::InlineHtml(tag), item.range.clone()));
                from = at;
                tags.next();
            }
            match &item.event {
                Event::Text(t) if from < end => {
                    out.push(Self::text(&item, t, from - start..len));
                }
                Event::Text(_) => {}
                _ => out.push(item),
            }
        }
        // The tags at the end.
        for (_, abbreviation) in tags {
            debug_assert!(abbreviation.is_none(), "an <abbr> opened at the end");
            out.push(Item::new(
                Event::InlineHtml(CowStr::Borrowed("</abbr>")),
                last.clone(),
            ));
        }
    }

    /// The part `part` of `item`'s text `t`.
    fn text<'i>(item: &Item<'i>, t: &CowStr<'i>, part: std::ops::Range<usize>) -> Item<'i> {
        let text = match t {
            CowStr::Borrowed(t) => CowStr::Borrowed(&t[part]),
            t => CowStr::from(t[part].to_owned()),
        };
        Item::new(Event::Text(text), item.range.clone())
    }

    /// Where each use of an abbreviation in `text` is, and which.
    fn uses(&self, text: &str) -> Vec<(usize, usize, &'s Abbreviation<'a>)> {
        let mut uses = Vec::new();
        let mut at = 0;
        while at < text.len() {
            let after_word = text[..at].chars().next_back().is_some_and(Self::word);
            let found = (!after_word)
                .then(|| {
                    self.defined.iter().find_map(|abbreviation| {
                        let len = Self::matches(&text[at..], abbreviation.abbreviation)?;
                        let before_word = text[at + len..].chars().next().is_some_and(Self::word);
                        (!before_word).then_some((len, *abbreviation))
                    })
                })
                .flatten();
            match found {
                Some((len, abbreviation)) => {
                    uses.push((at, at + len, abbreviation));
                    at += len;
                }
                None => at += text[at..].chars().next().map_or(1, char::len_utf8),
            }
        }
        uses
    }

    /// How long the match for `abbreviation` at the start of `text` is, if
    /// it matches, its whitespace matching any whitespace.
    fn matches(text: &str, abbreviation: &str) -> Option<usize> {
        let mut rest = text;
        for (n, word) in abbreviation.split_whitespace().enumerate() {
            if n > 0 {
                let spaced = rest.trim_start();
                if spaced.len() == rest.len() {
                    return None;
                }
                rest = spaced;
            }
            rest = rest.strip_prefix(word)?;
        }
        Some(text.len() - rest.len())
    }

    /// Whether `c` is a word character to Ruby's `\w`.
    fn word(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }
}

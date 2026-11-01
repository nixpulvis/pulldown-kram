//! kramdown's attribute lists, like `{: .new #intro lang="en"}`.

use std::borrow::Cow;

/// The id, classes and other attributes kramdown's attribute lists give an
/// element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attributes<'a> {
    pub id: Option<Cow<'a, str>>,
    pub classes: Vec<Cow<'a, str>>,
    /// Every other attribute, like `hreflang="de"`, in the order given.
    pub pairs: Vec<(&'a str, Cow<'a, str>)>,
}

impl<'a> Attributes<'a> {
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.classes.is_empty() && self.pairs.is_empty()
    }

    pub fn has_class(&self, class: &str) -> bool {
        self.classes.iter().any(|c| c == class)
    }

    /// Adds `other`'s attributes, which win over any of the same name.
    pub(crate) fn extend(&mut self, other: Self) {
        if other.id.is_some() {
            self.id = other.id;
        }
        self.classes.extend(other.classes);
        for (key, value) in other.pairs {
            self.set(key, value);
        }
    }

    fn set(&mut self, key: &'a str, value: Cow<'a, str>) {
        match key {
            "id" => self.id = Some(value),
            "class" => self.classes = words(value),
            _ => match self.pairs.iter_mut().find(|(k, _)| *k == key) {
                Some(pair) => pair.1 = value,
                None => self.pairs.push((key, value)),
            },
        }
    }
}

/// What one or more attribute lists in a row, like `{:#id}{:.new}`, hold.
#[derive(Debug, Default)]
pub(crate) struct Lists<'a> {
    pub attributes: Attributes<'a>,
    /// Whether one names `toc`, asking for a table of contents.
    pub toc: bool,
    /// How many bytes they take.
    pub len: usize,
}

impl<'a> Lists<'a> {
    /// The lists at the start of `text`, if it starts with one.
    /// Definitions, `{:name: …}`, and extension tags, `{::name}` and
    /// `{:/name}`, are read but give nothing, as neither is supported.
    pub fn parse(text: &'a str) -> Option<Self> {
        let mut lists = Self::default();
        let mut rest = text;
        while let Some(inside) = rest.strip_prefix("{:") {
            let Some(end) = close(inside) else { break };
            lists.read(&inside[..end]);
            let len = "{:".len() + end + "}".len();
            lists.len += len;
            rest = &rest[len..];
        }
        (lists.len > 0).then_some(lists)
    }

    fn read(&mut self, list: &'a str) {
        if list.starts_with([':', '/']) || is_definition(list) {
            return;
        }
        let mut rest = list.trim_start();
        while !rest.is_empty() {
            if let Some((key, value, after)) = pair(rest) {
                self.attributes.set(key, value);
                rest = after;
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                let (word, after) = rest.split_at(end);
                if word.starts_with(['#', '.']) {
                    self.ids_and_classes(word);
                } else if word == "toc" {
                    self.toc = true;
                }
                rest = after;
            }
            rest = rest.trim_start();
        }
    }

    /// Reads a word like `#intro.new.wide`, or nothing if any part of it
    /// isn't an id or class.
    fn ids_and_classes(&mut self, word: &'a str) {
        let mut parts = Vec::new();
        let mut rest = word;
        while let Some(marker) = rest.chars().next() {
            let name = &rest[1..];
            let len = match marker {
                '#' => {
                    let len = name
                        .find(|c: char| !(is_word(c) || matches!(c, ':' | '-')))
                        .unwrap_or(name.len());
                    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
                        return;
                    }
                    len
                }
                '.' => name.find(['.', '#']).unwrap_or(name.len()),
                _ => return,
            };
            if len == 0 {
                return;
            }
            parts.push((marker, &name[..len]));
            rest = &name[len..];
        }
        for (marker, name) in parts {
            if marker == '#' {
                self.attributes.id = Some(Cow::Borrowed(name));
            } else {
                self.attributes.classes.push(Cow::Borrowed(name));
            }
        }
    }
}

/// Where the `}` closing a list is, skipping escaped ones, if it's on the
/// same line.
fn close(list: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in list.char_indices() {
        match c {
            '\n' => return None,
            '}' if !escaped => return Some(i),
            '\\' => {
                escaped = !escaped;
                continue;
            }
            _ => {}
        }
        escaped = false;
    }
    None
}

/// Whether `list` defines attributes for later, like `{:name: .new}`.
fn is_definition(list: &str) -> bool {
    let len = name_len(list);
    len > 0 && list[len..].starts_with(':')
}

/// Reads `key="value"` or `key='value'` from the start of `text`, returning
/// the key, the value without its escapes, and the rest.
fn pair(text: &str) -> Option<(&str, Cow<'_, str>, &str)> {
    let len = name_len(text);
    let (key, rest) = text.split_at(len);
    let rest = rest.strip_prefix('=')?;
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let rest = &rest[1..];
    let mut escaped = false;
    let end = rest.char_indices().find_map(|(i, c)| {
        let end = c == quote && !escaped;
        escaped = c == '\\' && !escaped;
        end.then_some(i)
    })?;
    if len == 0 {
        return None;
    }
    let value = &rest[..end];
    let value = if value.contains('\\') {
        let mut unescaped = String::with_capacity(value.len());
        let mut chars = value.chars().peekable();
        while let Some(c) = chars.next() {
            match chars.peek() {
                Some(&next) if c == '\\' && (next == '}' || next == quote) => {}
                _ => unescaped.push(c),
            }
        }
        Cow::Owned(unescaped)
    } else {
        Cow::Borrowed(value)
    };
    Some((key, value, &rest[end + 1..]))
}

/// The length of the name at the start of `text`, a word character and then
/// word characters or dashes.
fn name_len(text: &str) -> usize {
    if !text.starts_with(is_word) {
        return 0;
    }
    text.find(|c: char| !(is_word(c) || c == '-'))
        .unwrap_or(text.len())
}

/// Ruby's `\w`, which is ASCII only.
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The words of a `class="…"` value.
fn words(value: Cow<'_, str>) -> Vec<Cow<'_, str>> {
    match value {
        Cow::Borrowed(value) => value.split_whitespace().map(Cow::Borrowed).collect(),
        Cow::Owned(value) => value
            .split_whitespace()
            .map(|w| Cow::Owned(w.to_owned()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ids_classes_and_pairs() {
        let text = r#"{:.new}{: .old.x #id k="v \" \}" j='w'} rest"#;
        let lists = Lists::parse(text).unwrap();
        let attributes = lists.attributes;
        assert_eq!(attributes.classes, ["new", "old", "x"]);
        assert_eq!(attributes.id.as_deref(), Some("id"));
        assert_eq!(
            attributes.pairs,
            [("k", Cow::from("v \" }")), ("j", Cow::from("w"))]
        );
        assert_eq!(&text[lists.len..], " rest");
    }

    #[test]
    fn later_attributes_win() {
        let attributes = Lists::parse(r#"{: .a class="b c" .d id="x" #y k="1" k="2"}"#)
            .unwrap()
            .attributes;
        assert_eq!(attributes.classes, ["b", "c", "d"]);
        assert_eq!(attributes.id.as_deref(), Some("y"));
        assert_eq!(attributes.pairs, [("k", Cow::from("2"))]);
    }

    #[test]
    fn names_toc() {
        assert!(Lists::parse("{:toc}").unwrap().toc);
        assert!(!Lists::parse("{:.toc}").unwrap().toc);
    }

    #[test]
    fn definitions_and_extensions_give_nothing() {
        for list in ["{:ref: .x}", "{::comment}", "{:/comment}", "{::}"] {
            let lists = Lists::parse(list).unwrap();
            assert!(lists.attributes.is_empty(), "{list}");
            assert_eq!(lists.len, list.len());
        }
    }

    #[test]
    fn skips_what_isnt_an_id_or_class() {
        let attributes = Lists::parse("{: #1x .a#  bare k=v}").unwrap().attributes;
        assert_eq!(attributes, Attributes::default());
    }

    #[test]
    fn needs_a_closing_brace_on_the_line() {
        assert!(Lists::parse("{:.a").is_none());
        assert!(Lists::parse("{:.a\n}").is_none());
        assert!(Lists::parse("text {:.a}").is_none());
    }
}

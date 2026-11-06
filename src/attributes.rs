//! kramdown's attribute lists, like `{: .new #intro lang="en"}`.

use pulldown_cmark::CowStr;

/// The id, classes and other attributes that kramdown's attribute lists
/// assign to an element.
///
/// ```
/// use pulldown_kram::{Document, Extensions};
///
/// let markdown = "A paragraph.\n{: #intro .note lang=\"en\"}\n";
/// let document = Document::new(markdown, Extensions::all());
/// let (_, attributes) = document.events().next().unwrap();
///
/// assert_eq!(attributes.id.as_deref(), Some("intro"));
/// assert!(attributes.has_class("note"));
/// assert_eq!(attributes.pairs, [("lang".into(), "en".into())]);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attributes<'a> {
    /// The id, from `#intro` or `id="intro"`. If several are given, the last
    /// one wins.
    pub id: Option<CowStr<'a>>,
    /// The classes, from `.new` or `class="new"`, in order. As in kramdown,
    /// `class="…"` replaces any classes before it in the same list.
    pub classes: Vec<CowStr<'a>>,
    /// All other attributes, like `hreflang="de"`, in order.
    pub pairs: Vec<(CowStr<'a>, CowStr<'a>)>,
}

impl<'a> Attributes<'a> {
    /// Returns `true` if there's no id, class or other attribute.
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.classes.is_empty() && self.pairs.is_empty()
    }

    /// Returns `true` if `class` is one of the classes.
    pub fn has_class(&self, class: &str) -> bool {
        self.classes.iter().any(|c| c.as_ref() == class)
    }

    /// Converts these attributes into owned ones, which can outlive the
    /// document they were borrowed from.
    ///
    /// ```
    /// use pulldown_kram::{Attributes, Document, Extensions};
    ///
    /// let attributes: Attributes<'static> = {
    ///     let markdown = String::from("a\n{:.x}\n");
    ///     let document = Document::new(&markdown, Extensions::all());
    ///     let (_, attributes) = document.events().next().unwrap();
    ///     attributes.into_static()
    /// };
    /// assert!(attributes.has_class("x"));
    /// ```
    pub fn into_static(self) -> Attributes<'static> {
        Attributes {
            id: self.id.map(CowStr::into_static),
            classes: self.classes.into_iter().map(CowStr::into_static).collect(),
            pairs: self
                .pairs
                .into_iter()
                .map(|(key, value)| (key.into_static(), value.into_static()))
                .collect(),
        }
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

    fn set(&mut self, key: CowStr<'a>, value: CowStr<'a>) {
        match key.as_ref() {
            "id" => self.id = Some(value),
            "class" => self.classes = Self::split_classes(value),
            _ => match self.pairs.iter_mut().find(|(k, _)| *k == key) {
                Some(pair) => pair.1 = value,
                None => self.pairs.push((key, value)),
            },
        }
    }

    /// The classes in a `class="…"` value.
    fn split_classes(value: CowStr<'a>) -> Vec<CowStr<'a>> {
        match value {
            CowStr::Borrowed(value) => value.split_whitespace().map(CowStr::Borrowed).collect(),
            value => value
                .split_whitespace()
                .map(|word| CowStr::from(word.to_owned()))
                .collect(),
        }
    }
}

/// What one or more attribute lists in a row, like `{:#id}{:.new}`, hold.
#[derive(Debug, Clone, Default)]
pub(crate) struct AttributeLists<'a> {
    pub attributes: Attributes<'a>,
    /// Whether one names `toc`, asking for a table of contents.
    pub toc: bool,
}

impl<'a> AttributeLists<'a> {
    /// The lists at the start of `text`, if it starts with one, and how
    /// many bytes they take.
    ///
    /// Attribute list definitions, `{:name: …}`, are read but add nothing,
    /// as they aren't supported. Neither do the tags of kramdown's
    /// extensions, `{::name}` and `{:/name}`, whose supported uses the
    /// rewrites have already taken out. The tag of an extension kramdown
    /// doesn't have isn't a list, and stays text, as in kramdown.
    pub fn parse(text: &'a str) -> Option<(Self, usize)> {
        let mut attribute_lists = Self::default();
        let mut rest = text;
        while let Some((list, after)) = List::first(rest) {
            if list.is_unknown_extension() {
                break;
            }
            attribute_lists.read(list);
            rest = after;
        }
        let len = text.len() - rest.len();
        (len > 0).then_some((attribute_lists, len))
    }

    /// The lists `text` is made of, if it's nothing else.
    pub fn whole(text: &'a str) -> Option<Self> {
        Self::parse(text)
            .filter(|(_, len)| *len == text.len())
            .map(|(attribute_lists, _)| attribute_lists)
    }

    /// Adds `other`'s attributes, which win over any of the same name.
    pub fn extend(&mut self, other: Self) {
        self.attributes.extend(other.attributes);
        self.toc |= other.toc;
    }

    fn read(&mut self, list: List<'a>) {
        if list.gives_nothing() {
            return;
        }
        for part in list.parts() {
            match part {
                Part::Pair(key, value) => self.attributes.set(CowStr::Borrowed(key), value),
                Part::Word(word) if word.starts_with(['#', '.']) => self.ids_and_classes(word),
                Part::Word("toc") => self.toc = true,
                Part::Word(_) => {}
            }
        }
    }

    /// Reads a word like `#intro.new.wide`, or nothing if any part of it
    /// isn't an id or class.
    fn ids_and_classes(&mut self, word: &'a str) {
        if Self::ids_and_classes_in(word).any(|part| part.is_none()) {
            return;
        }
        for (marker, name) in Self::ids_and_classes_in(word).flatten() {
            if marker == '#' {
                self.attributes.id = Some(CowStr::Borrowed(name));
            } else {
                self.attributes.classes.push(CowStr::Borrowed(name));
            }
        }
    }

    /// The ids and classes in a word like `#intro.new`, each as its marker
    /// and name. A part that's neither is `None`, and ends the word.
    fn ids_and_classes_in(word: &'a str) -> impl Iterator<Item = Option<(char, &'a str)>> {
        let mut rest = word;
        std::iter::from_fn(move || {
            let marker = rest.chars().next()?;
            let name = &rest[marker.len_utf8()..];
            let len = match marker {
                '#' if name.starts_with(|c: char| c.is_ascii_alphabetic()) => name
                    .find(|c: char| !(List::is_word(c) || matches!(c, ':' | '-')))
                    .unwrap_or(name.len()),
                '.' => name.find(['.', '#']).unwrap_or(name.len()),
                _ => 0,
            };
            if len == 0 {
                rest = "";
                return Some(None);
            }
            rest = &name[len..];
            Some(Some((marker, &name[..len])))
        })
    }
}

/// The text of one attribute list, between its `{:` and `}`.
#[derive(Clone, Copy)]
struct List<'a>(&'a str);

impl<'a> List<'a> {
    /// The list at the start of `text`, if it starts with one closed on the
    /// same line, and the text after it. A `}` escaped with `\` doesn't
    /// close it.
    fn first(text: &'a str) -> Option<(Self, &'a str)> {
        let inside = text.strip_prefix("{:")?;
        let mut escaped = false;
        for (i, c) in inside.char_indices() {
            match c {
                '\n' => return None,
                '}' if !escaped => return Some((Self(&inside[..i]), &inside[i + 1..])),
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

    /// Whether it's an extension tag, `{::name}` or `{:/name}`, or defines
    /// attributes for later, like `{:name: .new}`.
    fn gives_nothing(self) -> bool {
        let len = Self::name_len(self.0);
        self.0.starts_with([':', '/']) || (len > 0 && self.0[len..].starts_with(':'))
    }

    /// Whether it's the tag of an extension kramdown doesn't have, like
    /// `{::something}` or `{:/something}`.
    fn is_unknown_extension(self) -> bool {
        let Some(tag) = self.0.strip_prefix([':', '/']) else {
            return false;
        };
        let len = Self::name_len(tag);
        len > 0 && !matches!(&tag[..len], "comment" | "nomarkdown" | "options")
    }

    /// Its pairs and words, in order.
    fn parts(self) -> impl Iterator<Item = Part<'a>> {
        let mut rest = self.0;
        std::iter::from_fn(move || {
            rest = rest.trim_start();
            if rest.is_empty() {
                return None;
            }
            let (part, after) = Part::pair(rest).unwrap_or_else(|| {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                (Part::Word(&rest[..end]), &rest[end..])
            });
            rest = after;
            Some(part)
        })
    }

    /// The length of the name at the start of `text`, a word character and
    /// then word characters or dashes.
    fn name_len(text: &str) -> usize {
        if !text.starts_with(Self::is_word) {
            return 0;
        }
        text.find(|c: char| !(Self::is_word(c) || c == '-'))
            .unwrap_or(text.len())
    }

    /// Ruby's `\w`, which is ASCII only.
    fn is_word(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }
}

/// A part of an attribute list.
enum Part<'a> {
    /// `key="value"` or `key='value'`, the value without its escapes.
    Pair(&'a str, CowStr<'a>),
    /// Anything else, up to the next whitespace, like `.new` or `toc`.
    Word(&'a str),
}

impl<'a> Part<'a> {
    /// The pair at the start of `text`, if it starts with one, and the text
    /// after it.
    fn pair(text: &'a str) -> Option<(Self, &'a str)> {
        let len = List::name_len(text);
        if len == 0 {
            return None;
        }
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
            CowStr::from(unescaped)
        } else {
            CowStr::Borrowed(value)
        };
        Some((Self::Pair(key, value), &rest[end + 1..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_lists_parse_reads_ids_classes_and_pairs() {
        let text = r#"{:.new}{: .old.x #id k="v \" \}" j='w'} rest"#;
        let (attribute_lists, len) = AttributeLists::parse(text).unwrap();
        let attributes = attribute_lists.attributes;
        assert_eq!(attributes.classes, ["new".into(), "old".into(), "x".into()]);
        assert_eq!(attributes.id.as_deref(), Some("id"));
        assert_eq!(
            attributes.pairs,
            [
                ("k".into(), CowStr::from("v \" }")),
                ("j".into(), CowStr::from("w"))
            ]
        );
        assert_eq!(&text[len..], " rest");
    }

    #[test]
    fn attribute_lists_parse_later_attributes_win() {
        let attributes = AttributeLists::parse(r#"{: .a class="b c" .d id="x" #y k="1" k="2"}"#)
            .unwrap()
            .0
            .attributes;
        assert_eq!(attributes.classes, ["b".into(), "c".into(), "d".into()]);
        assert_eq!(attributes.id.as_deref(), Some("y"));
        assert_eq!(attributes.pairs, [("k".into(), CowStr::from("2"))]);
    }

    #[test]
    fn attribute_lists_parse_names_toc() {
        assert!(AttributeLists::parse("{:toc}").unwrap().0.toc);
        assert!(!AttributeLists::parse("{:.toc}").unwrap().0.toc);
    }

    #[test]
    fn attribute_lists_parse_definitions_and_extensions_give_nothing() {
        for list in [
            "{:ref: .x}",
            "{::comment}",
            "{:/comment}",
            "{::options a=\"b\" /}",
            "{::}",
        ] {
            let (attribute_lists, len) = AttributeLists::parse(list).unwrap();
            assert!(attribute_lists.attributes.is_empty(), "{list}");
            assert_eq!(len, list.len());
        }
    }

    #[test]
    fn attribute_lists_parse_leaves_unknown_extensions_as_text() {
        for list in ["{::something}", "{::something/}", "{:/something}"] {
            assert!(AttributeLists::parse(list).is_none(), "{list}");
        }
        let (_, len) = AttributeLists::parse("{:.a}{::something}").unwrap();
        assert_eq!(len, "{:.a}".len());
    }

    #[test]
    fn attribute_lists_parse_skips_what_isnt_an_id_or_class() {
        let attributes = AttributeLists::parse("{: #1x .a#  bare k=v}")
            .unwrap()
            .0
            .attributes;
        assert_eq!(attributes, Attributes::default());
    }

    #[test]
    fn attribute_lists_parse_needs_a_closing_brace_on_the_line() {
        assert!(AttributeLists::parse("{:.a").is_none());
        assert!(AttributeLists::parse("{:.a\n}").is_none());
        assert!(AttributeLists::parse("text {:.a}").is_none());
    }

    #[test]
    fn attribute_lists_parse_stops_at_a_list_left_open() {
        let (attribute_lists, len) = AttributeLists::parse("{:.a}{:.b").unwrap();
        assert_eq!(attribute_lists.attributes.classes, ["a".into()]);
        assert_eq!(len, "{:.a}".len());
    }

    #[test]
    fn attribute_lists_parse_unescapes_only_the_quote_and_brace() {
        let attributes = AttributeLists::parse(r#"{:k="a\b\'\}" j='\''}"#)
            .unwrap()
            .0
            .attributes;
        assert_eq!(
            attributes.pairs,
            [
                ("k".into(), CowStr::from(r"a\b\'}")),
                ("j".into(), CowStr::from("'"))
            ]
        );
    }

    #[test]
    fn attribute_lists_parse_ids_start_with_an_ascii_letter_but_classes_need_not() {
        let attributes = AttributeLists::parse("{: #über .ünï}")
            .unwrap()
            .0
            .attributes;
        assert_eq!(attributes.id, None);
        assert_eq!(attributes.classes, ["ünï".into()]);
    }

    #[test]
    fn attribute_lists_extend_keeps_order_and_lets_the_other_win() {
        let parse = |text| AttributeLists::parse(text).unwrap().0;
        let mut attribute_lists = parse(r#"{:#a .x k="1" j="2"}"#);
        attribute_lists.extend(parse(r#"{:toc #b .y k="3"}"#));
        attribute_lists.extend(parse("{:.z}"));
        assert_eq!(attribute_lists.attributes.id.as_deref(), Some("b"));
        assert_eq!(
            attribute_lists.attributes.classes,
            ["x".into(), "y".into(), "z".into()]
        );
        assert_eq!(
            attribute_lists.attributes.pairs,
            [
                ("k".into(), CowStr::from("3")),
                ("j".into(), CowStr::from("2"))
            ]
        );
        assert!(attribute_lists.toc);
    }

    #[test]
    fn attribute_lists_parse_skips_a_word_running_into_a_wide_character() {
        // The id ends at `é`, which is neither an id nor a class.
        let attributes = AttributeLists::parse("{: #aé .b}").unwrap().0.attributes;
        assert_eq!(attributes.id, None);
        assert_eq!(attributes.classes, ["b".into()]);
    }
}

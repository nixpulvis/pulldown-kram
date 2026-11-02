//! kramdown's definition lists, from pulldown-cmark's: each line before the
//! definitions is a term of its own, and a definition's first paragraph is
//! bare unless a blank line comes before the definition.

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::read::Item;

/// The definition lists of a document, read as kramdown reads them.
pub(crate) struct DefinitionLists<'t> {
    text: &'t str,
}

impl<'t> DefinitionLists<'t> {
    pub fn new(text: &'t str) -> Self {
        Self { text }
    }

    /// Rewrites the definition lists in `items`, read from this text.
    pub fn rewrite(&self, items: &mut Vec<Item<'_>>) {
        let mut i = 0;
        while i < items.len() {
            match items[i].event {
                Event::Start(Tag::DefinitionListTitle) => Self::split_terms(items, i),
                Event::Start(Tag::DefinitionListDefinition)
                    if !self.blank_line_before(items[i].range.start) =>
                {
                    Self::unwrap_first_paragraph(items, i);
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Ends the term starting at `title` at each line break in it, and
    /// starts another.
    fn split_terms(items: &mut Vec<Item<'_>>, title: usize) {
        let mut end = Item::end_of(items, title);
        let range = items[title].range.clone();
        let mut depth = 0usize;
        let mut i = title + 1;
        while i < end {
            match items[i].event {
                Event::Start(_) => depth += 1,
                Event::End(_) => depth -= 1,
                Event::SoftBreak if depth == 0 => {
                    let line = items[i].range.clone();
                    items[i] = Item::new(
                        Event::End(TagEnd::DefinitionListTitle),
                        range.start..line.start,
                    );
                    items.insert(
                        i + 1,
                        Item::new(Event::Start(Tag::DefinitionListTitle), line.end..range.end),
                    );
                    i += 1;
                    end += 1;
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Takes the paragraph that starts the definition at `definition` out
    /// of it, giving the definition its attributes.
    fn unwrap_first_paragraph(items: &mut Vec<Item<'_>>, definition: usize) {
        let paragraph = definition + 1;
        if !matches!(
            items.get(paragraph).map(|item| &item.event),
            Some(Event::Start(Tag::Paragraph))
        ) {
            return;
        }
        let end = Item::end_of(items, paragraph);
        let lists = std::mem::take(&mut items[paragraph].lists);
        items[definition].lists.extend(lists);
        items.remove(end);
        items.remove(paragraph);
    }

    /// Whether the line before the one `offset` is on is blank, or blank
    /// but for quote markers.
    fn blank_line_before(&self, offset: usize) -> bool {
        let line = self.text[..offset].rfind('\n').map_or(0, |i| i + 1);
        let Some(before) = self.text[..line].strip_suffix('\n') else {
            return false;
        };
        let start = before.rfind('\n').map_or(0, |i| i + 1);
        before[start..]
            .trim_matches(|c: char| c.is_whitespace() || c == '>')
            .is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::{Options, Parser};

    /// The items `text` reads as, with pulldown-cmark's definition lists.
    fn items(text: &str) -> Vec<Item<'_>> {
        Parser::new_ext(text, Options::ENABLE_DEFINITION_LIST)
            .into_offset_iter()
            .map(|(event, range)| Item::new(event, range))
            .collect()
    }

    /// `items` as HTML-like text.
    fn outline(items: &[Item<'_>]) -> String {
        items
            .iter()
            .map(|item| match &item.event {
                Event::Start(Tag::DefinitionList) => "<dl>",
                Event::End(TagEnd::DefinitionList) => "</dl>",
                Event::Start(Tag::DefinitionListTitle) => "<dt>",
                Event::End(TagEnd::DefinitionListTitle) => "</dt>",
                Event::Start(Tag::DefinitionListDefinition) => "<dd>",
                Event::End(TagEnd::DefinitionListDefinition) => "</dd>",
                Event::Start(Tag::Paragraph) => "<p>",
                Event::End(TagEnd::Paragraph) => "</p>",
                Event::Start(Tag::Emphasis) => "<em>",
                Event::End(TagEnd::Emphasis) => "</em>",
                Event::SoftBreak => "\n",
                Event::Text(text) => text,
                event => panic!("unexpected {event:?}"),
            })
            .collect()
    }

    #[test]
    fn definition_lists_rewrite_splits_terms_only_outside_spans() {
        let text = "a\n*b\nc*\nd\n: e\n";
        let mut items = items(text);
        DefinitionLists::new(text).rewrite(&mut items);
        assert_eq!(
            outline(&items),
            "<dl><dt>a</dt><dt><em>b\nc</em></dt><dt>d</dt><dd>e</dd></dl>"
        );
    }

    #[test]
    fn definition_lists_rewrite_gives_a_bare_definition_its_paragraphs_attributes() {
        // Loose, as pulldown-cmark only gives a loose list's definitions
        // paragraphs.
        for (text, bare) in [("a\n: b\n\n  c\n", true), ("a\n\n: b\n\n  c\n", false)] {
            let mut items = items(text);
            let paragraph = items
                .iter()
                .position(|item| matches!(item.event, Event::Start(Tag::Paragraph)))
                .unwrap();
            items[paragraph].lists.attributes.classes.push("x".into());
            DefinitionLists::new(text).rewrite(&mut items);
            let definition = items
                .iter()
                .find(|item| matches!(item.event, Event::Start(Tag::DefinitionListDefinition)))
                .unwrap();
            assert_eq!(definition.lists.attributes.has_class("x"), bare, "{text:?}");
            let paragraphs = items
                .iter()
                .filter(|item| matches!(item.event, Event::Start(Tag::Paragraph)))
                .count();
            assert_eq!(paragraphs, if bare { 1 } else { 2 }, "{text:?}");
        }
    }

    #[test]
    fn definition_lists_blank_line_before_allows_spaces_and_quote_markers() {
        let lists = DefinitionLists::new("a\n>\n  \nb\nc");
        // The first line has none before it.
        assert!(!lists.blank_line_before(0));
        assert!(!lists.blank_line_before(2));
        assert!(lists.blank_line_before(4));
        assert!(lists.blank_line_before(7));
        // From anywhere on the line.
        assert!(lists.blank_line_before(8));
        assert!(!lists.blank_line_before(9));
    }
}

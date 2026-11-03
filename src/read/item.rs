use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};

use crate::attributes::AttributeLists;

/// An event, where it was read in the text, and what kramdown's attribute
/// lists give it.
#[derive(Debug, Clone)]
pub(crate) struct Item<'a> {
    pub event: Event<'a>,
    pub range: Range<usize>,
    pub attribute_lists: AttributeLists<'a>,
}

impl<'a> Item<'a> {
    pub fn new(event: Event<'a>, range: Range<usize>) -> Self {
        Self {
            event,
            range,
            attribute_lists: AttributeLists::default(),
        }
    }

    /// Gives a code block without a language the one from its `language-`
    /// class, if it has one.
    pub(super) fn take_language_from_classes(&mut self) {
        let Event::Start(Tag::CodeBlock(kind)) = &mut self.event else {
            return;
        };
        if matches!(kind, CodeBlockKind::Fenced(info) if !info.trim().is_empty()) {
            return;
        }
        let language = self
            .attribute_lists
            .attributes
            .classes
            .iter()
            .find_map(|class| {
                let language = class.strip_prefix("language-")?;
                Some(match class {
                    CowStr::Borrowed(class) => CowStr::Borrowed(&class["language-".len()..]),
                    _ => CowStr::from(language.to_owned()),
                })
            });
        if let Some(language) = language {
            *kind = CodeBlockKind::Fenced(language);
        }
    }

    /// Whether `event` is part of a paragraph's text, not a block.
    pub fn inline(event: &Event<'_>) -> bool {
        match event {
            Event::Start(tag) => matches!(
                tag,
                Tag::Emphasis
                    | Tag::Strong
                    | Tag::Strikethrough
                    | Tag::Superscript
                    | Tag::Subscript
                    | Tag::Link { .. }
                    | Tag::Image { .. }
            ),
            Event::End(tag) => matches!(
                tag,
                TagEnd::Emphasis
                    | TagEnd::Strong
                    | TagEnd::Strikethrough
                    | TagEnd::Superscript
                    | TagEnd::Subscript
                    | TagEnd::Link
                    | TagEnd::Image
            ),
            Event::Text(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::TaskListMarker(_) => true,
            Event::Html(_) | Event::Rule => false,
        }
    }

    /// The index of the end of the element starting at `start` in `items`.
    pub fn end_of(items: &[Self], start: usize) -> usize {
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

    /// Gives the lists in `items` the classes kramdown's GFM parser gives
    /// task lists. A list with a task in it gets `task-list`, and each of
    /// its items from the first task on gets `task-list-item`. These come
    /// before any classes from attribute lists.
    pub(super) fn mark_task_lists(items: &mut [Self]) {
        for list in 0..items.len() {
            if !matches!(items[list].event, Event::Start(Tag::List(_))) {
                continue;
            }
            let mut tasks = false;
            let mut depth = 0usize;
            for i in list + 1..Self::end_of(items, list) {
                if depth == 0 && matches!(items[i].event, Event::Start(Tag::Item)) {
                    tasks |= Self::starts_with_task(&items[i + 1..]);
                    if tasks {
                        items[i]
                            .attribute_lists
                            .attributes
                            .classes
                            .insert(0, "task-list-item".into());
                    }
                }
                match items[i].event {
                    Event::Start(_) => depth += 1,
                    Event::End(_) => depth -= 1,
                    _ => {}
                }
            }
            if tasks {
                items[list]
                    .attribute_lists
                    .attributes
                    .classes
                    .insert(0, "task-list".into());
            }
        }
    }

    /// Whether `rest`, the content of an item, starts with a task's box. In
    /// a loose list the box is inside the item's first paragraph.
    pub(super) fn starts_with_task(rest: &[Self]) -> bool {
        let mut events = rest.iter().map(|item| &item.event);
        match events.next() {
            Some(Event::TaskListMarker(_)) => true,
            Some(Event::Start(Tag::Paragraph)) => {
                matches!(events.next(), Some(Event::TaskListMarker(_)))
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_end_of_finds_the_matching_end() {
        let items: Vec<_> = [
            Event::Start(Tag::List(None)),
            Event::Start(Tag::Item),
            Event::Text("a".into()),
            Event::End(TagEnd::Item),
            Event::Start(Tag::Item),
            Event::End(TagEnd::Item),
            Event::End(TagEnd::List(false)),
        ]
        .into_iter()
        .map(|event| Item::new(event, 0..0))
        .collect();
        assert_eq!(Item::end_of(&items, 0), 6);
        assert_eq!(Item::end_of(&items, 1), 3);
        assert_eq!(Item::end_of(&items, 4), 5);
        // Cut off before its end, the element runs to the last item.
        assert_eq!(Item::end_of(&items[..3], 0), 2);
    }

    #[test]
    fn item_take_language_from_classes_when_the_fence_has_none() {
        let language = |kind| {
            let mut item = Item::new(Event::Start(Tag::CodeBlock(kind)), 0..0);
            item.attribute_lists.attributes.classes =
                vec!["x".into(), "language-rb".into(), "language-py".into()];
            item.take_language_from_classes();
            item.event
        };
        let fenced =
            |info: &'static str| Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info.into())));
        assert_eq!(language(CodeBlockKind::Indented), fenced("rb"));
        assert_eq!(language(CodeBlockKind::Fenced(" ".into())), fenced("rb"));
        assert_eq!(language(CodeBlockKind::Fenced("c".into())), fenced("c"));
    }
}

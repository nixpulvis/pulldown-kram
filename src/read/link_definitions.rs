use pulldown_cmark::{Event, LinkType, RefDefs, Tag};

use crate::attributes::AttributeLists;
use crate::text::Lines;

/// The attribute lists written for link definitions, which kramdown gives
/// to the links that use them. They're on the lines right after a
/// definition, or before it, either right before or alone between blocks.
#[derive(Default)]
pub(super) struct LinkDefinitions<'a> {
    /// The attribute lists of each definition that has any, by its label in
    /// lowercase.
    attribute_lists: Vec<(String, AttributeLists<'a>)>,
    /// Where each line of a definition's attribute lists starts, in order.
    lines: Vec<usize>,
}

impl<'a> LinkDefinitions<'a> {
    /// The attribute lists of the link definition `event` refers to, if
    /// it starts a link or image by reference and its definition has any.
    pub(super) fn for_link(&self, event: &Event<'a>) -> Option<AttributeLists<'a>> {
        if self.attribute_lists.is_empty() {
            return None;
        }
        let Event::Start(Tag::Link { link_type, id, .. } | Tag::Image { link_type, id, .. }) =
            event
        else {
            return None;
        };
        if !matches!(
            link_type,
            LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
        ) {
            return None;
        }
        let label = id.to_lowercase();
        self.attribute_lists
            .iter()
            .find(|(other, _)| *other == label)
            .map(|(_, attribute_lists)| attribute_lists.clone())
    }

    /// Whether a line of attribute lists for a link definition starts at
    /// `start`.
    pub(super) fn on_line(&self, start: usize) -> bool {
        self.lines.binary_search(&start).is_ok()
    }

    /// The attribute lists of the link `definitions` parsed from `text`.
    pub(super) fn read(text: &'a str, definitions: &RefDefs<'_>) -> Self {
        let mut spans: Vec<_> = definitions
            .iter()
            .map(|(label, definition)| (label, definition.span.clone()))
            .collect();
        spans.sort_by_key(|(_, span)| span.start);
        let mut found = Self::default();
        // Where the lines claimed by a definition so far end.
        let mut taken = 0;
        for (label, span) in spans {
            let lines = found.lines.len();
            let mut attribute_lists = AttributeLists::default();
            for (start, before) in Self::before(text, span.start, taken) {
                found.lines.push(start);
                attribute_lists.extend(before);
            }
            let mut line = Lines(text).next(span.end.saturating_sub(1));
            while let Some((start, after, end)) = Self::attribute_lists_on(text, line) {
                found.lines.push(start);
                attribute_lists.extend(after);
                line = end;
            }
            taken = taken.max(line);
            if found.lines.len() > lines {
                found
                    .attribute_lists
                    .push((label.to_lowercase(), attribute_lists));
            }
        }
        found
    }

    /// The attribute lists before the definition at `start`, each with where
    /// it starts, first to last. They're on the lines right before it, or
    /// alone between it and the block before. Lines before `taken` belong to
    /// another definition.
    fn before(text: &'a str, start: usize, taken: usize) -> Vec<(usize, AttributeLists<'a>)> {
        let mut attribute_lists = Vec::new();
        let mut above = Lines(text).previous(Lines(text).start(start));
        while let Some(line) = above
            && Lines(text).rest_of_line(line).trim().is_empty()
        {
            above = Lines(text).previous(line);
        }
        while let Some(line) = above
            && line >= taken
            && let Some((start, found, _)) = Self::attribute_lists_on(text, line)
        {
            attribute_lists.push((start, found));
            above = Lines(text).previous(line);
        }
        // Right after another block, they belong to that block.
        if above.is_some_and(|line| !Lines(text).rest_of_line(line).trim().is_empty()) {
            return Vec::new();
        }
        attribute_lists.reverse();
        attribute_lists
    }

    /// The attribute lists alone on the line at `line`, if there are any,
    /// with where they start and where the next line starts.
    fn attribute_lists_on(
        text: &'a str,
        line: usize,
    ) -> Option<(usize, AttributeLists<'a>, usize)> {
        let content = Lines(text).rest_of_line(line);
        let indent = content.len() - content.trim_start().len();
        let attribute_lists = AttributeLists::whole(content.trim()).filter(|_| indent < 4)?;
        Some((line + indent, attribute_lists, Lines(text).next(line)))
    }
}

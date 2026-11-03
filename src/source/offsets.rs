use std::borrow::Cow;
use std::ops::Range;

/// Maps offsets in rewritten text back to the text it was rewritten from.
///
/// Text outside the edits is the same in both, so its offsets only move by
/// what the edits before it put in or took out. A range starting inside an
/// edit's text starts where the text it replaced starts, and one ending
/// inside it ends where that text ends, so a range takes in all of what it
/// covers part of. Text put in where there was none maps to the point it
/// went in at. So an ordered range on char boundaries in the rewritten text
/// maps to an ordered range on char boundaries in the original.
///
/// Text left as it was has no edits, and costs nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Offsets {
    /// The edits, in order, none touching another.
    edits: Vec<Edit>,
}

/// Text put in place of some of the original, or of none of it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    /// Where the new text is in the rewritten text. Never empty.
    rewritten: Range<usize>,
    /// Where the text it replaced was in the original.
    original: Range<usize>,
}

impl Offsets {
    /// Whether nothing was rewritten.
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Where `range`, in the rewritten text, was in the original.
    pub fn original(&self, range: Range<usize>) -> Range<usize> {
        self.to_original(range.start, false)..self.to_original(range.end, true)
    }

    /// These offsets, for their rewritten text since rewritten again with
    /// `later`'s edits.
    pub fn then(self, later: Offsets) -> Offsets {
        if self.edits.is_empty() {
            return later;
        }
        if later.edits.is_empty() {
            return self;
        }
        // What either rewrite changed of the text between them, joined
        // where it overlaps or touches.
        let mut changed: Vec<Range<usize>> = self
            .edits
            .iter()
            .map(|edit| edit.rewritten.clone())
            .chain(later.edits.iter().map(|edit| edit.original.clone()))
            .collect();
        changed.sort_by_key(|span| span.start);
        changed.dedup_by(|span, last| {
            let touches = span.start <= last.end;
            if touches {
                last.end = last.end.max(span.end);
            }
            touches
        });
        // Neither rewrite's edits are partly in a span, so its ends map
        // exactly both ways.
        let edits = changed
            .into_iter()
            .map(|span| Edit {
                rewritten: later.to_rewritten(span.start, false)
                    ..later.to_rewritten(span.end, true),
                original: self.to_original(span.start, false)..self.to_original(span.end, true),
            })
            .collect();
        Offsets { edits }
    }

    /// Where `at`, in the rewritten text, was in the original. Inside an
    /// edit's text, that's where the text it replaced ends, if `at` ends a
    /// range, or else where it starts.
    fn to_original(&self, at: usize, end: bool) -> usize {
        let before = self.edits.partition_point(|edit| edit.rewritten.start < at);
        let Some(edit) = before.checked_sub(1).map(|i| &self.edits[i]) else {
            return at;
        };
        if at < edit.rewritten.end {
            return if end {
                edit.original.end
            } else {
                edit.original.start
            };
        }
        edit.original.end + (at - edit.rewritten.end)
    }

    /// Where `at`, in the original and not inside a replaced text, is in
    /// the rewritten text. Text put in at `at` comes before it if `at` ends
    /// a range, and after it otherwise.
    fn to_rewritten(&self, at: usize, end: bool) -> usize {
        let before = self
            .edits
            .partition_point(|edit| edit.original.start < at || end && edit.original.start == at);
        match before.checked_sub(1).map(|i| &self.edits[i]) {
            Some(edit) => edit.rewritten.end + (at - edit.original.end),
            None => at,
        }
    }

    /// Where `at`, in the original and not inside a replaced text, is in
    /// the rewritten text, after any text put in at it.
    pub fn moved(&self, at: usize) -> usize {
        self.to_rewritten(at, true)
    }

    /// Records `edit`, made after the others, joined onto the last if they
    /// touch.
    fn push(&mut self, edit: Edit) {
        debug_assert!(!edit.rewritten.is_empty());
        match self.edits.last_mut() {
            // Text between edits is the same length in both, so they touch
            // in the original exactly when they do in the rewritten text.
            Some(last) if last.rewritten.end == edit.rewritten.start => {
                last.rewritten.end = edit.rewritten.end;
                last.original.end = edit.original.end;
            }
            _ => self.edits.push(edit),
        }
    }

    /// Whether `range`, in the original, overlaps or touches text that was
    /// replaced, or a point text was put in at.
    #[cfg(test)]
    pub fn touches(&self, range: &Range<usize>) -> bool {
        self.edits
            .iter()
            .any(|edit| edit.original.start <= range.end && range.start <= edit.original.end)
    }

    /// What the first `needle` in `rewritten` was in `src`.
    #[cfg(test)]
    pub fn was<'s>(&self, src: &'s str, rewritten: &str, needle: &str) -> &'s str {
        let start = rewritten
            .find(needle)
            .expect("needle in the rewritten text");
        &src[self.original(start..start + needle.len())]
    }

    /// Checks every range on char boundaries in `rewritten` maps to an
    /// ordered range on char boundaries in `src`.
    #[cfg(test)]
    pub fn assert_fit(&self, src: &str, rewritten: &str) {
        let boundaries: Vec<usize> = (0..=rewritten.len())
            .filter(|&i| rewritten.is_char_boundary(i))
            .collect();
        for (i, &start) in boundaries.iter().enumerate() {
            for &end in &boundaries[i..] {
                let range = self.original(start..end);
                assert!(
                    src.get(range.clone()).is_some(),
                    "{start}..{end} maps to {range:?} in {src:?}"
                );
            }
        }
        assert_eq!(self.original(0..rewritten.len()), 0..src.len());
    }
}

/// Text being rewritten from the start, with its edits.
#[derive(Default)]
pub(super) struct Rewrite {
    pub(super) out: String,
    offsets: Offsets,
    /// How much of the original has been read.
    pub(super) read: usize,
}

impl Rewrite {
    /// A rewrite of `original`, with nothing read yet.
    pub(super) fn of(original: &str) -> Self {
        Self {
            // Room for the few lines the rewrites put in.
            out: String::with_capacity(original.len() + 64),
            ..Self::default()
        }
    }

    /// Keeps `text`, the original's next.
    pub(super) fn keep(&mut self, text: &str) {
        self.out.push_str(text);
        self.read += text.len();
    }

    /// Puts `text` in, in place of none of the original.
    pub(super) fn insert(&mut self, text: &str) {
        self.replace(0, text);
    }

    /// Puts `text` in place of the original's next `len` bytes.
    pub(super) fn replace(&mut self, len: usize, text: &str) {
        let start = self.out.len();
        self.out.push_str(text);
        self.offsets.push(Edit {
            rewritten: start..self.out.len(),
            original: self.read..self.read + len,
        });
        self.read += len;
    }

    /// The rewritten text, just `original` if nothing changed, and its
    /// offsets.
    pub(super) fn finish(self, original: &str) -> (Cow<'_, str>, Offsets) {
        debug_assert_eq!(self.read, original.len());
        if self.offsets.is_empty() {
            (Cow::Borrowed(original), Offsets::default())
        } else {
            (Cow::Owned(self.out), self.offsets)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{MarkdownElements, SpanTags};

    /// `src` rewritten by two rewrites, and its offsets.
    fn rewrite(src: &str) -> (String, Offsets) {
        let (separated, first) = MarkdownElements::separate(src);
        let (joined, second) = SpanTags::join(&separated);
        (joined.into_owned(), first.then(second))
    }

    #[test]
    fn offsets_original_skips_added_blank_lines() {
        let src = "<div markdown=\"1\">\nä *ö*\n</div>\nü\n";
        let (text, offsets) = MarkdownElements::separate(src);
        assert_eq!(text, "<div markdown=\"1\">\n\nä *ö*\n\n</div>\nü\n");
        for needle in ["<div markdown=\"1\">\n", "ä *ö*\n", "</div>\n", "ü\n"] {
            assert_eq!(offsets.was(src, &text, needle), needle);
        }
        // An added line maps to the point it went in at, starting or ending
        // a range.
        let open = "<div markdown=\"1\">\n".len();
        assert_eq!(offsets.original(open..open + 1), open..open);
        assert_eq!(offsets.was(src, &text, "\n\nä"), "\nä");
        assert_eq!(offsets.was(src, &text, "*\n\n"), "*\n");
        offsets.assert_fit(src, &text);
    }

    #[test]
    fn offsets_original_takes_in_joined_line_ends() {
        let src = "<img alt=\"é\" />  \r\nä\n<b>\nö\n";
        let (text, offsets) = SpanTags::join(src);
        assert_eq!(text, "<img alt=\"é\" /> ä\n<b> ö\n");
        for needle in ["<img alt=\"é\" />", "ä\n", "<b>", "ö\n"] {
            assert_eq!(offsets.was(src, &text, needle), needle);
        }
        // The space in place of a line's end maps to all of it.
        assert_eq!(offsets.was(src, &text, "/> ä"), "/>  \r\nä");
        assert_eq!(offsets.was(src, &text, " ö"), "\nö");
        offsets.assert_fit(src, &text);
    }

    #[test]
    fn offsets_then_follows_both_rewrites() {
        let src = "<div markdown=\"1\">\n<b>\nü *x*\n</div>\n<i>  \ny\n";
        let (text, offsets) = rewrite(src);
        assert_eq!(text, "<div markdown=\"1\">\n\n<b> ü *x*\n\n</div>\n<i> y\n");
        for needle in [
            "<div markdown=\"1\">\n",
            "<b>",
            "ü *x*\n",
            "</div>\n",
            "<i>",
            "y\n",
        ] {
            assert_eq!(offsets.was(src, &text, needle), needle);
        }
        assert_eq!(offsets.was(src, &text, "\n\n<b> ü"), "\n<b>\nü");
        assert_eq!(offsets.was(src, &text, "<i> y"), "<i>  \ny");
        offsets.assert_fit(src, &text);
    }

    #[test]
    fn offsets_then_lets_later_edits_replace_earlier_ones() {
        let src = "ab\ncd\n";
        let mut first = Rewrite::default();
        first.keep("ab");
        first.insert("XY");
        first.keep("\ncd\n");
        let (middle, first) = first.finish(src);
        assert_eq!(middle, "abXY\ncd\n");
        // Replaces some of what the first put in, and some of the original.
        let mut second = Rewrite::default();
        second.keep("abX");
        second.replace(2, "-");
        second.keep("cd\n");
        let (text, second) = second.finish(&middle);
        assert_eq!(text, "abX-cd\n");
        let offsets = first.then(second);
        assert_eq!(offsets.was(src, &text, "ab"), "ab");
        assert_eq!(offsets.was(src, &text, "X-"), "\n");
        assert_eq!(offsets.was(src, &text, "-"), "\n");
        assert_eq!(offsets.was(src, &text, "cd\n"), "cd\n");
        offsets.assert_fit(src, &text);
    }
}

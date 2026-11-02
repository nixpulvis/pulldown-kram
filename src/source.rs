//! Rewrites the few things pulldown-cmark's CommonMark parser reads
//! differently from kramdown, before it reads them, keeping track of where
//! the rewritten text was in the original.

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
}

/// Text being rewritten from the start, with its edits.
#[derive(Default)]
struct Rewrite {
    out: String,
    offsets: Offsets,
    /// How much of the original has been read.
    read: usize,
}

impl Rewrite {
    /// Keeps `text`, the original's next.
    fn keep(&mut self, text: &str) {
        self.out.push_str(text);
        self.read += text.len();
    }

    /// Puts `text` in, in place of none of the original.
    fn insert(&mut self, text: &str) {
        self.replace(0, text);
    }

    /// Puts `text` in place of the original's next `len` bytes.
    fn replace(&mut self, len: usize, text: &str) {
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
    fn finish(self, original: &str) -> (Cow<'_, str>, Offsets) {
        debug_assert_eq!(self.read, original.len());
        if self.offsets.is_empty() {
            (Cow::Borrowed(original), Offsets::default())
        } else {
            (Cow::Owned(self.out), self.offsets)
        }
    }
}

/// kramdown parses Markdown inside an HTML element marked `markdown="1"`.
/// CommonMark only does so when blank lines separate the tags from the
/// Markdown, so this adds them.
pub(crate) struct MarkdownElements {
    out: Rewrite,
    /// The elements still open, innermost last.
    open: Vec<Element>,
    fence: Fence,
}

/// An open `markdown="1"` element.
struct Element {
    name: String,
    /// How many elements of the same name are open inside it.
    depth: usize,
}

impl MarkdownElements {
    /// `markdown`, with blank lines around what's inside each
    /// `markdown="1"` element, and the offsets of the lines added.
    pub fn separate(markdown: &str) -> (Cow<'_, str>, Offsets) {
        if !markdown.contains("markdown=\"1\"") {
            return (Cow::Borrowed(markdown), Offsets::default());
        }
        let mut elements = Self {
            out: Rewrite {
                out: String::with_capacity(markdown.len() + 64),
                ..Rewrite::default()
            },
            open: Vec::new(),
            fence: Fence::default(),
        };
        for line in markdown.split_inclusive('\n') {
            elements.line(line);
        }
        elements.out.finish(markdown)
    }

    fn line(&mut self, line: &str) {
        if self.fence.read(line) {
            self.out.keep(line);
            return;
        }
        let trimmed = line.trim();
        if self.closes(trimmed) {
            self.out.insert("\n");
            self.out.keep(line);
            return;
        }
        self.out.keep(line);
        if let Some(name) = Element::opened_by(trimmed) {
            if !line.ends_with('\n') {
                self.out.insert("\n");
            }
            self.out.insert("\n");
            self.open.push(Element { name, depth: 0 });
        }
    }

    /// Whether `trimmed` closes the innermost open element, keeping count of
    /// elements of the same name opened inside it.
    fn closes(&mut self, trimmed: &str) -> bool {
        let Some(element) = self.open.last_mut() else {
            return false;
        };
        match Tag::starting(trimmed) {
            Some(tag) if tag.name == element.name && tag.closing => {
                if element.depth == 0 {
                    self.open.pop();
                    return true;
                }
                element.depth -= 1;
            }
            // One closed on the same line, like `<div>x</div>`, isn't open.
            Some(tag) if tag.name == element.name && !Self::closed_in(trimmed, tag.name) => {
                element.depth += 1;
            }
            _ => {}
        }
        false
    }

    /// Whether `trimmed` has a closing tag named `name` in it.
    fn closed_in(trimmed: &str, name: &str) -> bool {
        trimmed
            .match_indices("</")
            .any(|(i, _)| trimmed[i + "</".len()..].starts_with(name))
    }
}

impl Element {
    /// The name of the `markdown="1"` element `trimmed` opens, if it's a line
    /// of only that tag.
    fn opened_by(trimmed: &str) -> Option<String> {
        if !(trimmed.ends_with('>') && trimmed.contains("markdown=\"1\"")) {
            return None;
        }
        let tag = Tag::starting(trimmed).filter(|tag| !tag.closing)?;
        Some(tag.name.to_owned())
    }
}

/// The tag at the start of a line.
struct Tag<'a> {
    name: &'a str,
    /// Whether it's a closing tag, like `</div>`.
    closing: bool,
}

impl<'a> Tag<'a> {
    /// The tag `trimmed` starts with, if it starts with one.
    fn starting(trimmed: &'a str) -> Option<Self> {
        let rest = trimmed.strip_prefix('<')?;
        let (rest, closing) = match rest.strip_prefix('/') {
            Some(rest) => (rest, true),
            None => (rest, false),
        };
        let len = rest
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(rest.len());
        (len > 0).then(|| Self {
            name: &rest[..len],
            closing,
        })
    }
}

/// kramdown reads a line of only a span-level tag, like `<img>` or `<span>`,
/// as the start of a paragraph, Markdown on the lines after it included.
/// CommonMark starts an HTML block there instead, which runs to the next
/// blank line and leaves any Markdown in it as text. Joining the next line
/// onto the tag's makes the same paragraph of it.
pub(crate) struct SpanTags;

impl SpanTags {
    /// The span-level elements kramdown keeps in paragraphs.
    const NAMES: [&str; 38] = [
        "a", "abbr", "acronym", "b", "bdo", "big", "br", "button", "cite", "code", "del", "dfn",
        "em", "i", "img", "input", "ins", "kbd", "label", "mark", "option", "q", "rb", "rbc", "rp",
        "rt", "rtc", "ruby", "samp", "select", "small", "span", "strong", "sub", "sup", "textarea",
        "tt", "var",
    ];

    /// `markdown`, with each line of only a span tag joined onto the next
    /// line, when that's text, and the offsets of the joins.
    pub fn join(markdown: &str) -> (Cow<'_, str>, Offsets) {
        let mut out = Rewrite::default();
        let mut fence = Fence::default();
        let mut lines = markdown.split_inclusive('\n').peekable();
        while let Some(line) = lines.next() {
            if fence.read(line) {
                out.keep(line);
                continue;
            }
            let next_is_text = lines.peek().is_some_and(|next| {
                let next = next.trim();
                !next.is_empty() && !next.starts_with("```") && !next.starts_with("~~~")
            });
            if next_is_text && Self::alone(line) {
                let tag = line.trim_end();
                out.keep(tag);
                out.replace(line.len() - tag.len(), " ");
            } else {
                out.keep(line);
            }
        }
        out.finish(markdown)
    }

    /// Whether `line` is only a span-level tag, opening or closing, and not
    /// indented enough to be code.
    fn alone(line: &str) -> bool {
        let indent = line.len() - line.trim_start().len();
        let Some(tag) = line
            .trim()
            .strip_prefix('<')
            .and_then(|tag| tag.strip_suffix('>'))
            .filter(|tag| indent < 4 && !tag.contains(['<', '>']))
        else {
            return false;
        };
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        Self::NAMES.contains(&name.as_str())
    }
}

/// Keeps track of fenced code blocks, where nothing is kramdown.
#[derive(Default)]
struct Fence {
    /// The character and length of the open fence, if inside one.
    open: Option<(char, usize)>,
}

impl Fence {
    /// Reads `line`, returning whether it's part of a fenced code block,
    /// fences included. A fence closes at a line of only as many of its
    /// characters or more.
    fn read(&mut self, line: &str) -> bool {
        let trimmed = line.trim();
        let run = |c: char| trimmed.len() - trimmed.trim_start_matches(c).len();
        if let Some((c, len)) = self.open {
            if run(c) >= len && trimmed.trim_start_matches(c).is_empty() {
                self.open = None;
            }
            return true;
        }
        for c in ['`', '~'] {
            let len = run(c);
            if len >= 3 {
                self.open = Some((c, len));
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `src` rewritten as a document with every extension is, and its
    /// offsets.
    fn rewrite(src: &str) -> (String, Offsets) {
        let (separated, first) = MarkdownElements::separate(src);
        let (joined, second) = SpanTags::join(&separated);
        (joined.into_owned(), first.then(second))
    }

    /// What the first `needle` in `rewritten` was in `src`.
    fn was<'a>(src: &'a str, rewritten: &str, offsets: &Offsets, needle: &str) -> &'a str {
        let start = rewritten
            .find(needle)
            .expect("needle in the rewritten text");
        &src[offsets.original(start..start + needle.len())]
    }

    /// Checks every range on char boundaries in `rewritten` maps to an
    /// ordered range on char boundaries in `src`.
    fn assert_ranges_fit(src: &str, rewritten: &str, offsets: &Offsets) {
        let boundaries: Vec<usize> = (0..=rewritten.len())
            .filter(|&i| rewritten.is_char_boundary(i))
            .collect();
        for (i, &start) in boundaries.iter().enumerate() {
            for &end in &boundaries[i..] {
                let range = offsets.original(start..end);
                assert!(
                    src.get(range.clone()).is_some(),
                    "{start}..{end} maps to {range:?} in {src:?}"
                );
            }
        }
        assert_eq!(offsets.original(0..rewritten.len()), 0..src.len());
    }

    #[test]
    fn separates_markdown_from_tags() {
        let src = "<div markdown=\"1\">\n```rust\n</div>\n```\n<div>\n</div>\n</div>\nafter\n";
        assert_eq!(
            MarkdownElements::separate(src).0,
            "<div markdown=\"1\">\n\n```rust\n</div>\n```\n<div>\n</div>\n\n</div>\nafter\n"
        );
        // Elements closed on their own line, or only alike in their start,
        // don't nest.
        let src = "<p markdown=\"1\">\n<p>x</p>\n<pre>\n</pre>\n</p>\n";
        assert_eq!(
            MarkdownElements::separate(src).0,
            "<p markdown=\"1\">\n\n<p>x</p>\n<pre>\n</pre>\n\n</p>\n"
        );
        // A fence only closes with as many marks.
        let src = "<div markdown=\"1\">\n````\n```\n</div>\n````\n</div>\n";
        assert_eq!(
            MarkdownElements::separate(src).0,
            "<div markdown=\"1\">\n\n````\n```\n</div>\n````\n\n</div>\n"
        );
        // Nothing to separate in a fence.
        let (text, offsets) = MarkdownElements::separate("```\n<div markdown=\"1\">\n```\n");
        assert!(matches!(text, Cow::Borrowed(_)));
        assert!(offsets.is_empty());
    }

    #[test]
    fn span_tags_start_paragraphs() {
        assert_eq!(
            SpanTags::join("<img src=\"a\" />\n![b](c)\n").0,
            "<img src=\"a\" /> ![b](c)\n"
        );
        assert_eq!(
            SpanTags::join("<span class=\"g\">\n![a](b)\n![c](d)\n</span>\n").0,
            "<span class=\"g\"> ![a](b)\n![c](d)\n</span>\n"
        );
        // Not block elements, tags before a blank line or a fence, or code.
        let src = "<div>\n*a*\n</div>\n\n<img />\n\n<b>\n```\n<b>\nx\n```\n    <b>\n    x\n";
        let (text, offsets) = SpanTags::join(src);
        assert!(matches!(text, Cow::Borrowed(_)));
        assert!(offsets.is_empty());
    }

    #[test]
    fn span_tags_ignore_case_and_may_close() {
        assert_eq!(SpanTags::join("<IMG src=x>\na\n").0, "<IMG src=x> a\n");
        assert_eq!(SpanTags::join("</span>\na\n").0, "</span> a\n");
        // Not two tags on a line.
        assert!(matches!(
            SpanTags::join("<span><b>\na\n").0,
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn a_markdown_element_ending_the_input_gets_its_blank_line() {
        let src = "<div markdown=\"1\">";
        let (text, offsets) = MarkdownElements::separate(src);
        assert_eq!(text, "<div markdown=\"1\">\n\n");
        // The added lines are where the input ends.
        assert_eq!(
            offsets.original(src.len()..text.len()),
            src.len()..src.len()
        );
        assert_ranges_fit(src, &text, &offsets);
    }

    #[test]
    fn offsets_skip_added_blank_lines() {
        let src = "<div markdown=\"1\">\nä *ö*\n</div>\nü\n";
        let (text, offsets) = MarkdownElements::separate(src);
        assert_eq!(text, "<div markdown=\"1\">\n\nä *ö*\n\n</div>\nü\n");
        for needle in ["<div markdown=\"1\">\n", "ä *ö*\n", "</div>\n", "ü\n"] {
            assert_eq!(was(src, &text, &offsets, needle), needle);
        }
        // An added line maps to the point it went in at, starting or ending
        // a range.
        let open = "<div markdown=\"1\">\n".len();
        assert_eq!(offsets.original(open..open + 1), open..open);
        assert_eq!(was(src, &text, &offsets, "\n\nä"), "\nä");
        assert_eq!(was(src, &text, &offsets, "*\n\n"), "*\n");
        assert_ranges_fit(src, &text, &offsets);
    }

    #[test]
    fn offsets_take_in_joined_line_ends() {
        let src = "<img alt=\"é\" />  \r\nä\n<b>\nö\n";
        let (text, offsets) = SpanTags::join(src);
        assert_eq!(text, "<img alt=\"é\" /> ä\n<b> ö\n");
        for needle in ["<img alt=\"é\" />", "ä\n", "<b>", "ö\n"] {
            assert_eq!(was(src, &text, &offsets, needle), needle);
        }
        // The space in place of a line's end maps to all of it.
        assert_eq!(was(src, &text, &offsets, "/> ä"), "/>  \r\nä");
        assert_eq!(was(src, &text, &offsets, " ö"), "\nö");
        assert_ranges_fit(src, &text, &offsets);
    }

    #[test]
    fn offsets_follow_both_rewrites() {
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
            assert_eq!(was(src, &text, &offsets, needle), needle);
        }
        assert_eq!(was(src, &text, &offsets, "\n\n<b> ü"), "\n<b>\nü");
        assert_eq!(was(src, &text, &offsets, "<i> y"), "<i>  \ny");
        assert_ranges_fit(src, &text, &offsets);
    }

    #[test]
    fn later_edits_may_replace_earlier_ones() {
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
        assert_eq!(was(src, &text, &offsets, "ab"), "ab");
        assert_eq!(was(src, &text, &offsets, "X-"), "\n");
        assert_eq!(was(src, &text, &offsets, "-"), "\n");
        assert_eq!(was(src, &text, &offsets, "cd\n"), "cd\n");
        assert_ranges_fit(src, &text, &offsets);
    }

    #[test]
    fn fences_close_only_with_their_own_marks() {
        let mut fence = Fence::default();
        assert!(!fence.read("a\n"));
        assert!(fence.read("~~~ruby\n"));
        assert!(fence.read("```\n"));
        // Not with more after the marks.
        assert!(fence.read("~~~ x\n"));
        assert!(fence.read("~~~~\n"));
        assert!(!fence.read("b\n"));
        // Not with fewer marks.
        assert!(fence.read("````\n"));
        assert!(fence.read("```\n"));
        assert!(fence.read("````\n"));
        assert!(!fence.read("c\n"));
    }
}

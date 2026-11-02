//! Rewrites the few things pulldown-cmark's CommonMark parser reads
//! differently from kramdown, before it reads them, keeping track of where
//! the rewritten text was in the original.

use std::borrow::Cow;
use std::ops::Range;

use crate::Extensions;
use crate::abbreviations::Abbreviation;
use crate::attributes::Lists;

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

    /// `html` without the `markdown="1"` in its tags, which kramdown reads
    /// and leaves out, or `None` if there's none to leave out.
    pub fn without_attribute(html: &str) -> Option<String> {
        const ATTRIBUTE: &str = "markdown=\"1\"";
        let mut out = String::new();
        let mut kept = 0;
        for (i, _) in html.match_indices(ATTRIBUTE) {
            let before = &html[..i];
            let in_tag = before.rfind('<') > before.rfind('>');
            let start = before
                .trim_end_matches(|c: char| c.is_ascii_whitespace())
                .len();
            if in_tag && start < i {
                out.push_str(&html[kept..start]);
                kept = i + ATTRIBUTE.len();
            }
        }
        if kept == 0 {
            return None;
        }
        out.push_str(&html[kept..]);
        Some(out)
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

/// kramdown reads a line of only attribute lists as a block of its own,
/// ending the one before it. CommonMark reads a line indented as code, or
/// a link definition, right after it as more of the paragraph the lists
/// are in, so this puts a blank line between them, outside lists, quotes
/// and HTML, where one would end more than the paragraph.
pub(crate) struct AttributeLines;

impl AttributeLines {
    /// `markdown`, with a blank line after each unindented line of only
    /// attribute lists that code or a link definition comes right after,
    /// and the offsets of the lines added.
    pub fn separate(markdown: &str) -> (Cow<'_, str>, Offsets) {
        if !markdown.contains("{:") {
            return (Cow::Borrowed(markdown), Offsets::default());
        }
        let mut out = Rewrite {
            out: String::with_capacity(markdown.len() + 16),
            ..Rewrite::default()
        };
        let mut fence = Fence::default();
        // Whether the lines since the last blank one may be in a list, a
        // quote or HTML.
        let mut contained = false;
        let mut after_blank = true;
        let mut lines = markdown.split_inclusive('\n').peekable();
        while let Some(line) = lines.next() {
            out.keep(line);
            if fence.read(line) {
                after_blank = false;
                continue;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                after_blank = true;
                continue;
            }
            let indent = Self::indent(line);
            if after_blank {
                contained = (1..4).contains(&indent)
                    || indent == 0 && (trimmed.starts_with(['<', '>']) || Self::item(trimmed));
                after_blank = false;
            }
            let only_lists = || Lists::parse(trimmed).is_some_and(|(_, len)| len == trimmed.len());
            let block_next = lines.peek().is_some_and(|next| {
                let indent = Self::indent(next);
                let next = next.trim();
                !next.is_empty() && (indent >= 4 || Self::link_definition(next))
            });
            if !contained && indent == 0 && block_next && only_lists() {
                out.insert("\n");
            }
        }
        out.finish(markdown)
    }

    /// Whether `trimmed` looks like the start of a link definition, like
    /// `[a]: b`, but not a footnote's.
    fn link_definition(trimmed: &str) -> bool {
        trimmed
            .strip_prefix('[')
            .filter(|label| !label.starts_with('^'))
            .and_then(|label| label.find(']'))
            .is_some_and(|end| trimmed[end + 2..].starts_with(':'))
    }

    /// Whether `trimmed` starts a list item, like `* a` or `1. a`.
    fn item(trimmed: &str) -> bool {
        let digits = trimmed.trim_start_matches(|c: char| c.is_ascii_digit());
        let rest = if digits.len() < trimmed.len() {
            digits.strip_prefix(['.', ')'])
        } else {
            trimmed.strip_prefix(['*', '-', '+'])
        };
        rest.is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
    }

    /// The columns `line` is indented by, with tab stops at multiples of
    /// four.
    fn indent(line: &str) -> usize {
        let mut columns = 0;
        for c in line.chars() {
            match c {
                ' ' => columns += 1,
                '\t' => columns += 4 - columns % 4,
                _ => break,
            }
        }
        columns
    }
}

/// kramdown reads two things about definitions differently from
/// pulldown-cmark, which this rewrites the lines for. A `:` more than one
/// blank line after the terms starts no definition, so it's escaped. And a
/// definition with nothing on its first line has its content indented by
/// four, not two past the `:`, so the lines of it indented by four or more
/// lose the difference.
pub(crate) struct DefinitionLines {
    out: Rewrite,
    fence: Fence,
    /// The blank lines in a row before this one.
    blanks: usize,
    /// Whether the lines are in a definition, which a `:` after blank lines
    /// adds one to.
    in_definition: bool,
    /// For a definition with nothing on its first line, the spaces to take
    /// off its lines, and whether a line of it has been read yet.
    shift: Option<(usize, bool)>,
}

impl DefinitionLines {
    /// `markdown`, with its definitions' lines rewritten as kramdown reads
    /// them, and the offsets of the edits.
    pub fn rewrite(markdown: &str) -> (Cow<'_, str>, Offsets) {
        let starts_definition =
            |line: &str| line.trim_start_matches([' ', '\t', '>']).starts_with(':');
        if !markdown.lines().any(starts_definition) {
            return (Cow::Borrowed(markdown), Offsets::default());
        }
        let mut lines = Self {
            out: Rewrite {
                out: String::with_capacity(markdown.len() + 16),
                ..Rewrite::default()
            },
            fence: Fence::default(),
            blanks: 0,
            in_definition: false,
            shift: None,
        };
        for line in markdown.split_inclusive('\n') {
            lines.line(line);
        }
        lines.out.finish(markdown)
    }

    fn line(&mut self, line: &str) {
        let shift = self.take_shift(line);
        if self.fence.read(line) {
            self.blanks = 0;
            self.keep(line, shift);
            return;
        }
        let text = line.trim_end_matches(['\n', '\r']);
        let lead = text.len() - text.trim_start_matches([' ', '>']).len();
        let rest = &text[lead..];
        if rest.trim().is_empty() {
            self.blanks += 1;
            self.keep(line, shift);
            return;
        }
        // The spaces after the last quote marker.
        let indent = lead - text[..lead].rfind('>').map_or(0, |i| i + 1);
        let marker = indent <= 3
            && rest
                .strip_prefix(':')
                .is_some_and(|after| after.is_empty() || after.starts_with([' ', '\t']));
        if marker && self.blanks >= 2 && !self.in_definition {
            self.out.keep(&line[..lead]);
            self.out.insert("\\");
            self.out.keep(&line[lead..]);
        } else if marker {
            let first_line_empty = rest[1..].trim().is_empty();
            let quoted = text[..lead].contains('>');
            if first_line_empty && !quoted && self.blanks == 0 && self.out.read > 0 && indent < 2 {
                self.shift = Some((2 - indent, false));
            }
            self.in_definition = true;
            self.keep(line, shift);
        } else {
            if self.blanks > 0 && indent == 0 {
                self.in_definition = false;
            }
            self.keep(line, shift);
        }
        self.blanks = 0;
    }

    /// The spaces to take off `line`, if it's indented by four or more in a
    /// definition with nothing on its first line, which ends at a line
    /// indented by less than two, or a blank line right after the first.
    fn take_shift(&mut self, line: &str) -> usize {
        let Some((by, started)) = self.shift else {
            return 0;
        };
        if line.trim().is_empty() {
            if !started {
                self.shift = None;
            }
            return 0;
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent < 2 {
            self.shift = None;
            return 0;
        }
        self.shift = Some((by, true));
        if indent >= 4 { by } else { 0 }
    }

    /// Keeps `line`, less `shift` of the four spaces it starts with.
    fn keep(&mut self, line: &str, shift: usize) {
        if shift == 0 {
            self.out.keep(line);
            return;
        }
        self.out.replace(4, &"    "[shift..]);
        self.out.keep(&line[4..]);
    }
}

/// kramdown ends an HTML block with the end tag of the element starting
/// it, or the end of a comment, reading any text after it as Markdown.
/// CommonMark runs most HTML blocks to the next blank line, so this puts
/// one after the end, and a line break before any text after it on its
/// line.
pub(crate) struct HtmlBlocks;

impl HtmlBlocks {
    /// kramdown's block-level elements.
    const ELEMENTS: [&str; 50] = [
        "address",
        "applet",
        "article",
        "aside",
        "blockquote",
        "body",
        "caption",
        "col",
        "colgroup",
        "dd",
        "div",
        "dl",
        "dt",
        "fieldset",
        "figcaption",
        "footer",
        "form",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "header",
        "hgroup",
        "hr",
        "html",
        "iframe",
        "legend",
        "li",
        "main",
        "map",
        "menu",
        "nav",
        "ol",
        "optgroup",
        "p",
        "pre",
        "section",
        "summary",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "tr",
        "ul",
        "figure",
    ];

    /// `markdown`, with each HTML block ended where kramdown ends it, and
    /// the offsets of the lines added.
    pub fn separate(markdown: &str) -> (Cow<'_, str>, Offsets) {
        if !markdown.contains('<') {
            return (Cow::Borrowed(markdown), Offsets::default());
        }
        let mut out = Rewrite {
            out: String::with_capacity(markdown.len() + 16),
            ..Rewrite::default()
        };
        let lines: Vec<&str> = markdown.split_inclusive('\n').collect();
        let mut fence = Fence::default();
        // Whether the lines are in an HTML block this leaves as it is.
        let mut html = false;
        // What's left of a line after an HTML block ending on it.
        let mut rest: Option<&str> = None;
        let mut i = 0;
        loop {
            let line = match rest.take() {
                Some(line) => line,
                None if i < lines.len() => {
                    i += 1;
                    lines[i - 1]
                }
                None => break,
            };
            if fence.read(line) {
                out.keep(line);
                continue;
            }
            if line.trim().is_empty() {
                html = false;
                out.keep(line);
                continue;
            }
            let content = line.trim_start_matches(' ');
            if html || line.len() - content.len() > 3 || !content.starts_with('<') {
                out.keep(line);
                continue;
            }
            let Some((last, end)) = Self::end(content, &lines[i..]) else {
                // Not one, or never ended: CommonMark's to read.
                html = true;
                out.keep(line);
                continue;
            };
            let end_line = if last == 0 { line } else { lines[i + last - 1] };
            let end = if last == 0 {
                line.len() - content.len() + end
            } else {
                end
            };
            if last > 0 {
                out.keep(line);
                for line in &lines[i..i + last - 1] {
                    out.keep(line);
                }
                i += last;
            }
            out.keep(&end_line[..end]);
            let after = &end_line[end..];
            if after.trim().is_empty() {
                out.keep(after);
                if !after.ends_with('\n') {
                    continue;
                }
                if lines.get(i).is_some_and(|next| !next.trim().is_empty()) {
                    out.insert("\n");
                }
            } else {
                out.insert("\n\n");
                rest = Some(after);
            }
        }
        out.finish(markdown)
    }

    /// Where the HTML block `content` starts ends, if it starts one kramdown
    /// ends: the index of the line it ends on, `0` for this one and `1` for
    /// the first of `after`, and the end's offset in that line, or in
    /// `content` for this one.
    fn end(content: &str, after: &[&str]) -> Option<(usize, usize)> {
        if content.starts_with("<!--") {
            return std::iter::once(content)
                .chain(after.iter().copied())
                .enumerate()
                .find_map(|(n, line)| {
                    let from = if n == 0 { "<!--".len() } else { 0 };
                    let at = line[from..].find("-->")?;
                    Some((n, from + at + "-->".len()))
                });
        }
        let name_len = content[1..]
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(content.len() - 1);
        let name = &content[1..1 + name_len];
        let element = Self::ELEMENTS
            .into_iter()
            .find(|element| element.eq_ignore_ascii_case(name))?;
        let opening = &content[..content.find('>')? + 1];
        if opening.contains("markdown=") {
            return None;
        }
        if opening.ends_with("/>") || matches!(element, "hr" | "col") {
            return Some((0, opening.len()));
        }
        let mut depth = 0usize;
        for (n, line) in std::iter::once(content)
            .chain(after.iter().copied())
            .enumerate()
        {
            for (at, _) in line.match_indices('<') {
                let tag = &line[at + 1..];
                let (closing, tag) = match tag.strip_prefix('/') {
                    Some(tag) => (true, tag),
                    None => (false, tag),
                };
                let named = tag
                    .get(..element.len())
                    .is_some_and(|name| name.eq_ignore_ascii_case(element))
                    && tag[element.len()..].starts_with([' ', '\t', '\n', '\r', '>', '/']);
                if !named {
                    continue;
                }
                let Some(close) = line[at..].find('>').map(|close| at + close + 1) else {
                    continue;
                };
                if closing {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((n, close));
                    }
                } else if !line[..close].ends_with("/>") {
                    depth += 1;
                }
            }
        }
        None
    }
}

/// What a stand-in, put in the text by [`StandIns`], stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StandIn<'a> {
    /// Nothing: an end-of-block marker, or an extension giving nothing.
    Nothing,
    /// A `comment` extension's text.
    Comment(&'a str),
    /// A `nomarkdown` extension's text, given out as HTML.
    Raw(&'a str),
}

/// A stand-in in the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mark<'a> {
    /// Where it starts.
    pub at: usize,
    /// Whether it's a block's, rather than a span's.
    pub block: bool,
    pub stands_for: StandIn<'a>,
}

/// kramdown's end-of-block marker, a line of only `^`, and its `comment`
/// and `nomarkdown` extensions, `{::comment}…{:/}`, are text to
/// CommonMark. This puts a stand-in in place of each, for the reader to
/// give out what it stands for instead: for a block, an empty HTML
/// comment, which ends the same blocks a marker does, and for a span, an
/// autolink, which can't start an HTML block. The extensions' tags that
/// aren't read as one, like one never closed, are escaped, to stay text as
/// in kramdown.
pub(crate) struct StandIns<'a> {
    markdown: &'a str,
    out: Rewrite,
    marks: Vec<Mark<'a>>,
    end_of_blocks: bool,
    comments: bool,
    nomarkdown: bool,
    tables: bool,
    /// Whether `$$` math is read, a `|` in which is no cell's end.
    math: bool,
    /// The abbreviations defined, if they're read.
    abbreviations: Option<Vec<Abbreviation<'a>>>,
}

/// Markdown with stand-ins in it, by [`StandIns`].
pub(crate) struct Marked<'a> {
    pub text: Cow<'a, str>,
    pub offsets: Offsets,
    pub marks: Vec<Mark<'a>>,
    pub abbreviations: Vec<Abbreviation<'a>>,
}

/// Reading the lines of kramdown's tables.
struct Table;

impl Table {
    /// Where the `|` in `line` outside code, and math if `math` is read, and
    /// not escaped, are.
    fn pipes(line: &str, math: bool) -> Vec<usize> {
        let code = Self::code_spans(line);
        let math = if math {
            Self::math_spans(line)
        } else {
            Vec::new()
        };
        Self::unescaped_pipes(line)
            .filter(|at| !code.iter().chain(&math).any(|span| span.contains(at)))
            .collect()
    }

    /// Where the math between `$$` in `line` is.
    fn math_spans(line: &str) -> Vec<Range<usize>> {
        let mut spans = Vec::new();
        let mut from = 0;
        while let Some(open) = line[from..].find("$$").map(|at| from + at + 2)
            && let Some(close) = line[open..].find("$$").map(|at| open + at)
        {
            spans.push(open..close);
            from = close + 2;
        }
        spans
    }

    /// Where the `|` in code in `line`, and not escaped, are.
    fn code_pipes(line: &str) -> Vec<usize> {
        let code = Self::code_spans(line);
        Self::unescaped_pipes(line)
            .filter(|at| code.iter().any(|span| span.contains(at)))
            .collect()
    }

    fn unescaped_pipes(line: &str) -> impl Iterator<Item = usize> + '_ {
        line.match_indices('|')
            .map(|(at, _)| at)
            .filter(move |&at| {
                let backslashes = line[..at].len() - line[..at].trim_end_matches('\\').len();
                backslashes.is_multiple_of(2)
            })
    }

    /// Where the code spans closed in `line` are.
    fn code_spans(line: &str) -> Vec<Range<usize>> {
        let mut spans = Vec::new();
        let mut from = 0;
        while let Some(open) = line[from..].find('`').map(|at| from + at) {
            let run = line[open..].len() - line[open..].trim_start_matches('`').len();
            let start = open + run;
            // The next run of as many backticks closes it.
            let mut search = start;
            let mut close = None;
            while let Some(at) = line[search..].find('`').map(|at| search + at) {
                let len = line[at..].len() - line[at..].trim_start_matches('`').len();
                if len == run {
                    close = Some(at);
                    break;
                }
                search = at + len;
            }
            match close {
                Some(close) => {
                    spans.push(start..close);
                    from = close + run;
                }
                None => from = start,
            }
        }
        spans
    }

    /// How many cells the row `line` has.
    fn cells(line: &str, math: bool) -> usize {
        let row = line.trim();
        let pipes = Self::pipes(row, math);
        let mut cells = pipes.len() + 1;
        if row.starts_with('|') {
            cells -= 1;
        }
        if row.len() > 1 && pipes.last() == Some(&(row.len() - 1)) {
            cells -= 1;
        }
        cells.max(1)
    }

    /// Whether `line` separates parts of a table, like `|---|---|`.
    fn separator(line: &str) -> bool {
        let line = line.trim();
        line.contains(['-', '='])
            && line
                .chars()
                .all(|c| matches!(c, '|' | '+' | '-' | ':' | '=' | ' ' | '\t'))
    }

    /// The separator `line` as CommonMark reads a header's, with a cell for
    /// each of `columns`, aligned as in `line`, if it isn't one already:
    /// kramdown's may have fewer cells, and `+` between them.
    fn delimiter(line: &str, columns: usize) -> Option<String> {
        let line = line.trim();
        let inside = line.strip_prefix(['|', '+']).unwrap_or(line);
        let inside = inside.strip_suffix(['|', '+']).unwrap_or(inside);
        let cells: Vec<&str> = inside.split(['|', '+']).map(str::trim).collect();
        if cells.len() == columns && !line.contains('+') {
            return None;
        }
        let mut delimiter = String::from("|");
        for n in 0..columns {
            let cell = cells.get(n).copied().unwrap_or("-");
            delimiter.push_str(match (cell.starts_with(':'), cell.ends_with(':')) {
                (true, true) => ":-:",
                (true, false) => ":-",
                (false, true) => "-:",
                (false, false) => "-",
            });
            delimiter.push('|');
        }
        Some(delimiter)
    }
}

/// An extension's start tag, like `{::comment}` or `{::nomarkdown/}`.
struct StartTag<'a> {
    name: &'a str,
    /// How long it is.
    len: usize,
    /// Whether it closes itself, giving nothing.
    closed: bool,
    /// Whether it gives what it holds out as HTML, which a `nomarkdown`
    /// does unless its `type` is for another converter.
    html: bool,
}

impl<'a> StandIns<'a> {
    /// What stands in for a block.
    pub const BLOCK: &'static str = "<!---->";
    /// What stands in for a span.
    pub const SPAN: &'static str = "<kd:x>";

    /// `markdown`, with a stand-in in place of each end-of-block marker,
    /// `comment`, `nomarkdown` and abbreviation definition the `extensions`
    /// read, and a header row put in each table without one, with the
    /// offsets of the edits, the stand-ins, in order, and the abbreviations.
    pub fn mark(markdown: &'a str, extensions: Extensions) -> Marked<'a> {
        let reads = |extension| extensions.contains(extension);
        let markers = reads(Extensions::ENABLE_END_OF_BLOCK_MARKERS) && markdown.contains('^');
        let tags = (reads(Extensions::ENABLE_COMMENT_EXTENSION)
            || reads(Extensions::ENABLE_NOMARKDOWN_EXTENSION))
            && markdown.contains("{:");
        let tables = reads(Extensions::ENABLE_TABLES_WITHOUT_HEADERS)
            && reads(Extensions::ENABLE_TABLES)
            && markdown.contains('|');
        let abbreviations = reads(Extensions::ENABLE_ABBREVIATIONS) && markdown.contains("*[");
        if !markers && !tags && !tables && !abbreviations {
            return Marked {
                text: Cow::Borrowed(markdown),
                offsets: Offsets::default(),
                marks: Vec::new(),
                abbreviations: Vec::new(),
            };
        }
        let mut stand_ins = Self {
            markdown,
            out: Rewrite {
                out: String::with_capacity(markdown.len() + 16),
                ..Rewrite::default()
            },
            marks: Vec::new(),
            abbreviations: abbreviations.then(Vec::new),
            end_of_blocks: reads(Extensions::ENABLE_END_OF_BLOCK_MARKERS),
            comments: reads(Extensions::ENABLE_COMMENT_EXTENSION),
            nomarkdown: reads(Extensions::ENABLE_NOMARKDOWN_EXTENSION),
            tables,
            math: reads(Extensions::ENABLE_MATH),
        };
        stand_ins.lines();
        let (text, offsets) = stand_ins.out.finish(markdown);
        Marked {
            text,
            offsets,
            marks: stand_ins.marks,
            abbreviations: stand_ins.abbreviations.unwrap_or_default(),
        }
    }

    /// Puts a stand-in in place of the abbreviation definition starting at
    /// the line at `start`, with the lines of only attribute lists right
    /// before and after it, which are for it, returning the index of its
    /// last line, after keeping all but that line's end.
    fn abbreviation(&mut self, lines: &[&'a str], start: usize) -> Option<usize> {
        self.abbreviations.as_ref()?;
        let only_lists = |line: &'a str| {
            let trimmed = line.trim();
            Lists::parse(trimmed)
                .filter(|(_, len)| *len == trimmed.len())
                .map(|(lists, _)| lists)
        };
        let mut lists = Lists::default();
        let mut end = start;
        while let Some(before) = lines.get(end).copied().and_then(only_lists) {
            lists.extend(before);
            end += 1;
        }
        let mut abbreviation = Abbreviation::defined(lines.get(end)?)?;
        while let Some(after) = lines.get(end + 1).copied().and_then(only_lists) {
            lists.extend(after);
            end += 1;
        }
        abbreviation.lists = lists;
        self.abbreviations.as_mut()?.push(abbreviation);
        let line = lines[start];
        let lead = line.len() - line.trim_start().len();
        let len = self.offset(lines[end]) + lines[end].trim_end().len() - self.offset(line) - lead;
        self.stand_in(&line[..lead], len, true, StandIn::Nothing);
        Some(end)
    }

    fn lines(&mut self) {
        let lines: Vec<&'a str> = self.markdown.split_inclusive('\n').collect();
        let mut fence = Fence::default();
        // Whether the lines are in an HTML block, where nothing is kramdown.
        let mut html = false;
        let mut after_blank = true;
        // Whether a block may start on the line, as a table only does.
        let mut boundary = true;
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            i += 1;
            let starts_block = std::mem::replace(&mut boundary, false);
            if fence.read(line) {
                self.out.keep(line);
                after_blank = false;
                continue;
            }
            let trimmed = line.trim();
            let lead = line.len() - line.trim_start_matches([' ', '>']).len();
            if trimmed.is_empty() {
                html = false;
                after_blank = true;
                boundary = true;
                self.out.keep(line);
                continue;
            }
            html |= after_blank && line[lead..].starts_with('<');
            after_blank = false;
            // The spaces after the last quote marker.
            let indent = lead - line[..lead].rfind('>').map_or(0, |i| i + 1);
            if html || indent >= 4 {
                self.out.keep(line);
                continue;
            }
            let content = line[lead..].trim_end();
            boundary = content.starts_with('#')
                || Lists::parse(content).is_some_and(|(_, len)| len == content.len());
            if starts_block
                && !line[..lead].contains('>')
                && let Some(end) = self.abbreviation(&lines, i - 1)
            {
                let last = lines[end];
                self.out.keep(&last[last.trim_end().len()..]);
                i = end + 1;
                boundary = true;
                continue;
            }
            if self.end_of_blocks && content == "^" {
                self.stand_in(&line[..lead], content.len(), true, StandIn::Nothing);
                self.out.keep(&line[lead + content.len()..]);
                boundary = true;
                continue;
            }
            let quoted = line[..lead].contains('>');
            if !quoted && let Some(end) = self.block(&lines, i - 1, lead) {
                let last = lines[end];
                let last_content = last.trim_end().len();
                self.out.keep(&last[last_content..]);
                i = end + 1;
                continue;
            }
            if self.tables
                && starts_block
                && !quoted
                && let Some(end) = self.table(&lines, i - 1)
            {
                i = end;
                continue;
            }
            if self.comments || self.nomarkdown {
                self.spans(line);
            } else {
                self.out.keep(line);
            }
        }
    }

    /// Keeps the table starting at the line at `start`, if one does,
    /// returning the index of the line after it. A table is the lines with
    /// a `|` in them outside code, ending at a blank line, the end, or
    /// attribute lists. One without a separator line gets a header row
    /// put in for the reader to leave out, and a `|` in code is escaped,
    /// for CommonMark to read it as kramdown does.
    fn table(&mut self, lines: &[&'a str], start: usize) -> Option<usize> {
        let end = (start..lines.len())
            .find(|&j| Table::pipes(lines[j], self.math).is_empty())
            .unwrap_or(lines.len());
        if end == start {
            return None;
        }
        let after = lines.get(end).map_or("", |line| line.trim());
        let lists_after = Lists::parse(after).is_some_and(|(_, len)| len == after.len());
        let rows = &lines[start..end];
        let separators = rows.iter().filter(|row| Table::separator(row)).count();
        if !(after.is_empty() || lists_after) || separators == rows.len() {
            return None;
        }
        if separators == 0 {
            let columns = rows
                .iter()
                .map(|row| Table::cells(row, self.math))
                .max()
                .unwrap_or(1);
            let mut head = "|".to_owned();
            head.push_str(&" |".repeat(columns));
            head.push_str("\n|");
            head.push_str(&"-|".repeat(columns));
            head.push('\n');
            self.marks.push(Mark {
                at: self.out.out.len(),
                block: true,
                stands_for: StandIn::Nothing,
            });
            self.out.insert(&head);
        } else if !rows.get(1).is_some_and(|row| Table::separator(row)) {
            // A separator further on, which CommonMark has no way to read.
            return None;
        }
        // A header's separator as CommonMark reads one: a cell for each of
        // the header's, between `|`.
        let delimiter = (separators > 0)
            .then(|| Table::delimiter(rows[1], Table::cells(rows[0], self.math)))
            .flatten();
        for (n, row) in rows.iter().enumerate() {
            if n == 1
                && let Some(delimiter) = &delimiter
            {
                let content = row.trim_end().len();
                self.out.replace(content, delimiter);
                self.out.keep(&row[content..]);
                continue;
            }
            let mut kept = 0;
            for at in Table::code_pipes(row) {
                self.out.keep(&row[kept..at]);
                self.out.insert("\\");
                kept = at;
            }
            self.out.keep(&row[kept..]);
        }
        Some(end)
    }

    /// Puts a stand-in for `stands_for` in place of `len` bytes, after
    /// keeping `before`.
    fn stand_in(&mut self, before: &str, len: usize, block: bool, stands_for: StandIn<'a>) {
        self.out.keep(before);
        self.marks.push(Mark {
            at: self.out.out.len(),
            block,
            stands_for,
        });
        let text = if block { Self::BLOCK } else { Self::SPAN };
        self.out.replace(len, text);
    }

    /// Puts a stand-in in place of the block extension the line at `start`
    /// starts, after its `lead`, if it's one closed on a line of its own,
    /// returning the index of that line, after keeping all but its line
    /// end.
    fn block(&mut self, lines: &[&'a str], start: usize, lead: usize) -> Option<usize> {
        let line = lines[start];
        let content = line[lead..].trim_end();
        let tag = self
            .start_tag(content)
            .filter(|tag| tag.len == content.len())?;
        if tag.closed {
            self.stand_in(&line[..lead], content.len(), true, StandIn::Nothing);
            return Some(start);
        }
        let end = (start + 1..lines.len()).find(|&j| Self::closes(lines[j].trim(), tag.name))?;
        let body = &self.markdown[self.offset(lines[start + 1])..self.offset(lines[end])];
        let len = self.offset(lines[end]) + lines[end].trim_end().len() - self.offset(line) - lead;
        let stands_for = Self::giving(&tag, body);
        self.stand_in(&line[..lead], len, true, stands_for);
        Some(end)
    }

    /// Where `line`, a part of the markdown, starts in it.
    fn offset(&self, line: &str) -> usize {
        line.as_ptr() as usize - self.markdown.as_ptr() as usize
    }

    /// Keeps `line`, putting a stand-in in place of each span extension in
    /// it outside code, and escaping the tags left.
    fn spans(&mut self, line: &'a str) {
        let bytes = line.as_bytes();
        let mut kept = 0;
        let mut i = 0;
        while i < line.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'`' => {
                    let run = line[i..].len() - line[i..].trim_start_matches('`').len();
                    let closing = "`".repeat(run);
                    i += run;
                    // Past the code span, if it's closed on the line.
                    let mut from = i;
                    while let Some(at) = line[from..].find(&closing) {
                        let at = from + at;
                        let after = line[at..].len() - line[at..].trim_start_matches('`').len();
                        if after == run {
                            i = at + run;
                            break;
                        }
                        from = at + after;
                    }
                }
                b'{' => {
                    let rest = &line[i..];
                    if let Some(tag) = self.start_tag(rest) {
                        let closing = if tag.closed {
                            Some((tag.len, tag.len))
                        } else {
                            Self::find_end(&rest[tag.len..], tag.name)
                                .map(|(at, len)| (tag.len + at, tag.len + at + len))
                        };
                        if let Some((body_end, end)) = closing {
                            let stands_for = if tag.closed {
                                StandIn::Nothing
                            } else {
                                Self::giving(&tag, &rest[tag.len..body_end])
                            };
                            self.stand_in(&line[kept..i], end, false, stands_for);
                            i += end;
                            kept = i;
                            continue;
                        }
                    }
                    if self.start_tag(rest).is_some() || self.end_tag(rest).is_some() {
                        self.out.keep(&line[kept..i]);
                        self.out.insert("\\");
                        kept = i;
                    }
                    i += 1;
                }
                _ => i += 1,
            }
        }
        self.out.keep(&line[kept..]);
    }

    /// What an extension started with `tag`, holding `body`, gives.
    fn giving(tag: &StartTag<'_>, body: &'a str) -> StandIn<'a> {
        match tag.name {
            "comment" => StandIn::Comment(body),
            _ if tag.html => StandIn::Raw(body),
            _ => StandIn::Nothing,
        }
    }

    /// The start tag of an extension read, at the start of `text`.
    fn start_tag<'t>(&self, text: &'t str) -> Option<StartTag<'t>> {
        let inside = text.strip_prefix("{::")?;
        let close = inside.find('}')?;
        let inside = &inside[..close];
        let name_len = inside
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(inside.len());
        let name = &inside[..name_len];
        let after = &inside[name_len..];
        if !self.reads(name) || !(after.is_empty() || after.starts_with([' ', '\t', '/'])) {
            return None;
        }
        let closed = after.trim_end().ends_with('/');
        let html = name != "nomarkdown"
            || Self::type_of(after).is_none_or(|kind| kind.is_empty() || kind.contains("html"));
        Some(StartTag {
            name,
            len: "{::".len() + close + 1,
            closed,
            html,
        })
    }

    /// The length of the end tag at the start of `text` of an extension
    /// read, `{:/name}` or `{:/}`.
    fn end_tag(&self, text: &str) -> Option<usize> {
        let inside = text.strip_prefix("{:/")?;
        let name = &inside[..inside.find('}')?];
        (name.is_empty() || self.reads(name)).then(|| "{:/}".len() + name.len())
    }

    /// Where the first end tag for `name` in `text` is, and its length.
    fn find_end(text: &str, name: &str) -> Option<(usize, usize)> {
        text.match_indices("{:/").find_map(|(at, _)| {
            let inside = &text[at + "{:/".len()..];
            let tag = &inside[..inside.find('}')?];
            (tag.is_empty() || tag == name).then(|| (at, "{:/}".len() + tag.len()))
        })
    }

    /// Whether `trimmed` is a line of only an end tag for `name`.
    fn closes(trimmed: &str, name: &str) -> bool {
        Self::find_end(trimmed, name).is_some_and(|(at, len)| at == 0 && len == trimmed.len())
    }

    /// The value of the `type` in the rest of a start tag, if any.
    fn type_of(rest: &str) -> Option<&str> {
        let after = &rest[rest.find("type=")? + "type=".len()..];
        let quote = after.chars().next().filter(|c| matches!(c, '"' | '\''))?;
        let value = &after[1..];
        Some(&value[..value.find(quote)?])
    }

    /// Whether the extension `name` is read.
    fn reads(&self, name: &str) -> bool {
        match name {
            "comment" => self.comments,
            "nomarkdown" => self.nomarkdown,
            _ => false,
        }
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
    fn markdown_elements_separate_splits_markdown_from_tags() {
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
    fn markdown_elements_without_attribute_takes_it_only_out_of_tags() {
        assert_eq!(
            MarkdownElements::without_attribute("<div class=\"a\"\n  markdown=\"1\" id=\"b\">\n"),
            Some("<div class=\"a\" id=\"b\">\n".into())
        );
        assert_eq!(
            MarkdownElements::without_attribute("<p markdown=\"1\">a</p><p markdown=\"1\">"),
            Some("<p>a</p><p>".into())
        );
        // Not in text, nor as part of another attribute.
        for html in ["<pre>x markdown=\"1\"</pre>", "<p data-markdown=\"1\">"] {
            assert_eq!(MarkdownElements::without_attribute(html), None, "{html}");
        }
    }

    #[test]
    fn span_tags_join_starts_paragraphs_with_span_tags() {
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
    fn span_tags_join_ignores_case_and_joins_closing_tags() {
        assert_eq!(SpanTags::join("<IMG src=x>\na\n").0, "<IMG src=x> a\n");
        assert_eq!(SpanTags::join("</span>\na\n").0, "</span> a\n");
        // Not two tags on a line.
        assert!(matches!(
            SpanTags::join("<span><b>\na\n").0,
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn markdown_elements_separate_gives_an_element_ending_the_input_its_blank_line() {
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
    fn offsets_original_skips_added_blank_lines() {
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
    fn offsets_original_takes_in_joined_line_ends() {
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
            assert_eq!(was(src, &text, &offsets, needle), needle);
        }
        assert_eq!(was(src, &text, &offsets, "\n\n<b> ü"), "\n<b>\nü");
        assert_eq!(was(src, &text, &offsets, "<i> y"), "<i>  \ny");
        assert_ranges_fit(src, &text, &offsets);
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
        assert_eq!(was(src, &text, &offsets, "ab"), "ab");
        assert_eq!(was(src, &text, &offsets, "X-"), "\n");
        assert_eq!(was(src, &text, &offsets, "-"), "\n");
        assert_eq!(was(src, &text, &offsets, "cd\n"), "cd\n");
        assert_ranges_fit(src, &text, &offsets);
    }

    #[test]
    fn attribute_lines_separate_ends_a_line_of_lists_before_code_or_a_link_definition() {
        let src = "a\n{:.x}\n    b\n\n{:.y}\n\tc\n\n{:.z}\n[d]: e\n";
        let (text, offsets) = AttributeLines::separate(src);
        assert_eq!(
            text,
            "a\n{:.x}\n\n    b\n\n{:.y}\n\n\tc\n\n{:.z}\n\n[d]: e\n"
        );
        assert_eq!(was(src, &text, &offsets, "{:.x}\n\n"), "{:.x}\n");
        assert_ranges_fit(src, &text, &offsets);
        // Not before what isn't code or a link definition, after more than
        // lists, in a list, a quote, HTML or a fence, or indented.
        let src = "{:.x}\n  a\n\n{:.x}\n[^b]: c\n\n{:.x}\n[d] e: f\n\nb {:.x}\n    c\n\n\
            * d\n{:.x}\n    e\n\n> f\n{:.x}\n    g\n\n<div>\n{:.x}\n    h\n\n```\n{:.x}\n    i\n\
            ```\n\n  j\n {:.x}\n    k\n";
        let (text, offsets) = AttributeLines::separate(src);
        assert!(matches!(text, Cow::Borrowed(_)));
        assert!(offsets.is_empty());
    }

    #[test]
    fn definition_lines_rewrite_escapes_a_definition_far_from_its_terms() {
        let src = "a\n\n\n: b\n\n> c\n>\n>\n>  : d\n\ne\n\n: f\n\n\n: g\n\n    : h\n";
        let (text, offsets) = DefinitionLines::rewrite(src);
        // Not after one blank line, nor after a definition, nor as code.
        assert_eq!(
            text,
            "a\n\n\n\\: b\n\n> c\n>\n>\n>  \\: d\n\ne\n\n: f\n\n\n: g\n\n    : h\n"
        );
        assert_eq!(was(src, &text, &offsets, "\\: b"), ": b");
        assert_ranges_fit(src, &text, &offsets);
        let (text, _) = DefinitionLines::rewrite("```\na\n\n\n: b\n```\n");
        assert!(matches!(text, Cow::Borrowed(_)));
    }

    #[test]
    fn definition_lines_rewrite_indents_a_definition_empty_on_its_first_line_by_four() {
        let src = "a\n: \n        b\n   c\n\n      d\ne\n        f\n";
        let (text, offsets) = DefinitionLines::rewrite(src);
        // Only the lines indented by four or more, until one by less than two.
        assert_eq!(text, "a\n: \n      b\n   c\n\n    d\ne\n        f\n");
        assert_eq!(was(src, &text, &offsets, "      b"), "        b");
        assert_ranges_fit(src, &text, &offsets);
        // By one with the `:` indented by one, and not at all after a blank
        // line, which ends the definition, or with something on the line.
        for (src, rewritten) in [
            ("a\n : \n     b\n", "a\n : \n    b\n"),
            ("a\n: \n\n    b\n", "a\n: \n\n    b\n"),
            ("a\n: x\n    b\n", "a\n: x\n    b\n"),
        ] {
            assert_eq!(DefinitionLines::rewrite(src).0, rewritten, "{src:?}");
        }
    }

    #[test]
    fn html_blocks_separate_ends_a_block_at_its_end_tag() {
        let src = "<DIV>\n<div>a</div>\n</div>\nb\n<p>c</p> <hr> d\n<!-- e --> f\n";
        let (text, offsets) = HtmlBlocks::separate(src);
        assert_eq!(
            text,
            "<DIV>\n<div>a</div>\n</div>\n\nb\n<p>c</p>\n\n <hr>\n\n d\n<!-- e -->\n\n f\n"
        );
        assert_ranges_fit(src, &text, &offsets);
        // Not span elements, ones never ended, or what a blank line ends.
        let src = "<span>a</span> b\n<div>\nc\n\n<p>d</p>\n\ne\n";
        let (text, offsets) = HtmlBlocks::separate(src);
        assert!(matches!(text, Cow::Borrowed(_)));
        assert!(offsets.is_empty());
    }

    #[test]
    fn stand_ins_mark_only_lines_of_an_end_of_block_marker() {
        let src = "a\n^\n> b\n>  ^ \n";
        let Marked {
            text,
            offsets,
            marks,
            ..
        } = StandIns::mark(src, Extensions::ENABLE_END_OF_BLOCK_MARKERS);
        assert_eq!(text, "a\n<!---->\n> b\n>  <!----> \n");
        let at: Vec<_> = marks.iter().map(|mark| (mark.at, mark.block)).collect();
        assert_eq!(at, [(2, true), (17, true)]);
        assert_eq!(was(src, &text, &offsets, "<!---->"), "^");
        assert_ranges_fit(src, &text, &offsets);
        // Not with more on the line, as code, in a fence, or in HTML.
        let src = "^ a\n\n    ^\n\n```\n^\n```\n\n<div>\n^\n</div>\n";
        let Marked { text, marks, .. } =
            StandIns::mark(src, Extensions::ENABLE_END_OF_BLOCK_MARKERS);
        assert!(matches!(text, Cow::Borrowed(_)));
        assert!(marks.is_empty());
    }

    #[test]
    fn stand_ins_mark_heads_tables_without_one() {
        let tables = Extensions::ENABLE_TABLES | Extensions::ENABLE_TABLES_WITHOUT_HEADERS;
        let src = "| a | `b|c`\n| d |\n\n| e | f |\n|-+-:\n| g |\n";
        let Marked {
            text,
            offsets,
            marks,
            ..
        } = StandIns::mark(src, tables);
        assert_eq!(
            text,
            "| | |\n|-|-|\n| a | `b\\|c`\n| d |\n\n| e | f |\n|-|-:|\n| g |\n"
        );
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].at, 0);
        assert_ranges_fit(src, &text, &offsets);
        // Not after a line of a paragraph, before more of one, of only
        // separators, with a separator further on, or with `|` only in code
        // or math, or escaped.
        let src =
            "a\n| b |\n\n| c |\nd\n\n|-|\n\n| e |\n| f |\n|-|\n\n`g|h`\n\n$$i|j$$\n\nk \\| l\n";
        let Marked { text, marks, .. } = StandIns::mark(src, tables | Extensions::ENABLE_MATH);
        assert!(matches!(text, Cow::Borrowed(_)));
        assert!(marks.is_empty());
    }

    #[test]
    fn stand_ins_mark_extensions_closed_and_escape_the_rest() {
        let src = "{::comment}\na {:/} b\n\n{:/comment}\n{::nomarkdown type=\"latex\"}\nc\n{:/}\n\
            d {::comment}e{:/} `{::comment}f{:/}` {::nomarkdown/} {:/} {::comment} {:/other}\n";
        let Marked {
            text,
            offsets,
            marks,
            ..
        } = StandIns::mark(
            src,
            Extensions::ENABLE_COMMENT_EXTENSION | Extensions::ENABLE_NOMARKDOWN_EXTENSION,
        );
        assert_eq!(
            text,
            "<!---->\n<!---->\nd <kd:x> `{::comment}f{:/}` <kd:x> \\{:/} \\{::comment} {:/other}\n"
        );
        let stands_for: Vec<_> = marks.iter().map(|mark| mark.stands_for).collect();
        assert_eq!(
            stands_for,
            [
                StandIn::Comment("a {:/} b\n\n"),
                StandIn::Nothing,
                StandIn::Comment("e"),
                StandIn::Nothing,
            ]
        );
        assert_eq!(was(src, &text, &offsets, "<kd:x> `"), "{::comment}e{:/} `");
        assert_ranges_fit(src, &text, &offsets);
        // Only the extensions read: here, the two `nomarkdown`.
        let marks = StandIns::mark(src, Extensions::ENABLE_NOMARKDOWN_EXTENSION).marks;
        assert_eq!(marks.len(), 2);
    }

    #[test]
    fn fence_read_closes_only_with_its_own_marks() {
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

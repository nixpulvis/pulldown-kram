use std::borrow::Cow;
use std::ops::Range;

use super::{Fence, Offsets, Rewrite};
use crate::Extensions;
use crate::abbreviations::Abbreviation;
use crate::attributes::AttributeLists;

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
struct TableLine;

impl TableLine {
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
        let tags = (reads(Extensions::ENABLE_COMMENTS) || reads(Extensions::ENABLE_NOMARKDOWN))
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
            out: Rewrite::of(markdown),
            marks: Vec::new(),
            abbreviations: abbreviations.then(Vec::new),
            end_of_blocks: reads(Extensions::ENABLE_END_OF_BLOCK_MARKERS),
            comments: reads(Extensions::ENABLE_COMMENTS),
            nomarkdown: reads(Extensions::ENABLE_NOMARKDOWN),
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
            AttributeLists::parse(trimmed)
                .filter(|(_, len)| *len == trimmed.len())
                .map(|(attribute_lists, _)| attribute_lists)
        };
        let mut attribute_lists = AttributeLists::default();
        let mut end = start;
        while let Some(before) = lines.get(end).copied().and_then(only_lists) {
            attribute_lists.extend(before);
            end += 1;
        }
        let mut abbreviation = Abbreviation::defined(lines.get(end)?)?;
        while let Some(after) = lines.get(end + 1).copied().and_then(only_lists) {
            attribute_lists.extend(after);
            end += 1;
        }
        abbreviation.attribute_lists = attribute_lists;
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
                || AttributeLists::parse(content).is_some_and(|(_, len)| len == content.len());
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
            .find(|&j| TableLine::pipes(lines[j], self.math).is_empty())
            .unwrap_or(lines.len());
        if end == start {
            return None;
        }
        let after = lines.get(end).map_or("", |line| line.trim());
        let lists_after = AttributeLists::parse(after).is_some_and(|(_, len)| len == after.len());
        let rows = &lines[start..end];
        let separators = rows.iter().filter(|row| TableLine::separator(row)).count();
        if !(after.is_empty() || lists_after) || separators == rows.len() {
            return None;
        }
        if separators == 0 {
            let columns = rows
                .iter()
                .map(|row| TableLine::cells(row, self.math))
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
        } else if !rows.get(1).is_some_and(|row| TableLine::separator(row)) {
            // A separator further on, which CommonMark has no way to read.
            return None;
        }
        // A header's separator as CommonMark reads one: a cell for each of
        // the header's, between `|`.
        let delimiter = (separators > 0)
            .then(|| TableLine::delimiter(rows[1], TableLine::cells(rows[0], self.math)))
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
            for at in TableLine::code_pipes(row) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(offsets.was(src, &text, "<!---->"), "^");
        offsets.assert_fit(src, &text);
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
        offsets.assert_fit(src, &text);
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
            Extensions::ENABLE_COMMENTS | Extensions::ENABLE_NOMARKDOWN,
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
        assert_eq!(offsets.was(src, &text, "<kd:x> `"), "{::comment}e{:/} `");
        offsets.assert_fit(src, &text);
        // Only the extensions read: here, the two `nomarkdown`.
        let marks = StandIns::mark(src, Extensions::ENABLE_NOMARKDOWN).marks;
        assert_eq!(marks.len(), 2);
    }
}

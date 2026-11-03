use std::borrow::Cow;

use super::{Fence, Offsets, Rewrite};

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
        let mut out = Rewrite::of(markdown);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_blocks_separate_ends_a_block_at_its_end_tag() {
        let src = "<DIV>\n<div>a</div>\n</div>\nb\n<p>c</p> <hr> d\n<!-- e --> f\n";
        let (text, offsets) = HtmlBlocks::separate(src);
        assert_eq!(
            text,
            "<DIV>\n<div>a</div>\n</div>\n\nb\n<p>c</p>\n\n <hr>\n\n d\n<!-- e -->\n\n f\n"
        );
        offsets.assert_fit(src, &text);
        // Not span elements, ones never ended, or what a blank line ends.
        let src = "<span>a</span> b\n<div>\nc\n\n<p>d</p>\n\ne\n";
        let (text, offsets) = HtmlBlocks::separate(src);
        assert!(matches!(text, Cow::Borrowed(_)));
        assert!(offsets.is_empty());
    }
}

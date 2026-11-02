//! kramdown's HTML and pulldown-kram's made comparable: tokenized, with
//! what's only formatting erased, one token on each line for a readable
//! diff.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::SUITE;

/// Normalises HTML, decoding the entities kramdown writes.
pub struct Normaliser {
    /// kramdown's named entities, by name.
    entities: HashMap<String, char>,
}

/// A piece of HTML.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    /// A start tag, its name lowercased, with its attributes.
    Start(String, Vec<(String, String)>),
    /// An end tag, its name lowercased.
    End(String),
    /// Text, its entities decoded.
    Text(String),
    /// A comment, a doctype, a processing instruction or CDATA, as written.
    Other(String),
}

/// Elements without an end tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Elements whose content is text, up to their end tag.
const RAW: &[&str] = &["script", "style", "textarea"];

/// Elements around which whitespace doesn't show.
const BLOCK: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "br",
    "center",
    "dd",
    "details",
    "dialog",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
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
    "li",
    "main",
    "menu",
    "nav",
    "ol",
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
];

impl Normaliser {
    /// The normaliser, with kramdown's named entities from entities.txt.
    pub fn load() -> Self {
        let table = fs::read_to_string(Path::new(SUITE).join("entities.txt")).unwrap();
        let entities = table
            .lines()
            .map(|line| {
                let (name, code_point) = line.split_once(' ').unwrap();
                let c = char::from_u32(code_point.parse().unwrap()).unwrap();
                (name.to_owned(), c)
            })
            .collect();
        Self { entities }
    }

    /// `html` as one token on each line, indented by depth, with what is only
    /// formatting erased: the order of attributes and of classes, how
    /// characters are written, entity or not, whitespace runs in text outside
    /// `<pre>` and in attribute values, whitespace next to a block element or a
    /// comment, and `<x />` against `<x></x>`. kramdown's ids for tables of
    /// contents, `markdown-toc` on the list and `markdown-toc-…` on its links,
    /// are dropped, as they're made by its converter.
    pub fn normalise(&self, html: &str) -> String {
        let tokens = Self::collapse_whitespace(self.tokenize(html));
        Self::print(Self::trim_at_blocks(&tokens))
    }

    /// `tokens` with whitespace runs in text made one space, outside `<pre>`,
    /// each with whether it's inside one.
    fn collapse_whitespace(tokens: Vec<Token>) -> Vec<(Token, bool)> {
        let mut pre = 0usize;
        tokens
            .into_iter()
            .map(|token| {
                match &token {
                    Token::Start(name, _) if name == "pre" => pre += 1,
                    Token::End(name) if name == "pre" => pre = pre.saturating_sub(1),
                    _ => {}
                }
                let token = match token {
                    Token::Text(text) if pre == 0 => Token::Text(Self::collapse(&text)),
                    token => token,
                };
                (token, pre > 0)
            })
            .collect()
    }

    /// `tokens` with text outside `<pre>` trimmed where it meets a block
    /// element or a comment, text left empty dropped, and text next to text
    /// joined.
    fn trim_at_blocks(tokens: &[(Token, bool)]) -> Vec<Token> {
        let is_boundary = |token: Option<&(Token, bool)>| match token {
            None | Some((Token::Other(_), _)) => true,
            Some((Token::Start(name, _) | Token::End(name), _)) => BLOCK.contains(&name.as_str()),
            Some((Token::Text(_), _)) => false,
        };
        let mut kept: Vec<Token> = Vec::with_capacity(tokens.len());
        for (i, (token, in_pre)) in tokens.iter().enumerate() {
            let token = match token {
                Token::Text(text) if !in_pre => {
                    let mut text = text.as_str();
                    if is_boundary(i.checked_sub(1).and_then(|i| tokens.get(i))) {
                        text = text.trim_start_matches(' ');
                    }
                    if is_boundary(tokens.get(i + 1)) {
                        text = text.trim_end_matches(' ');
                    }
                    Token::Text(text.to_owned())
                }
                token => token.clone(),
            };
            match (&token, kept.last_mut()) {
                (Token::Text(text), _) if text.is_empty() => {}
                (Token::Text(text), Some(Token::Text(last))) => last.push_str(text),
                _ => kept.push(token),
            }
        }
        kept
    }

    /// `tokens` one on each line, indented by depth.
    fn print(tokens: Vec<Token>) -> String {
        let mut out = String::new();
        let mut depth = 0usize;
        for token in tokens {
            if let Token::End(_) = token {
                depth = depth.saturating_sub(1);
            }
            out.push_str(&"  ".repeat(depth));
            match token {
                Token::Start(name, attributes) => {
                    write!(out, "<{name}").unwrap();
                    for (key, value) in Self::attributes(attributes) {
                        write!(out, " {key}={value:?}").unwrap();
                    }
                    out.push('>');
                    if !VOID.contains(&name.as_str()) {
                        depth += 1;
                    }
                }
                Token::End(name) => write!(out, "</{name}>").unwrap(),
                Token::Text(text) => write!(out, "{text:?}").unwrap(),
                Token::Other(other) => out.push_str(&Self::collapse(&other)),
            }
            out.push('\n');
        }
        out
    }

    /// `attributes` in order of name, classes in order, whitespace runs in
    /// values made one space, and kramdown's table of contents ids left out.
    fn attributes(mut attributes: Vec<(String, String)>) -> Vec<(String, String)> {
        attributes.retain(|(key, value)| {
            key != "id" || !(value == "markdown-toc" || value.starts_with("markdown-toc-"))
        });
        for (key, value) in &mut attributes {
            *value = if key == "class" {
                let mut classes: Vec<&str> = value.split_whitespace().collect();
                classes.sort_unstable();
                classes.join(" ")
            } else {
                Self::collapse(value)
            };
        }
        attributes.sort();
        attributes
    }

    /// `text` with each run of ASCII whitespace made one space.
    fn collapse(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut space = false;
        for c in text.chars() {
            if c.is_ascii_whitespace() {
                space = true;
            } else {
                if space {
                    out.push(' ');
                    space = false;
                }
                out.push(c);
            }
        }
        if space {
            out.push(' ');
        }
        out
    }

    /// `html` as tokens. A `<` starting nothing HTML is text.
    fn tokenize(&self, html: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut text = String::new();
        let mut rest = html;
        while !rest.is_empty() {
            let Some(at) = rest.find('<') else {
                text.push_str(rest);
                break;
            };
            text.push_str(&rest[..at]);
            rest = &rest[at..];
            let Some((token, len)) = self.tag(rest) else {
                text.push('<');
                rest = &rest[1..];
                continue;
            };
            if !text.is_empty() {
                tokens.push(Token::Text(self.decode(&std::mem::take(&mut text))));
            }
            rest = &rest[len..];
            let raw = match &token {
                Token::Start(name, _) if RAW.contains(&name.as_str()) => Some(name.clone()),
                _ => None,
            };
            let close = matches!(&token, Token::Start(name, attributes)
            if attributes.iter().any(|(key, _)| key == "/") && !VOID.contains(&name.as_str()));
            let token = match token {
                Token::Start(name, mut attributes) => {
                    attributes.retain(|(key, _)| key != "/");
                    Token::Start(name, attributes)
                }
                token => token,
            };
            if close && let Token::Start(name, _) = &token {
                let name = name.clone();
                tokens.push(token);
                tokens.push(Token::End(name));
                continue;
            }
            tokens.push(token);
            // The content of a raw text element is text, as written.
            if let Some(name) = raw {
                let end = rest
                    .to_ascii_lowercase()
                    .find(&format!("</{name}"))
                    .unwrap_or(rest.len());
                if end > 0 {
                    tokens.push(Token::Text(rest[..end].to_owned()));
                }
                rest = &rest[end..];
            }
        }
        if !text.is_empty() {
            tokens.push(Token::Text(self.decode(&text)));
        }
        tokens
    }

    /// The tag, comment or the like at the start of `html`, and its length, if
    /// there is one. A self-closing start tag is given a `/` attribute.
    fn tag(&self, html: &str) -> Option<(Token, usize)> {
        let bytes = html.as_bytes();
        for (open, close) in [
            ("<!--", "-->"),
            ("<![CDATA[", "]]>"),
            ("<!", ">"),
            ("<?", ">"),
        ] {
            if let Some(rest) = html.strip_prefix(open) {
                let end = rest.find(close)? + open.len() + close.len();
                return Some((Token::Other(html[..end].to_owned()), end));
            }
        }
        let end_tag = bytes.get(1) == Some(&b'/');
        let start = if end_tag { 2 } else { 1 };
        if !bytes.get(start)?.is_ascii_alphabetic() {
            return None;
        }
        let is_name = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b':' || b == b'_';
        let mut i = start;
        while i < bytes.len() && is_name(bytes[i]) {
            i += 1;
        }
        let name = html[start..i].to_ascii_lowercase();
        if end_tag {
            let end = html[i..].find('>')? + i + 1;
            if !html[i..end - 1].trim().is_empty() {
                return None;
            }
            return Some((Token::End(name), end));
        }
        let mut attributes = Vec::new();
        loop {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            match bytes.get(i)? {
                b'>' => return Some((Token::Start(name, attributes), i + 1)),
                b'/' if bytes.get(i + 1) == Some(&b'>') => {
                    attributes.push(("/".to_owned(), String::new()));
                    return Some((Token::Start(name, attributes), i + 2));
                }
                _ => {}
            }
            let key_start = i;
            while i < bytes.len() && !b" \t\n\r=>".contains(&bytes[i]) && bytes[i] != b'/' {
                i += 1;
            }
            if i == key_start {
                return None;
            }
            let key = html[key_start..i].to_ascii_lowercase();
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let mut value = String::new();
            if bytes.get(i) == Some(&b'=') {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                match bytes.get(i)? {
                    quote @ (b'"' | b'\'') => {
                        let len = html[i + 1..].find(*quote as char)?;
                        value = self.decode(&html[i + 1..i + 1 + len]);
                        i += len + 2;
                    }
                    _ => {
                        let value_start = i;
                        while i < bytes.len() && !b" \t\n\r>".contains(&bytes[i]) {
                            i += 1;
                        }
                        value = self.decode(&html[value_start..i]);
                    }
                }
            }
            attributes.push((key, value));
        }
    }

    /// `text` with its character references decoded: numeric ones, and named
    /// ones kramdown knows, ending in `;`.
    fn decode(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find('&') {
            out.push_str(&rest[..at]);
            rest = &rest[at..];
            let decoded = rest[1..].find(';').and_then(|end| {
                let name = &rest[1..end + 1];
                let c = if let Some(number) = name.strip_prefix('#') {
                    let code_point = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse().ok()?,
                    };
                    char::from_u32(code_point)?
                } else {
                    *self.entities.get(name)?
                };
                Some((c, end + 2))
            });
            match decoded {
                Some((c, len)) => {
                    out.push(c);
                    rest = &rest[len..];
                }
                None => {
                    out.push('&');
                    rest = &rest[1..];
                }
            }
        }
        out.push_str(rest);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `a` and `b` normalise the same.
    fn same(a: &str, b: &str) -> bool {
        let normaliser = Normaliser::load();
        normaliser.normalise(a) == normaliser.normalise(b)
    }

    #[test]
    fn erases_formatting() {
        // How characters are written.
        assert!(same(
            "<p>&ldquo;a&rdquo; &#8230; &#x2026;</p>",
            "<p>“a” … …</p>"
        ));
        // Attribute and class order, and quotes.
        assert!(same(
            "<p id=\"x\" class=\"b a\">t</p>",
            "<p class='a b' id=x>t</p>"
        ));
        // Empty elements written either way.
        assert!(same("<br /><hr/>", "<br><hr>"));
        assert!(same("<div />", "<div></div>"));
        // Whitespace runs, and whitespace next to blocks.
        assert!(same(
            "<ul>\n  <li>a  b</li>\n</ul>\n",
            "<ul><li>a b</li></ul>"
        ));
    }

    #[test]
    fn keeps_content_and_structure() {
        assert!(!same("<p>a</p>", "<p>b</p>"));
        assert!(!same("<p>a</p>", "<p><em>a</em></p>"));
        assert!(!same("<p>a</p><p>b</p>", "<p>a b</p>"));
        assert!(!same("<p id=\"x\">a</p>", "<p>a</p>"));
        // Whitespace next to inline elements shows.
        assert!(!same("<p>a <em>b</em></p>", "<p>a<em>b</em></p>"));
        // And so does whitespace in code.
        assert!(!same(
            "<pre><code>a  b\n</code></pre>",
            "<pre><code>a b\n</code></pre>"
        ));
    }
}

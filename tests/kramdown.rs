//! Documents read as kramdown 2.5.2 reads them with GFM input, Jekyll's
//! default. Each case reads a document twice: with one extension, and the
//! few it builds on, to show what that extension does, and with every
//! extension but it, to show what's left without it. The outlines are cut
//! down to the elements and attributes the events carry. With the extension,
//! they match kramdown's HTML for the same input, less what the extensions
//! left off would add, like header ids.

use pulldown_kram::pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use pulldown_kram::{Attributes, Document, Extensions, Options};

/// The document's events as HTML-like text, attributes included.
fn outline(markdown: &str, options: impl Into<Options>) -> String {
    let document = Document::new(markdown, options);
    let mut out = String::new();
    for (event, attributes) in document.events() {
        match event {
            Event::Start(tag) => {
                let (name, extra) = match tag {
                    Tag::Paragraph => ("p", String::new()),
                    Tag::Heading { level, id, .. } => (
                        ["h1", "h2", "h3", "h4", "h5", "h6"][level as usize - 1],
                        format!(" id=\"{}\"", id.as_deref().unwrap_or("")),
                    ),
                    Tag::BlockQuote(_) => ("blockquote", String::new()),
                    Tag::CodeBlock(CodeBlockKind::Fenced(lang)) if !lang.is_empty() => {
                        ("pre", format!(" lang=\"{lang}\""))
                    }
                    Tag::CodeBlock(_) => ("pre", String::new()),
                    Tag::List(None) => ("ul", String::new()),
                    Tag::List(Some(_)) => ("ol", String::new()),
                    Tag::Item => ("li", String::new()),
                    Tag::Emphasis => ("em", String::new()),
                    Tag::Strong => ("strong", String::new()),
                    Tag::Strikethrough => ("del", String::new()),
                    Tag::Link { dest_url, .. } => ("a", format!(" href=\"{dest_url}\"")),
                    Tag::FootnoteDefinition(label) => ("footnote", format!(" label=\"{label}\"")),
                    Tag::HtmlBlock => ("html", String::new()),
                    Tag::Table(_) => ("table", String::new()),
                    Tag::TableHead => ("thead", String::new()),
                    Tag::TableRow => ("tr", String::new()),
                    Tag::TableCell => ("td", String::new()),
                    Tag::DefinitionList => ("dl", String::new()),
                    Tag::DefinitionListTitle => ("dt", String::new()),
                    Tag::DefinitionListDefinition => ("dd", String::new()),
                    tag => panic!("unexpected {tag:?}"),
                };
                out.push_str(&format!("<{name}{extra}{}>", attrs(&attributes)));
            }
            Event::End(tag) => out.push_str(match tag {
                TagEnd::Paragraph => "</p>",
                TagEnd::Heading(level) => {
                    ["</h1>", "</h2>", "</h3>", "</h4>", "</h5>", "</h6>"][level as usize - 1]
                }
                TagEnd::BlockQuote(_) => "</blockquote>",
                TagEnd::CodeBlock => "</pre>",
                TagEnd::List(false) => "</ul>",
                TagEnd::List(true) => "</ol>",
                TagEnd::Item => "</li>",
                TagEnd::Emphasis => "</em>",
                TagEnd::Strong => "</strong>",
                TagEnd::Strikethrough => "</del>",
                TagEnd::FootnoteDefinition => "</footnote>",
                TagEnd::Link => "</a>",
                TagEnd::HtmlBlock => "</html>",
                TagEnd::Table => "</table>",
                TagEnd::TableHead => "</thead>",
                TagEnd::TableRow => "</tr>",
                TagEnd::TableCell => "</td>",
                TagEnd::DefinitionList => "</dl>",
                TagEnd::DefinitionListTitle => "</dt>",
                TagEnd::DefinitionListDefinition => "</dd>",
                tag => panic!("unexpected {tag:?}"),
            }),
            Event::Text(text) => out.push_str(&text),
            Event::Code(code) => {
                out.push_str(&format!("<code{}>{code}</code>", attrs(&attributes)))
            }
            Event::Html(html) | Event::InlineHtml(html) => out.push_str(&html),
            Event::TaskListMarker(done) => out.push_str(if done { "[x]" } else { "[ ]" }),
            Event::InlineMath(math) => out.push_str(&format!("\\({math}\\)")),
            Event::DisplayMath(math) => out.push_str(&format!("\\[{}\\]", math.trim())),
            Event::SoftBreak => out.push('\n'),
            Event::Rule => out.push_str(&format!("<hr{}>", attrs(&attributes))),
            Event::FootnoteReference(label) => out.push_str(&format!("[^{label}]")),
            event => panic!("unexpected {event:?}"),
        }
    }
    out
}

fn attrs(attributes: &Attributes) -> String {
    let mut out = String::new();
    if !attributes.classes.is_empty() {
        out.push_str(&format!(" class=\"{}\"", attributes.classes.join(" ")));
    }
    if let Some(id) = &attributes.id {
        out.push_str(&format!(" id=\"{id}\""));
    }
    for (key, value) in &attributes.pairs {
        out.push_str(&format!(" {key}=\"{value}\""));
    }
    out
}

/// One of kramdown's extensions, and the others a case needs alongside it.
#[derive(Clone, Copy)]
struct Extension {
    extension: Extensions,
    needs: Extensions,
}

impl Extension {
    fn new(extension: Extensions) -> Self {
        Self {
            extension,
            needs: Extensions::empty(),
        }
    }

    /// The same extension, read along with `others`.
    fn needs(self, others: Extensions) -> Self {
        Self {
            needs: self.needs | others,
            ..self
        }
    }

    /// Checks that `markdown` reads as `with` given only this extension and
    /// what it needs, and as `without` given every extension but this one.
    #[track_caller]
    fn reads(self, markdown: &str, with: &str, without: &str) {
        let only = outline(markdown, self.extension | self.needs);
        let rest = outline(markdown, Extensions::all() - self.extension);
        assert_eq!(only, with, "with the extension");
        assert_eq!(rest, without, "without the extension");
    }
}

#[test]
fn tables() {
    let extension = Extension::new(Extensions::ENABLE_TABLES);
    extension.reads(
        "a | b\n-|-\n1 | 2\n",
        "<table><thead><td>a</td><td>b</td></thead><tr><td>1</td><td>2</td></tr></table>",
        "<p>a | b\n-|-\n1 | 2</p>",
    );
}

#[test]
fn footnotes() {
    let extension = Extension::new(Extensions::ENABLE_FOOTNOTES);
    extension.reads(
        "a[^1]\n\n[^1]: b\n",
        "<p>a[^1]</p><footnote label=\"1\"><p>b</p></footnote>",
        "<p>a<a href=\"b\">^1</a></p>",
    );
}

#[test]
fn strikethrough() {
    let extension = Extension::new(Extensions::ENABLE_STRIKETHROUGH);
    extension.reads("~~a~~\n", "<p><del>a</del></p>", "<p>~~a~~</p>");
    // A single tilde is text, as in kramdown.
    extension.reads(
        "~a~ and ~~b ~c~ d~~\n",
        "<p>~a~ and <del>b ~c~ d</del></p>",
        "<p>~a~ and ~~b ~c~ d~~</p>",
    );
}

#[test]
fn task_lists() {
    let extension = Extension::new(Extensions::ENABLE_TASK_LISTS);
    extension.reads(
        "* [x] a\n* [ ] b\n",
        "<ul><li>[x]a</li><li>[ ]b</li></ul>",
        "<ul><li>[x] a</li><li>[ ] b</li></ul>",
    );
}

#[test]
fn typography() {
    let extension = Extension::new(Extensions::ENABLE_TYPOGRAPHY);
    // Header ids leave out dashes, as kramdown's do.
    extension.needs(Extensions::ENABLE_AUTO_IDS).reads(
        "# a -- b\n",
        "<h1 id=\"a--b\">a – b</h1>",
        "<h1 id=\"a----b\">a -- b</h1>",
    );
    extension.needs(Extensions::ENABLE_AUTO_IDS).reads(
        "## Two \"quoted\" -- dash...\n",
        "<h2 id=\"two-quoted--dash\">Two “quoted” – dash…</h2>",
        "<h2 id=\"two-quoted----dash\">Two \"quoted\" -- dash...</h2>",
    );
}

#[test]
fn definition_lists() {
    let extension = Extension::new(Extensions::ENABLE_DEFINITION_LISTS);
    // A term on each line, and a definition's first paragraph bare unless a blank line comes before it.
    extension.reads(
        "apple\n: red\n\n  more of red\n\npear\nquince\n: fruit\n\n: after\n",
        "<dl><dt>apple</dt><dd>red<p>more of red</p></dd><dt>pear</dt><dt>quince</dt><dd>fruit</dd><dd><p>after</p></dd></dl>",
        "<p>apple\n: red</p><p>more of red</p><p>pear\nquince\n: fruit</p><p>: after</p>",
    );
    extension.reads(
        "> a\n> : b\n>\n> : c\n",
        "<blockquote><dl><dt>a</dt><dd>b</dd><dd><p>c</p></dd></dl></blockquote>",
        "<blockquote><p>a\n: b</p><p>: c</p></blockquote>",
    );
}

#[test]
fn math() {
    let extension = Extension::new(Extensions::ENABLE_MATH);
    // A single `$` is text, and `$$` with more in its paragraph is inline.
    extension.reads(
        "Costs $5, or $x$ or $$y$$.\n",
        "<p>Costs $5, or $x$ or \\(y\\).</p>",
        "<p>Costs $5, or $x$ or $$y$$.</p>",
    );
    // kramdown's math block isn't in a paragraph, but pulldown-cmark's is.
    extension.reads(
        "$$\nx = 1\n$$\n",
        "<p>\\[x = 1\\]</p>",
        "<p>$$\nx = 1\n$$</p>",
    );
}

#[test]
fn block_attributes() {
    let extension = Extension::new(Extensions::ENABLE_BLOCK_ATTRIBUTES);
    // On the line after a block or before it.
    extension.reads(
        "para\n{:.x}\n",
        "<p class=\"x\">para</p>",
        "<p>para\n{:.x}</p>",
    );
    extension.reads(
        "{:.x}\npara\n",
        "<p class=\"x\">para</p>",
        "<p>{:.x}\npara</p>",
    );
    extension.reads(
        "## Notes\n{:.no_toc}\n\ntext\n",
        "<h2 id=\"\" class=\"no_toc\">Notes</h2><p>text</p>",
        "<h2 id=\"notes\">Notes</h2><p>{:.no_toc}</p><p>text</p>",
    );
    extension.reads("---\n{:.x}\n", "<hr class=\"x\">", "<hr><p>{:.x}</p>");
    // After blocks already read, and before more.
    extension.reads(
        "a\n\n> b\n\nc\n{:.x}\n\nd\n",
        "<p>a</p><blockquote><p>b</p></blockquote><p class=\"x\">c</p><p>d</p>",
        "<p>a</p><blockquote><p>b</p></blockquote><p>c\n{:.x}</p><p>d</p>",
    );
    // Alone between blocks, for the next one.
    extension.reads(
        "para\n\n{:.x}\n\nafter\n",
        "<p>para</p><p class=\"x\">after</p>",
        "<p>para</p><p>{:.x}</p><p>after</p>",
    );
    extension.reads(
        "para\n\n{:.x}\n\n## H\n",
        "<p>para</p><h2 id=\"\" class=\"x\">H</h2>",
        "<p>para</p><p>{:.x}</p><h2 id=\"h\">H</h2>",
    );
    extension.reads("para\n\n{:.x}\n", "<p>para</p>", "<p>para</p><p>{:.x}</p>");
    extension.reads(
        "a\n\nb\n\n{:.x}\n\n{:.y}\n\nc\n",
        "<p>a</p><p>b</p><p class=\"x y\">c</p>",
        "<p>a</p><p>b</p><p>{:.x}</p><p>{:.y}</p><p>c</p>",
    );
    // Ending a paragraph.
    extension.reads(
        "para\n{:.x}\nmore\n",
        "<p class=\"x\">para</p><p>more</p>",
        "<p>para\n{:.x}\nmore</p>",
    );
    extension.reads(
        "para\n{:.x}\n{:.y}\nmore *a*\n",
        "<p class=\"x y\">para</p><p>more <em>a</em></p>",
        "<p>para\n{:.x}\n{:.y}\nmore <em>a</em></p>",
    );
    // Lazy, for the outermost container.
    extension.reads(
        "> q\n{:.x}\n",
        "<blockquote class=\"x\"><p>q</p></blockquote>",
        "<blockquote><p>q\n{:.x}</p></blockquote>",
    );
    extension.reads(
        "> > q\n{:.x}\n",
        "<blockquote class=\"x\"><blockquote><p>q</p></blockquote></blockquote>",
        "<blockquote><blockquote><p>q\n{:.x}</p></blockquote></blockquote>",
    );
    extension.reads(
        "- a\n\n  b\n{:.x}\n",
        "<ul class=\"x\"><li><p>a</p><p>b</p></li></ul>",
        "<ul><li><p>a</p><p>b\n{:.x}</p></li></ul>",
    );
    // Touching nothing, or in code, they're text.
    extension.reads("para {: .x}\n", "<p>para {: .x}</p>", "<p>para {: .x}</p>");
    extension.reads(
        "```\n{:.x}\n```\n",
        "<pre>{:.x}\n</pre>",
        "<pre>{:.x}\n</pre>",
    );
    // After a table, for it.
    extension.needs(Extensions::ENABLE_TABLES).reads(
        "a | b\n--|--\n1 | 2\n{:.x}\n",
        "<table class=\"x\"><thead><td>a</td><td>b</td></thead><tr><td>1</td><td>2</td></tr></table>",
        "<table><thead><td>a</td><td>b</td></thead><tr><td>1</td><td>2</td></tr><tr><td>{:.x}</td><td></td></tr></table>",
    );
    // Beside other cells, text.
    extension.needs(Extensions::ENABLE_TABLES).reads(
        "a | b\n--|--\n{:.x} | 2\n",
        "<table><thead><td>a</td><td>b</td></thead><tr><td>{:.x}</td><td>2</td></tr></table>",
        "<table><thead><td>a</td><td>b</td></thead><tr><td>{:.x}</td><td>2</td></tr></table>",
    );
}

#[test]
fn span_attributes() {
    let extension = Extension::new(Extensions::ENABLE_SPAN_ATTRIBUTES);
    extension.reads(
        "`x`{:.hl} and *e*{: #i k=\"v w\"}\n",
        "<p><code class=\"hl\">x</code> and <em id=\"i\" k=\"v w\">e</em></p>",
        "<p><code>x</code>{:.hl} and <em>e</em>{: #i k=“v w”}</p>",
    );
    // The empty list separates spans.
    extension.reads(
        "*is italic*{::}*marked*{:.special} text\n",
        "<p><em>is italic</em><em class=\"special\">marked</em> text</p>",
        "<p><em>is italic</em>{::}<em>marked</em>{:.special} text</p>",
    );
    // Not touching the span, they're text.
    extension.reads(
        "`code` {:.x}\n",
        "<p><code>code</code> {:.x}</p>",
        "<p><code>code</code> {:.x}</p>",
    );
    // Smart quotes split the text around quoted values.
    extension.needs(Extensions::ENABLE_TYPOGRAPHY).reads(
        "a [link](http://e.com){:hreflang=\"de\"} b\n",
        "<p>a <a href=\"http://e.com\" hreflang=\"de\">link</a> b</p>",
        "<p>a <a href=\"http://e.com\">link</a>{:hreflang=“de”} b</p>",
    );
}

#[test]
fn item_attributes() {
    let extension = Extension::new(Extensions::ENABLE_ITEM_ATTRIBUTES);
    extension.reads(
        "* {:.cls} item\n* b\n",
        "<ul><li class=\"cls\">item</li><li>b</li></ul>",
        "<ul><li>{:.cls} item</li><li>b</li></ul>",
    );
}

#[test]
fn header_ids() {
    let extension = Extension::new(Extensions::ENABLE_HEADER_IDS);
    extension.reads(
        "# A {#b}\n",
        "<h1 id=\"b\">A</h1>",
        "<h1 id=\"a-b\">A {#b}</h1>",
    );
    extension.reads(
        "# Hello #    {#custom}\n",
        "<h1 id=\"custom\">Hello</h1>",
        "<h1 id=\"hello-----custom\">Hello #    {#custom}</h1>",
    );
    // An attribute list isn't a header id.
    extension.needs(Extensions::ENABLE_AUTO_IDS).reads(
        "## H {: #id}\n",
        "<h2 id=\"h--id\">H {: #id}</h2>",
        "<h2 id=\"h--id\">H {: #id}</h2>",
    );
}

#[test]
fn auto_ids() {
    let extension = Extension::new(Extensions::ENABLE_AUTO_IDS);
    extension.reads("# A\n", "<h1 id=\"a\">A</h1>", "<h1 id=\"\">A</h1>");
    // Repeats are numbered, past ids already taken.
    extension.reads(
        "# One 1\n# One\n# One\n",
        "<h1 id=\"one-1\">One 1</h1><h1 id=\"one\">One</h1><h1 id=\"one-1\">One</h1>",
        "<h1 id=\"\">One 1</h1><h1 id=\"\">One</h1><h1 id=\"\">One</h1>",
    );
}

#[test]
fn markdown_attribute() {
    let extension = Extension::new(Extensions::ENABLE_MARKDOWN_ATTRIBUTE);
    extension.reads(
        "<div markdown=\"1\">\n*a*\n</div>\n",
        "<html><div markdown=\"1\">\n</html><p><em>a</em></p><html></div>\n</html>",
        "<html><div markdown=\"1\">\n*a*\n</div>\n</html>",
    );
}

#[test]
fn span_tag_paragraphs() {
    let extension = Extension::new(Extensions::ENABLE_SPAN_TAG_PARAGRAPHS);
    extension.reads(
        "<b>\n*a*\n",
        "<p><b> <em>a</em></p>",
        "<html><b>\n*a*\n</html>",
    );
}

#[test]
fn language_classes() {
    let extension = Extension::new(Extensions::ENABLE_LANGUAGE_CLASSES);
    extension.needs(Extensions::ENABLE_BLOCK_ATTRIBUTES).reads(
        "~~~\ndef x\n~~~\n{: .language-ruby}\n",
        "<pre lang=\"ruby\" class=\"language-ruby\">def x\n</pre>",
        "<pre class=\"language-ruby\">def x\n</pre>",
    );
}

#[test]
fn tables_of_contents() {
    let extension = Extension::new(Extensions::ENABLE_TABLES_OF_CONTENTS);
    // Nested by level.
    extension.needs(Extensions::ENABLE_BLOCK_ATTRIBUTES | Extensions::ENABLE_AUTO_IDS).reads(
        "1. TOC\n{:toc}\n\n## A\n### B\n#### C\n## D\n",
        "<ol><li><a href=\"#a\">A</a><ol><li><a href=\"#b\">B</a><ol><li><a href=\"#c\">C</a></li></ol></li></ol></li><li><a href=\"#d\">D</a></li></ol><h2 id=\"a\">A</h2><h3 id=\"b\">B</h3><h4 id=\"c\">C</h4><h2 id=\"d\">D</h2>",
        "<ol><li>TOC</li></ol><h2 id=\"a\">A</h2><h3 id=\"b\">B</h3><h4 id=\"c\">C</h4><h2 id=\"d\">D</h2>",
    );
    // Each lists every heading, before it or after.
    extension.needs(Extensions::ENABLE_BLOCK_ATTRIBUTES | Extensions::ENABLE_AUTO_IDS).reads(
        "* TOC\n{:toc}\n\n# A\n\n* TOC\n{:toc}\n\n# B\n",
        "<ul><li><a href=\"#a\">A</a></li><li><a href=\"#b\">B</a></li></ul><h1 id=\"a\">A</h1><ul><li><a href=\"#a\">A</a></li><li><a href=\"#b\">B</a></li></ul><h1 id=\"b\">B</h1>",
        "<ul><li>TOC</li></ul><h1 id=\"a\">A</h1><ul><li>TOC</li></ul><h1 id=\"b\">B</h1>",
    );
    // Leaving out links, footnotes and headings marked no_toc.
    extension.needs(Extensions::ENABLE_FOOTNOTES | Extensions::ENABLE_BLOCK_ATTRIBUTES | Extensions::ENABLE_HEADER_IDS | Extensions::ENABLE_AUTO_IDS).reads(
        "* TOC\n{:toc}\n\n## See [docs](http://x.com) *now* [^1]\n\n## Two\n{:.no_toc}\n\n# Hello #    {#custom}\n\n[^1]: note\n",
        "<ul><li><a href=\"#see-docs-now-\">See docs <em>now</em> </a></li><li><a href=\"#custom\">Hello</a></li></ul><h2 id=\"see-docs-now-\">See <a href=\"http://x.com\">docs</a> <em>now</em> [^1]</h2><h2 id=\"two\" class=\"no_toc\">Two</h2><h1 id=\"custom\">Hello</h1><footnote label=\"1\"><p>note</p></footnote>",
        "<ul><li>TOC</li></ul><h2 id=\"see-docs-now-\">See <a href=\"http://x.com\">docs</a> <em>now</em> [^1]</h2><h2 id=\"two\" class=\"no_toc\">Two</h2><h1 id=\"custom\">Hello</h1><footnote label=\"1\"><p>note</p></footnote>",
    );
}

#[test]
fn extensions_own_their_options() {
    use pulldown_kram::pulldown_cmark::{BlockQuoteKind, Options as Cmark, Parser};

    // An option for an extension is left to it.
    let options = Options::new(
        Extensions::all() - Extensions::ENABLE_TABLES,
        Cmark::ENABLE_TABLES,
    );
    assert_eq!(outline("a | b\n-|-\n", options), "<p>a | b\n-|-</p>");

    // The rest are added.
    let options = Options::new(Extensions::all(), Cmark::ENABLE_GFM);
    let document = Document::new("> [!NOTE]\n> a\n", options);
    assert!(matches!(
        document.events().next(),
        Some((Event::Start(Tag::BlockQuote(Some(BlockQuoteKind::Note))), _))
    ));

    // With nothing, it's pulldown-cmark's CommonMark.
    let markdown = "# A {#b}\n\n*c*{:.d} | e\n-|-\n";
    let document = Document::new(markdown, Options::from(Extensions::empty()));
    let events: Vec<_> = document
        .events()
        .map(|(event, attributes)| {
            assert!(attributes.is_empty());
            event
        })
        .collect();
    assert!(events.into_iter().eq(Parser::new(markdown)));
}

//! Documents read as kramdown 2.5.2 reads them with GFM input, Jekyll's
//! default. Each expected outline is kramdown's HTML for the same input,
//! cut down to the elements and attributes the events carry.

use pulldown_kram::pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use pulldown_kram::{Attributes, Document, Extensions, Options};

/// The document's events as HTML-like text, attributes included.
fn outline(markdown: &str) -> String {
    outline_with(markdown, Extensions::all())
}

/// [`outline`], read with `options`.
fn outline_with(markdown: &str, options: impl Into<Options>) -> String {
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

#[test]
fn block_lists_go_to_the_block_they_touch() {
    assert_eq!(outline("para\n{:.x}\n"), "<p class=\"x\">para</p>");
    assert_eq!(outline("{:.x}\npara\n"), "<p class=\"x\">para</p>");
    assert_eq!(
        outline("## Notes\n{:.no_toc}\n\ntext\n"),
        "<h2 id=\"notes\" class=\"no_toc\">Notes</h2><p>text</p>"
    );
    assert_eq!(
        outline("~~~\ndef x\n~~~\n{: .language-ruby}\n"),
        "<pre lang=\"ruby\" class=\"language-ruby\">def x\n</pre>"
    );
    assert_eq!(outline("---\n{:.x}\n"), "<hr class=\"x\">");
    // After blocks already read, and before more.
    assert_eq!(
        outline("a\n\n> b\n\nc\n{:.x}\n\nd\n"),
        "<p>a</p><blockquote><p>b</p></blockquote><p class=\"x\">c</p><p>d</p>"
    );
}

#[test]
fn extensions_read_by_pulldown_cmark() {
    assert_eq!(
        outline("~~a~~\n\n* [x] b\n"),
        "<p><del>a</del></p><ul><li>[x]b</li></ul>"
    );
    // Header ids leave out dashes, as kramdown's do.
    assert_eq!(outline("# a -- b\n"), "<h1 id=\"a--b\">a – b</h1>");
}

#[test]
fn single_tildes_and_dollars_are_text() {
    assert_eq!(
        outline("~a~ and ~~b ~c~ d~~\n"),
        "<p>~a~ and <del>b ~c~ d</del></p>"
    );
    assert_eq!(
        outline("Costs $5, or $x$ or $$y$$.\n"),
        "<p>Costs $5, or $x$ or \\(y\\).</p>"
    );
    // kramdown's math block isn't in a paragraph, but pulldown-cmark's is.
    assert_eq!(outline("$$\nx = 1\n$$\n"), "<p>\\[x = 1\\]</p>");
}

#[test]
fn definition_lists() {
    assert_eq!(
        outline("apple\n: red\n\n  more of red\n\npear\nquince\n: fruit\n\n: after\n"),
        "<dl><dt>apple</dt><dd>red<p>more of red</p></dd>\
        <dt>pear</dt><dt>quince</dt><dd>fruit</dd><dd><p>after</p></dd></dl>"
    );
    assert_eq!(
        outline("> a\n> : b\n>\n> : c\n"),
        "<blockquote><dl><dt>a</dt><dd>b</dd><dd><p>c</p></dd></dl></blockquote>"
    );
}

#[test]
fn block_lists_after_a_table_go_to_it() {
    let table = "<table class=\"x\"><thead><td>a</td><td>b</td></thead>\
        <tr><td>1</td><td>2</td></tr></table>";
    assert_eq!(outline("a | b\n--|--\n1 | 2\n{:.x}\n"), table);
    // Beside other cells, it's text.
    assert_eq!(
        outline("a | b\n--|--\n{:.x} | 2\n"),
        "<table><thead><td>a</td><td>b</td></thead>\
        <tr><td>{:.x}</td><td>2</td></tr></table>"
    );
}

#[test]
fn block_lists_alone_go_to_the_next_block() {
    assert_eq!(
        outline("para\n\n{:.x}\n\nafter\n"),
        "<p>para</p><p class=\"x\">after</p>"
    );
    assert_eq!(
        outline("para\n\n{:.x}\n\n## H\n"),
        "<p>para</p><h2 id=\"h\" class=\"x\">H</h2>"
    );
    assert_eq!(outline("para\n\n{:.x}\n"), "<p>para</p>");
    assert_eq!(
        outline("a\n\nb\n\n{:.x}\n\n{:.y}\n\nc\n"),
        "<p>a</p><p>b</p><p class=\"x y\">c</p>"
    );
}

#[test]
fn block_lists_end_paragraphs() {
    assert_eq!(
        outline("para\n{:.x}\nmore\n"),
        "<p class=\"x\">para</p><p>more</p>"
    );
    assert_eq!(
        outline("para\n{:.x}\n{:.y}\nmore *a*\n"),
        "<p class=\"x y\">para</p><p>more <em>a</em></p>"
    );
}

#[test]
fn lazy_block_lists_go_to_the_outermost_container() {
    assert_eq!(
        outline("> q\n{:.x}\n"),
        "<blockquote class=\"x\"><p>q</p></blockquote>"
    );
    assert_eq!(
        outline("> > q\n{:.x}\n"),
        "<blockquote class=\"x\"><blockquote><p>q</p></blockquote></blockquote>"
    );
    assert_eq!(
        outline("- a\n\n  b\n{:.x}\n"),
        "<ul class=\"x\"><li><p>a</p><p>b</p></li></ul>"
    );
}

#[test]
fn span_lists_go_to_the_span_before_them() {
    assert_eq!(
        outline("`x`{:.hl} and *e*{: #i k=\"v w\"}\n"),
        "<p><code class=\"hl\">x</code> and <em id=\"i\" k=\"v w\">e</em></p>"
    );
    // Smart quotes split the text around quoted values.
    assert_eq!(
        outline("a [link](http://e.com){:hreflang=\"de\"} b\n"),
        "<p>a <a href=\"http://e.com\" hreflang=\"de\">link</a> b</p>"
    );
    assert_eq!(
        outline("*is italic*{::}*marked*{:.special} text\n"),
        "<p><em>is italic</em><em class=\"special\">marked</em> text</p>"
    );
}

#[test]
fn lists_touching_nothing_stay_text() {
    assert_eq!(outline("`code` {:.x}\n"), "<p><code>code</code> {:.x}</p>");
    assert_eq!(outline("para {: .x}\n"), "<p>para {: .x}</p>");
    assert_eq!(outline("```\n{:.x}\n```\n"), "<pre>{:.x}\n</pre>");
}

#[test]
fn list_item_lists_go_to_the_item() {
    assert_eq!(
        outline("* {:.cls} item\n* b\n"),
        "<ul><li class=\"cls\">item</li><li>b</li></ul>"
    );
}

#[test]
fn header_ids() {
    assert_eq!(
        outline("# One 1\n# One\n# One\n"),
        "<h1 id=\"one-1\">One 1</h1><h1 id=\"one\">One</h1><h1 id=\"one-1\">One</h1>"
    );
    assert_eq!(
        outline("# Hello #    {#custom}\n"),
        "<h1 id=\"custom\">Hello</h1>"
    );
    assert_eq!(outline("## H {: #id}\n"), "<h2 id=\"h--id\">H {: #id}</h2>");
    assert_eq!(
        outline("## Two \"quoted\" -- dash\n"),
        "<h2 id=\"two-quoted--dash\">Two “quoted” – dash</h2>"
    );
}

#[test]
fn tables_of_contents() {
    let markdown = "* TOC\n{:toc}\n\n## See [docs](http://x.com) *now* [^1]\n\n## Two\n{:.no_toc}\n\n# Hello #    {#custom}\n\n### Deep\n\n[^1]: note\n";
    let toc = "<ul>\
        <li><a href=\"#see-docs-now-\">See docs <em>now</em> </a></li>\
        <li><a href=\"#custom\">Hello</a><ul>\
        <li><a href=\"#deep\">Deep</a></li>\
        </ul></li>\
        </ul>";
    assert!(outline(markdown).starts_with(toc), "{}", outline(markdown));

    let markdown = "1. TOC\n{:toc}\n\n## A\n### B\n#### C\n## D\n";
    let toc = "<ol>\
        <li><a href=\"#a\">A</a><ol>\
        <li><a href=\"#b\">B</a><ol>\
        <li><a href=\"#c\">C</a></li>\
        </ol></li>\
        </ol></li>\
        <li><a href=\"#d\">D</a></li>\
        </ol>";
    assert!(outline(markdown).starts_with(toc), "{}", outline(markdown));

    // Each table lists every heading, before it or after.
    let markdown = "* TOC\n{:toc}\n\n# A\n\n* TOC\n{:toc}\n\n# B\n";
    let toc = "<ul>\
        <li><a href=\"#a\">A</a></li>\
        <li><a href=\"#b\">B</a></li>\
        </ul>";
    assert_eq!(
        outline(markdown),
        format!("{toc}<h1 id=\"a\">A</h1>{toc}<h1 id=\"b\">B</h1>")
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
    assert_eq!(outline_with("a | b\n-|-\n", options), "<p>a | b\n-|-</p>");

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

#[test]
fn each_extension_can_be_left_off() {
    let without = |extension| Extensions::all() - extension;
    assert_eq!(
        outline_with("a | b\n-|-\n", without(Extensions::ENABLE_TABLES)),
        "<p>a | b\n-|-</p>"
    );
    assert_eq!(
        outline_with("a[^1]\n\n[^1]: b\n", without(Extensions::ENABLE_FOOTNOTES)),
        "<p>a<a href=\"b\">^1</a></p>"
    );
    assert_eq!(
        outline_with("~~a~~\n", without(Extensions::ENABLE_STRIKETHROUGH)),
        "<p>~~a~~</p>"
    );
    assert_eq!(
        outline_with("* [x] a\n", without(Extensions::ENABLE_TASK_LISTS)),
        "<ul><li>[x] a</li></ul>"
    );
    assert_eq!(
        outline_with("# a -- b\n", without(Extensions::ENABLE_TYPOGRAPHY)),
        "<h1 id=\"a----b\">a -- b</h1>"
    );
    assert_eq!(
        outline_with("a\n: b\n", without(Extensions::ENABLE_DEFINITION_LISTS)),
        "<p>a\n: b</p>"
    );
    assert_eq!(
        outline_with("$$x$$\n", without(Extensions::ENABLE_MATH)),
        "<p>$$x$$</p>"
    );
    assert_eq!(
        outline_with(
            "para\n{:.x}\n",
            without(Extensions::ENABLE_BLOCK_ATTRIBUTES)
        ),
        "<p>para\n{:.x}</p>"
    );
    assert_eq!(
        outline_with("*a*{:.x}\n", without(Extensions::ENABLE_SPAN_ATTRIBUTES)),
        "<p><em>a</em>{:.x}</p>"
    );
    assert_eq!(
        outline_with("* {:.x} a\n", without(Extensions::ENABLE_ITEM_ATTRIBUTES)),
        "<ul><li>{:.x} a</li></ul>"
    );
    assert_eq!(
        outline_with("# A {#b}\n", without(Extensions::ENABLE_HEADER_IDS)),
        "<h1 id=\"a-b\">A {#b}</h1>"
    );
    assert_eq!(
        outline_with("# A\n", without(Extensions::ENABLE_AUTO_IDS)),
        "<h1 id=\"\">A</h1>"
    );
    assert_eq!(
        outline_with(
            "<div markdown=\"1\">\n*a*\n</div>\n",
            without(Extensions::ENABLE_MARKDOWN_ATTRIBUTE)
        ),
        "<html><div markdown=\"1\">\n*a*\n</div>\n</html>"
    );
    assert_eq!(
        outline_with(
            "<b>\n*a*\n",
            without(Extensions::ENABLE_SPAN_TAG_PARAGRAPHS)
        ),
        "<html><b>\n*a*\n</html>"
    );
    assert_eq!(
        outline_with(
            "~~~\nx\n~~~\n{: .language-ruby}\n",
            without(Extensions::ENABLE_LANGUAGE_CLASSES)
        ),
        "<pre class=\"language-ruby\">x\n</pre>"
    );
    assert_eq!(
        outline_with(
            "* TOC\n{:toc}\n\n# A\n",
            without(Extensions::ENABLE_TABLES_OF_CONTENTS)
        ),
        "<ul><li>TOC</li></ul><h1 id=\"a\">A</h1>"
    );
}

//! Documents read as kramdown 2.5.2 reads them with GFM input, Jekyll's
//! default. Each case reads a document twice: with one extension, and the
//! few it builds on, to show what that extension does, and with every
//! extension but it, to show what's left without it. Each is written out
//! with `pulldown_kram::html`, as kramdown writes HTML. With the extension,
//! it's kramdown's HTML for the same input, less what the extensions left
//! off would add, like header ids.

use pulldown_kram::pulldown_cmark::{CodeBlockKind, Event, Tag};
use pulldown_kram::{Document, Extensions, Options, html};

/// The document as HTML.
fn html(markdown: &str, options: impl Into<Options>) -> String {
    let mut out = String::new();
    html::push_html(&mut out, Document::new(markdown, options).events());
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
        let only = html(markdown, self.extension | self.needs);
        let rest = html(markdown, Extensions::all() - self.extension);
        assert_eq!(only, with, "with the extension");
        assert_eq!(rest, without, "without the extension");
    }
}

#[test]
fn tables() {
    let extension = Extension::new(Extensions::ENABLE_TABLES);
    extension.reads(
        "a | b\n-|-\n1 | 2\n",
        "<table><thead><tr><th>a</th><th>b</th></tr></thead>\n<tbody><tr><td>1</td><td>2</td></tr>\n</tbody></table>\n",
        "<p>a | b\n-|-\n1 | 2</p>\n",
    );
}

#[test]
fn footnotes() {
    let extension = Extension::new(Extensions::ENABLE_FOOTNOTES);
    extension.reads(
        "a[^1]\n\n[^1]: b\n",
        "<p>a<sup id=\"fnref:1\"><a href=\"#fn:1\" class=\"footnote\" rel=\"footnote\" role=\"doc-noteref\">1</a></sup></p>\n<div class=\"footnotes\" role=\"doc-endnotes\">\n<ol>\n<li id=\"fn:1\">\n<p>b\u{a0}<a href=\"#fnref:1\" class=\"reversefootnote\" role=\"doc-backlink\">↩</a></p>\n</li>\n</ol>\n</div>\n",
        "<p>a<a href=\"b\">^1</a></p>\n",
    );
}

#[test]
fn strikethrough() {
    let extension = Extension::new(Extensions::ENABLE_STRIKETHROUGH);
    extension.reads("~~a~~\n", "<p><del>a</del></p>\n", "<p>~~a~~</p>\n");
    // A single tilde is text, as in kramdown.
    extension.reads(
        "~a~ and ~~b ~c~ d~~\n",
        "<p>~a~ and <del>b ~c~ d</del></p>\n",
        "<p>~a~ and ~~b ~c~ d~~</p>\n",
    );
}

#[test]
fn task_lists() {
    let extension = Extension::new(Extensions::ENABLE_TASK_LISTS);
    extension.reads(
        "* [x] a\n* [ ] b\n",
        "<ul class=\"task-list\"><li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"disabled\" checked=\"checked\" />a</li>\n<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"disabled\" />b</li>\n</ul>\n",
        "<ul><li>[x] a</li>\n<li>[ ] b</li>\n</ul>\n",
    );
    // Items are marked from the first task on, and the list for having one.
    extension.reads(
        "* a\n* [x] b\n* c\n",
        "<ul class=\"task-list\"><li>a</li>\n<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"disabled\" checked=\"checked\" />b</li>\n<li class=\"task-list-item\">c</li>\n</ul>\n",
        "<ul><li>a</li>\n<li>[x] b</li>\n<li>c</li>\n</ul>\n",
    );
}

#[test]
fn typography() {
    let extension = Extension::new(Extensions::ENABLE_TYPOGRAPHY);
    // Header ids leave out dashes, as kramdown's do.
    extension.needs(Extensions::ENABLE_AUTO_IDS).reads(
        "# a -- b\n",
        "<h1 id=\"a--b\">a – b</h1>\n",
        "<h1 id=\"a----b\">a -- b</h1>\n",
    );
    extension.needs(Extensions::ENABLE_AUTO_IDS).reads(
        "## Two \"quoted\" -- dash...\n",
        "<h2 id=\"two-quoted--dash\">Two “quoted” – dash…</h2>\n",
        "<h2 id=\"two-quoted----dash\">Two &quot;quoted&quot; -- dash...</h2>\n",
    );
}

#[test]
fn link_definition_lists() {
    let extension = Extension::new(Extensions::ENABLE_DEFINITION_LISTS);
    // A term on each line, and a definition's first paragraph bare unless a blank line comes before it.
    extension.reads(
        "apple\n: red\n\n  more of red\n\npear\nquince\n: fruit\n\n: after\n",
        "<dl><dt>apple</dt>\n<dd>red<p>more of red</p>\n</dd>\n<dt>pear</dt>\n<dt>quince</dt>\n<dd>fruit</dd>\n<dd><p>after</p>\n</dd>\n</dl>\n",
        "<p>apple\n: red</p>\n<p>more of red</p>\n<p>pear\nquince\n: fruit</p>\n<p>: after</p>\n",
    );
    extension.reads(
        "> a\n> : b\n>\n> : c\n",
        "<blockquote><dl><dt>a</dt>\n<dd>b</dd>\n<dd><p>c</p>\n</dd>\n</dl>\n</blockquote>\n",
        "<blockquote><p>a\n: b</p>\n<p>: c</p>\n</blockquote>\n",
    );
    // More than one blank line after the terms starts no definition, but
    // after a definition, any number may come before the next, even after
    // an empty one.
    extension.reads(
        "a\n\n\n: b\n",
        "<p>a</p>\n<p>: b</p>\n",
        "<p>a</p>\n<p>: b</p>\n",
    );
    extension.reads(
        "a\n: b\n\n\n: c\n\nd\n: \n\ne\n: f\n",
        "<dl><dt>a</dt>\n<dd>b</dd>\n<dd><p>c</p>\n</dd>\n<dt>d</dt>\n<dd></dd>\n<dt>e</dt>\n<dd>f</dd>\n</dl>\n",
        "<p>a\n: b</p>\n<p>: c</p>\n<p>d\n:</p>\n<p>e\n: f</p>\n",
    );
    // With nothing on its first line, a definition is indented by four.
    extension.reads(
        "a\n: \n        code\n",
        "<dl><dt>a</dt>\n<dd><pre><code>code\n</code></pre>\n</dd>\n</dl>\n",
        "<p>a\n:\ncode</p>\n",
    );
}

#[test]
fn math() {
    let extension = Extension::new(Extensions::ENABLE_MATH);
    // A single `$` is text, and `$$` with more in its paragraph is inline.
    extension.reads(
        "Costs $5, or $x$ or $$y$$.\n",
        "<p>Costs $5, or $x$ or \\(y\\).</p>\n",
        "<p>Costs $5, or $x$ or $$y$$.</p>\n",
    );
    // Alone in its paragraph, it's a block, which kramdown writes bare.
    // Either way, it's without the whitespace around it.
    extension.reads("$$\nx = 1\n$$\n", "\\[x = 1\\]\n", "<p>$$\nx = 1\n$$</p>\n");
    extension.reads("a $$ y $$\n", "<p>a \\(y\\)</p>\n", "<p>a $$ y $$</p>\n");
    // As it is when attribute lists on the next line end its paragraph.
    extension.needs(Extensions::ENABLE_BLOCK_ATTRIBUTES).reads(
        "$$x$$\n{:.m}\nmore\n",
        "<div class=\"m\">\\[x\\]\n</div>\n<p>more</p>\n",
        "<p class=\"m\">$$x$$</p>\n<p>more</p>\n",
    );
}

#[test]
fn block_attributes() {
    let extension = Extension::new(Extensions::ENABLE_BLOCK_ATTRIBUTES);
    // On the line after a block or before it.
    extension.reads(
        "para\n{:.x}\n",
        "<p class=\"x\">para</p>\n",
        "<p>para\n{:.x}</p>\n",
    );
    extension.reads(
        "{:.x}\npara\n",
        "<p class=\"x\">para</p>\n",
        "<p>{:.x}\npara</p>\n",
    );
    extension.reads(
        "## Notes\n{:.no_toc}\n\ntext\n",
        "<h2 class=\"no_toc\">Notes</h2>\n<p>text</p>\n",
        "<h2 id=\"notes\">Notes</h2>\n<p>{:.no_toc}</p>\n<p>text</p>\n",
    );
    extension.reads(
        "---\n{:.x}\n",
        "<hr class=\"x\" />\n",
        "<hr />\n<p>{:.x}</p>\n",
    );
    // Code right after them is a block of its own, not more of their line.
    extension.reads(
        "{:.x}\n    code\n",
        "<pre class=\"x\"><code>code\n</code></pre>\n",
        "<p>{:.x}\ncode</p>\n",
    );
    extension.reads(
        "    a\n{:.x}\n    b\n",
        "<pre class=\"x\"><code>a\n</code></pre>\n<pre><code>b\n</code></pre>\n",
        "<pre><code>a\n</code></pre>\n<p>{:.x}\nb</p>\n",
    );
    // After blocks already read, and before more.
    extension.reads(
        "a\n\n> b\n\nc\n{:.x}\n\nd\n",
        "<p>a</p>\n<blockquote><p>b</p>\n</blockquote>\n<p class=\"x\">c</p>\n<p>d</p>\n",
        "<p>a</p>\n<blockquote><p>b</p>\n</blockquote>\n<p>c\n{:.x}</p>\n<p>d</p>\n",
    );
    // Alone between blocks, for the next one.
    extension.reads(
        "para\n\n{:.x}\n\nafter\n",
        "<p>para</p>\n<p class=\"x\">after</p>\n",
        "<p>para</p>\n<p>{:.x}</p>\n<p>after</p>\n",
    );
    extension.reads(
        "para\n\n{:.x}\n\n## H\n",
        "<p>para</p>\n<h2 class=\"x\">H</h2>\n",
        "<p>para</p>\n<p>{:.x}</p>\n<h2 id=\"h\">H</h2>\n",
    );
    extension.reads(
        "para\n\n{:.x}\n",
        "<p>para</p>\n",
        "<p>para</p>\n<p>{:.x}</p>\n",
    );
    extension.reads(
        "a\n\nb\n\n{:.x}\n\n{:.y}\n\nc\n",
        "<p>a</p>\n<p>b</p>\n<p class=\"x y\">c</p>\n",
        "<p>a</p>\n<p>b</p>\n<p>{:.x}</p>\n<p>{:.y}</p>\n<p>c</p>\n",
    );
    // Ending a paragraph.
    extension.reads(
        "para\n{:.x}\nmore\n",
        "<p class=\"x\">para</p>\n<p>more</p>\n",
        "<p>para\n{:.x}\nmore</p>\n",
    );
    extension.reads(
        "para\n{:.x}\n{:.y}\nmore *a*\n",
        "<p class=\"x y\">para</p>\n<p>more <em>a</em></p>\n",
        "<p>para\n{:.x}\n{:.y}\nmore <em>a</em></p>\n",
    );
    // Lazy, for the outermost container.
    extension.reads(
        "> q\n{:.x}\n",
        "<blockquote class=\"x\"><p>q</p>\n</blockquote>\n",
        "<blockquote><p>q\n{:.x}</p>\n</blockquote>\n",
    );
    extension.reads(
        "> > q\n{:.x}\n",
        "<blockquote class=\"x\"><blockquote><p>q</p>\n</blockquote>\n</blockquote>\n",
        "<blockquote><blockquote><p>q\n{:.x}</p>\n</blockquote>\n</blockquote>\n",
    );
    // A footnote's, which kramdown doesn't write, rather than its last
    // paragraph's.
    extension.needs(Extensions::ENABLE_FOOTNOTES).reads(
        "a[^1]\n\n[^1]: b\n{: .x}\n",
        "<p>a<sup id=\"fnref:1\"><a href=\"#fn:1\" class=\"footnote\" rel=\"footnote\" role=\"doc-noteref\">1</a></sup></p>\n<div class=\"footnotes\" role=\"doc-endnotes\">\n<ol>\n<li id=\"fn:1\">\n<p>b\u{a0}<a href=\"#fnref:1\" class=\"reversefootnote\" role=\"doc-backlink\">↩</a></p>\n</li>\n</ol>\n</div>\n",
        "<p>a<sup id=\"fnref:1\"><a href=\"#fn:1\" class=\"footnote\" rel=\"footnote\" role=\"doc-noteref\">1</a></sup></p>\n<div class=\"footnotes\" role=\"doc-endnotes\">\n<ol>\n<li id=\"fn:1\">\n<p>b\n{: .x}\u{a0}<a href=\"#fnref:1\" class=\"reversefootnote\" role=\"doc-backlink\">↩</a></p>\n</li>\n</ol>\n</div>\n",
    );
    extension.reads(
        "- a\n\n  b\n{:.x}\n",
        "<ul class=\"x\"><li><p>a</p>\n<p>b</p>\n</li>\n</ul>\n",
        "<ul><li><p>a</p>\n<p>b\n{:.x}</p>\n</li>\n</ul>\n",
    );
    // Touching nothing, or in code, they're text.
    extension.reads(
        "para {: .x}\n",
        "<p>para {: .x}</p>\n",
        "<p>para {: .x}</p>\n",
    );
    extension.reads(
        "```\n{:.x}\n```\n",
        "<pre><code>{:.x}\n</code></pre>\n",
        "<pre><code>{:.x}\n</code></pre>\n",
    );
    // After a table, for it.
    extension.needs(Extensions::ENABLE_TABLES).reads(
        "a | b\n--|--\n1 | 2\n{:.x}\n",
        "<table class=\"x\"><thead><tr><th>a</th><th>b</th></tr></thead>\n<tbody><tr><td>1</td><td>2</td></tr>\n</tbody></table>\n",
        "<table><thead><tr><th>a</th><th>b</th></tr></thead>\n<tbody><tr><td>1</td><td>2</td></tr>\n<tr><td>{:.x}</td><td>\u{a0}</td></tr>\n</tbody></table>\n",
    );
    // Beside other cells, text.
    extension.needs(Extensions::ENABLE_TABLES).reads(
        "a | b\n--|--\n{:.x} | 2\n",
        "<table><thead><tr><th>a</th><th>b</th></tr></thead>\n<tbody><tr><td>{:.x}</td><td>2</td></tr>\n</tbody></table>\n",
        "<table><thead><tr><th>a</th><th>b</th></tr></thead>\n<tbody><tr><td>{:.x}</td><td>2</td></tr>\n</tbody></table>\n",
    );
    // Before a definition list's first term, or lazy after it, for the list.
    extension.needs(Extensions::ENABLE_DEFINITION_LISTS).reads(
        "{:.x}\na\n: b\n{:.y}\n",
        "<dl class=\"x y\"><dt>a</dt>\n<dd>b</dd>\n</dl>\n",
        "<dl><dt>{:.x}</dt>\n<dt>a</dt>\n<dd>b\n{:.y}</dd>\n</dl>\n",
    );
    extension.needs(Extensions::ENABLE_DEFINITION_LISTS).reads(
        "a\n: b\n\n  c\n{:.x}\n",
        "<dl class=\"x\"><dt>a</dt>\n<dd>b<p>c</p>\n</dd>\n</dl>\n",
        "<dl><dt>a</dt>\n<dd>b<p>c\n{:.x}</p>\n</dd>\n</dl>\n",
    );
    // Before or after a link definition, for the links and images using
    // it, and not the block after.
    extension.reads(
        "[a] and ![b][a]\n\n{:.x}\n[a]: /u\n{:.y}\n\ntext\n",
        "<p><a class=\"x y\" href=\"/u\">a</a> and <img class=\"x y\" src=\"/u\" alt=\"b\" /></p>\n<p>text</p>\n",
        "<p>[a] and ![b][a]</p>\n<p>{:.x}\n[a]: /u\n{:.y}</p>\n<p>text</p>\n",
    );
}

#[test]
fn span_attributes() {
    let extension = Extension::new(Extensions::ENABLE_SPAN_ATTRIBUTES);
    extension.reads(
        "`x`{:.hl} and *e*{: #i k=\"v w\"}\n",
        "<p><code class=\"hl\">x</code> and <em id=\"i\" k=\"v w\">e</em></p>\n",
        "<p><code>x</code>{:.hl} and <em>e</em>{: #i k=“v w”}</p>\n",
    );
    // The empty list separates spans.
    extension.reads(
        "*is italic*{::}*marked*{:.special} text\n",
        "<p><em>is italic</em><em class=\"special\">marked</em> text</p>\n",
        "<p><em>is italic</em>{::}<em>marked</em>{:.special} text</p>\n",
    );
    // Not touching the span, they're text.
    extension.reads(
        "`code` {:.x}\n",
        "<p><code>code</code> {:.x}</p>\n",
        "<p><code>code</code> {:.x}</p>\n",
    );
    // Smart quotes split the text around quoted values.
    extension.needs(Extensions::ENABLE_TYPOGRAPHY).reads(
        "a [link](http://e.com){:hreflang=\"de\"} b\n",
        "<p>a <a href=\"http://e.com\" hreflang=\"de\">link</a> b</p>\n",
        "<p>a <a href=\"http://e.com\">link</a>{:hreflang=“de”} b</p>\n",
    );
}

#[test]
fn item_attributes() {
    let extension = Extension::new(Extensions::ENABLE_ITEM_ATTRIBUTES);
    extension.reads(
        "* {:.cls} item\n* b\n",
        "<ul><li class=\"cls\">item</li>\n<li>b</li>\n</ul>\n",
        "<ul><li>{:.cls} item</li>\n<li>b</li>\n</ul>\n",
    );
}

#[test]
fn header_ids() {
    let extension = Extension::new(Extensions::ENABLE_HEADER_IDS);
    extension.reads(
        "# A {#b}\n",
        "<h1 id=\"b\">A</h1>\n",
        "<h1 id=\"a-b\">A {#b}</h1>\n",
    );
    extension.reads(
        "# Hello #    {#custom}\n",
        "<h1 id=\"custom\">Hello</h1>\n",
        "<h1 id=\"hello-----custom\">Hello #    {#custom}</h1>\n",
    );
    // An attribute list isn't a header id.
    extension.needs(Extensions::ENABLE_AUTO_IDS).reads(
        "## H {: #id}\n",
        "<h2 id=\"h--id\">H {: #id}</h2>\n",
        "<h2 id=\"h--id\">H {: #id}</h2>\n",
    );
}

#[test]
fn auto_ids() {
    let extension = Extension::new(Extensions::ENABLE_AUTO_IDS);
    extension.reads("# A\n", "<h1 id=\"a\">A</h1>\n", "<h1>A</h1>\n");
    // Repeats are numbered, past ids already taken.
    extension.reads(
        "# One 1\n# One\n# One\n",
        "<h1 id=\"one-1\">One 1</h1>\n<h1 id=\"one\">One</h1>\n<h1 id=\"one-1\">One</h1>\n",
        "<h1>One 1</h1>\n<h1>One</h1>\n<h1>One</h1>\n",
    );
}

#[test]
fn markdown_attribute() {
    let extension = Extension::new(Extensions::ENABLE_MARKDOWN_ATTRIBUTE);
    // The attribute is read, and left out, as in kramdown.
    extension.reads(
        "<div markdown=\"1\">\n*a*\n</div>\n",
        "<div>\n<p><em>a</em></p>\n</div>\n",
        "<div markdown=\"1\">\n*a*\n</div>\n",
    );
}

#[test]
fn span_tag_paragraphs() {
    let extension = Extension::new(Extensions::ENABLE_SPAN_TAG_PARAGRAPHS);
    extension.reads("<b>\n*a*\n", "<p><b> <em>a</em></p>\n", "<b>\n*a*\n");
}

#[test]
fn language_classes() {
    // The HTML is the same either way, as the class goes on the code, so
    // the extension shows in the event: the fence given the class's language.
    let markdown = "~~~\ndef x\n~~~\n{: .language-ruby}\n";
    let language = |extensions: Extensions| match Document::new(markdown, extensions)
        .events()
        .next()
    {
        Some((Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))), _)) => info.to_string(),
        event => panic!("unexpected {event:?}"),
    };
    let extension = Extensions::ENABLE_LANGUAGE_CLASSES;
    assert_eq!(
        language(extension | Extensions::ENABLE_BLOCK_ATTRIBUTES),
        "ruby"
    );
    assert_eq!(language(Extensions::all() - extension), "");
    assert_eq!(
        html(markdown, Extensions::all()),
        "<pre><code class=\"language-ruby\">def x\n</code></pre>\n"
    );
}

#[test]
fn tables_of_contents() {
    let extension = Extension::new(Extensions::ENABLE_TABLES_OF_CONTENTS);
    // Nested by level.
    extension.needs(Extensions::ENABLE_BLOCK_ATTRIBUTES | Extensions::ENABLE_AUTO_IDS).reads(
        "1. TOC\n{:toc}\n\n## A\n### B\n#### C\n## D\n",
        "<ol><li><a href=\"#a\">A</a><ol><li><a href=\"#b\">B</a><ol><li><a href=\"#c\">C</a></li>\n</ol>\n</li>\n</ol>\n</li>\n<li><a href=\"#d\">D</a></li>\n</ol>\n<h2 id=\"a\">A</h2>\n<h3 id=\"b\">B</h3>\n<h4 id=\"c\">C</h4>\n<h2 id=\"d\">D</h2>\n",
        "<ol><li>TOC</li>\n</ol>\n<h2 id=\"a\">A</h2>\n<h3 id=\"b\">B</h3>\n<h4 id=\"c\">C</h4>\n<h2 id=\"d\">D</h2>\n",
    );
    // Each lists every heading, before it or after.
    extension.needs(Extensions::ENABLE_BLOCK_ATTRIBUTES | Extensions::ENABLE_AUTO_IDS).reads(
        "* TOC\n{:toc}\n\n# A\n\n* TOC\n{:toc}\n\n# B\n",
        "<ul><li><a href=\"#a\">A</a></li>\n<li><a href=\"#b\">B</a></li>\n</ul>\n<h1 id=\"a\">A</h1>\n<ul><li><a href=\"#a\">A</a></li>\n<li><a href=\"#b\">B</a></li>\n</ul>\n<h1 id=\"b\">B</h1>\n",
        "<ul><li>TOC</li>\n</ul>\n<h1 id=\"a\">A</h1>\n<ul><li>TOC</li>\n</ul>\n<h1 id=\"b\">B</h1>\n",
    );
    // Leaving out links, footnotes and headings marked no_toc.
    extension.needs(Extensions::ENABLE_FOOTNOTES | Extensions::ENABLE_BLOCK_ATTRIBUTES | Extensions::ENABLE_HEADER_IDS | Extensions::ENABLE_AUTO_IDS).reads(
        "* TOC\n{:toc}\n\n## See [docs](http://x.com) *now* [^1]\n\n## Two\n{:.no_toc}\n\n# Hello #    {#custom}\n\n[^1]: note\n",
        "<ul><li><a href=\"#see-docs-now-\">See docs <em>now</em> </a></li>\n<li><a href=\"#custom\">Hello</a></li>\n</ul>\n<h2 id=\"see-docs-now-\">See <a href=\"http://x.com\">docs</a> <em>now</em> <sup id=\"fnref:1\"><a href=\"#fn:1\" class=\"footnote\" rel=\"footnote\" role=\"doc-noteref\">1</a></sup></h2>\n<h2 id=\"two\" class=\"no_toc\">Two</h2>\n<h1 id=\"custom\">Hello</h1>\n<div class=\"footnotes\" role=\"doc-endnotes\">\n<ol>\n<li id=\"fn:1\">\n<p>note\u{a0}<a href=\"#fnref:1\" class=\"reversefootnote\" role=\"doc-backlink\">↩</a></p>\n</li>\n</ol>\n</div>\n",
        "<ul><li>TOC</li>\n</ul>\n<h2 id=\"see-docs-now-\">See <a href=\"http://x.com\">docs</a> <em>now</em> <sup id=\"fnref:1\"><a href=\"#fn:1\" class=\"footnote\" rel=\"footnote\" role=\"doc-noteref\">1</a></sup></h2>\n<h2 id=\"two\" class=\"no_toc\">Two</h2>\n<h1 id=\"custom\">Hello</h1>\n<div class=\"footnotes\" role=\"doc-endnotes\">\n<ol>\n<li id=\"fn:1\">\n<p>note\u{a0}<a href=\"#fnref:1\" class=\"reversefootnote\" role=\"doc-backlink\">↩</a></p>\n</li>\n</ol>\n</div>\n",
    );
}

#[test]
fn end_of_block_markers() {
    let extension = Extension::new(Extensions::ENABLE_END_OF_BLOCK_MARKERS);
    // Ending a list, a code block, or a quote's lazy lines.
    extension.reads(
        "* a\n^\n* b\n",
        "<ul><li>a</li>\n</ul>\n<ul><li>b</li>\n</ul>\n",
        "<ul><li>a\n^</li>\n<li>b</li>\n</ul>\n",
    );
    extension.reads(
        "    c\n^\n    d\n",
        "<pre><code>c\n</code></pre>\n<pre><code>d\n</code></pre>\n",
        "<pre><code>c\n</code></pre>\n<p>^\nd</p>\n",
    );
    extension.reads(
        "> e\n^\nf\n",
        "<blockquote><p>e</p>\n</blockquote>\n<p>f</p>\n",
        "<blockquote><p>e\n^\nf</p>\n</blockquote>\n",
    );
}

#[test]
fn list_item_paragraphs() {
    let extension = Extension::new(Extensions::ENABLE_LIST_ITEM_PARAGRAPHS);
    // Bare unless a blank line comes after it, for each item.
    extension.reads(
        "* a\n* b\n\n* c\n",
        "<ul><li>a</li>\n<li><p>b</p>\n</li>\n<li>c</li>\n</ul>\n",
        "<ul><li><p>a</p>\n</li>\n<li><p>b</p>\n</li>\n<li><p>c</p>\n</li>\n</ul>\n",
    );
    // The last item's only paragraph, with a blank line after it, is bare
    // only if an item before it is.
    extension.reads(
        "1. a\n\n2. b\n\npara\n",
        "<ol><li><p>a</p>\n</li>\n<li><p>b</p>\n</li>\n</ol>\n<p>para</p>\n",
        "<ol><li><p>a</p>\n</li>\n<li><p>b</p>\n</li>\n</ol>\n<p>para</p>\n",
    );
    extension.reads(
        "* a\n* b\n\npara\n",
        "<ul><li>a</li>\n<li>b</li>\n</ul>\n<p>para</p>\n",
        "<ul><li>a</li>\n<li>b</li>\n</ul>\n<p>para</p>\n",
    );
}

#[test]
fn mixed_list_markers() {
    let extension = Extension::new(Extensions::ENABLE_MIXED_LIST_MARKERS);
    extension.reads(
        "- a\n+ b\n* c\n",
        "<ul><li>a</li>\n<li>b</li>\n<li>c</li>\n</ul>\n",
        "<ul><li>a</li>\n</ul>\n<ul><li>b</li>\n</ul>\n<ul><li>c</li>\n</ul>\n",
    );
}

#[test]
fn comment_extension() {
    let extension = Extension::new(Extensions::ENABLE_COMMENTS);
    // A span or a block given out as an HTML comment, and one never closed
    // left as text.
    extension.reads(
        "a {::comment}b{:/} c\n\n{::comment}\nd\n{:/comment}\n\n{::comment}\ne\n",
        "<p>a <!-- b --> c</p>\n<!-- d -->\n<p>{::comment}\ne</p>\n",
        "<p>a {::comment}b{:/} c</p>\n<p>d</p>\n<p>e</p>\n",
    );
}

#[test]
fn nomarkdown_extension() {
    let extension = Extension::new(Extensions::ENABLE_NOMARKDOWN);
    // Given out as HTML as it is, unless its type is another converter's.
    extension.reads(
        "a {::nomarkdown}*b*{:/} c\n\n{::nomarkdown}\n<i>d</i>\n{:/}\n\n{::nomarkdown type=\"latex\"}\ne\n{:/}\n",
        "<p>a *b* c</p>\n<i>d</i>\n",
        "<p>a {::nomarkdown}<em>b</em>{:/} c</p>\n<p><i>d</i>\n{:/}</p>\n<p>e\n{:/}</p>\n",
    );
}

#[test]
fn backslash_line_breaks() {
    let extension = Extension::new(Extensions::ENABLE_BACKSLASH_LINE_BREAKS);
    // The two backslashes ending a line, after any escaped, but not ending
    // the paragraph.
    extension.reads(
        "a\\\\\nb \\\\\\\\\nc \\\\\n",
        "<p>a<br />\nb \\<br />\nc \\</p>\n",
        "<p>a\\\nb \\\\\nc \\</p>\n",
    );
}

#[test]
fn raw_html_spans() {
    let extension = Extension::new(Extensions::ENABLE_RAW_HTML_SPANS);
    extension.reads(
        "Run <kbd>--help</kbd> or <code>*a*</code>.\n",
        "<p>Run <kbd>--help</kbd> or <code>*a*</code>.</p>\n",
        "<p>Run <kbd>–help</kbd> or <code><em>a</em></code>.</p>\n",
    );
}

#[test]
fn html_block_ends() {
    let extension = Extension::new(Extensions::ENABLE_HTML_BLOCK_ENDS);
    // At the element's end tag, with Markdown after it.
    extension.reads(
        "<div>\na\n</div>\n*b*\n\n<p>c</p> *d*\n",
        "<div>\na\n</div>\n<p><em>b</em></p>\n<p>c</p>\n<p><em>d</em></p>\n",
        "<div>\na\n</div>\n*b*\n<p>c</p> *d*\n",
    );
}

#[test]
fn tables_without_headers() {
    let extension =
        Extension::new(Extensions::ENABLE_TABLES_WITHOUT_HEADERS).needs(Extensions::ENABLE_TABLES);
    // Rows alone, with a `|` in code kept in its cell, and a separator of
    // fewer cells than the header's.
    extension.reads(
        "| a | `b|c` |\n| d |\n\n| e | f |\n|-\n| g | h |\n",
        "<table><tbody><tr><td>a</td><td><code>b|c</code></td></tr>\n<tr><td>d</td><td>\u{a0}</td></tr>\n</tbody></table>\n<table><thead><tr><th>e</th><th>f</th></tr></thead>\n<tbody><tr><td>g</td><td>h</td></tr>\n</tbody></table>\n",
        "<p>| a | <code>b|c</code> |\n| d |</p>\n<p>| e | f |\n|-\n| g | h |</p>\n",
    );
}

#[test]
fn abbreviations() {
    let extension = Extension::new(Extensions::ENABLE_ABBREVIATIONS);
    // Each use as a word, with the attributes from lists on the lines
    // around the definition.
    extension.reads(
        "*[HTML]: Hyper Text\n{:.x}\n\nHTML and XHTML or (HTML).\n",
        "<p><abbr class=\"x\" title=\"Hyper Text\">HTML</abbr> and XHTML or (<abbr class=\"x\" title=\"Hyper Text\">HTML</abbr>).</p>\n",
        "<p class=\"x\">*[HTML]: Hyper Text</p>\n<p>HTML and XHTML or (HTML).</p>\n",
    );
}

#[test]
fn kramdown_escapes() {
    let extension = Extension::new(Extensions::ENABLE_KRAMDOWN_ESCAPES);
    // A backslash before a character kramdown doesn't escape is kept.
    extension.reads(
        "\\& \\* \\\\& \\/\n",
        "<p>\\&amp; * \\&amp; \\/</p>\n",
        "<p>&amp; * \\&amp; /</p>\n",
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
    assert_eq!(html("a | b\n-|-\n", options), "<p>a | b\n-|-</p>\n");

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

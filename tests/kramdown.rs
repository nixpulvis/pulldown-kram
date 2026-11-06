//! Tests of pulldown-kram's public API, including rendering a typical Jekyll
//! post. Each extension's behavior is compared against kramdown's own output
//! in tests/kramdown_suite/, which has a directory of cases per extension.

use pulldown_kram::pulldown_cmark::{BlockQuoteKind, Event, Options as Cmark, Parser, Tag};
use pulldown_kram::{Attributes, Document, Extensions, Options, html};

/// The document as HTML.
fn html(markdown: &str, options: impl Into<Options>) -> String {
    let mut out = String::new();
    html::push_html(&mut out, Document::new(markdown, options).events());
    out
}

/// The document's events, each with its attributes.
fn events(
    markdown: &str,
    options: impl Into<Options>,
) -> Vec<(Event<'static>, Attributes<'static>)> {
    Document::new(markdown, options)
        .events()
        .map(|(event, attributes)| (event.into_static(), attributes.into_static()))
        .collect()
}

#[test]
fn reads_a_post() {
    let markdown = r#"* TOC
{:toc}

# Getting started

Install it with `gem install kramdown`{:.shell}, then build the site.[^1]

## Options
{:.no_toc}

Term
: A definition, in HTML.

| Option | Default |
|--------|---------|
| toc    | `false` |

~~~ruby
puts "hi"
~~~

*[HTML]: HyperText Markup Language

[^1]: A footnote.
"#;
    // The space before the footnote's ↩ is a no-break space, as kramdown
    // writes it.
    let expected = r##"<ul><li><a href="#getting-started">Getting started</a></li>
</ul>
<h1 id="getting-started">Getting started</h1>
<p>Install it with <code class="shell">gem install kramdown</code>, then build the site.<sup id="fnref:1"><a href="#fn:1" class="footnote" rel="footnote" role="doc-noteref">1</a></sup></p>
<h2 id="options" class="no_toc">Options</h2>
<dl><dt>Term</dt>
<dd>A definition, in <abbr title="HyperText Markup Language">HTML</abbr>.</dd>
</dl>
<table><thead><tr><th>Option</th><th>Default</th></tr></thead>
<tbody><tr><td>toc</td><td><code>false</code></td></tr>
</tbody></table>
<pre><code class="language-ruby">puts &quot;hi&quot;
</code></pre>
<div class="footnotes" role="doc-endnotes">
<ol>
<li id="fn:1">
<p>A footnote. <a href="#fnref:1" class="reversefootnote" role="doc-backlink">↩</a></p>
</li>
</ol>
</div>
"##;
    assert_eq!(html(markdown, Extensions::all()), expected);
}

#[test]
fn each_extension_can_be_turned_off() {
    // For each extension: a document that uses it, and any other extensions
    // it depends on.
    let documents = [
        (
            Extensions::ENABLE_TABLES,
            "a | b\n-|-\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_FOOTNOTES,
            "a[^1]\n\n[^1]: b\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_STRIKETHROUGH,
            "~~a~~\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_TASK_LISTS,
            "* [x] a\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_TYPOGRAPHY,
            "a -- b\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_DEFINITION_LISTS,
            "a\n: b\n",
            Extensions::empty(),
        ),
        (Extensions::ENABLE_MATH, "$$x$$\n", Extensions::empty()),
        (
            Extensions::ENABLE_BLOCK_ATTRIBUTES,
            "a\n{:.x}\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_SPAN_ATTRIBUTES,
            "*a*{:.x}\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_ITEM_ATTRIBUTES,
            "* {:.x} a\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_HEADER_IDS,
            "# a {#b}\n",
            Extensions::empty(),
        ),
        (Extensions::ENABLE_AUTO_IDS, "# a\n", Extensions::empty()),
        (
            Extensions::ENABLE_MARKDOWN_ATTRIBUTE,
            "<div markdown=\"1\">\n*a*\n</div>\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_SPAN_TAG_PARAGRAPHS,
            "<b>\n*a*\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_LANGUAGE_CLASSES,
            "~~~\nx\n~~~\n{:.language-ruby}\n",
            Extensions::ENABLE_BLOCK_ATTRIBUTES,
        ),
        (
            Extensions::ENABLE_TABLES_OF_CONTENTS,
            "* x\n{:toc}\n\n# a\n",
            Extensions::ENABLE_BLOCK_ATTRIBUTES.union(Extensions::ENABLE_AUTO_IDS),
        ),
        (
            Extensions::ENABLE_END_OF_BLOCK_MARKERS,
            "* a\n^\n* b\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_LIST_ITEM_PARAGRAPHS,
            "* a\n* b\n\n* c\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_MIXED_LIST_MARKERS,
            "- a\n+ b\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_COMMENTS,
            "{::comment}a{:/}\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_NOMARKDOWN,
            "{::nomarkdown}*a*{:/}\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_BACKSLASH_LINE_BREAKS,
            "a\\\\\nb\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_RAW_HTML_SPANS,
            "<kbd>*a*</kbd>\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_HTML_BLOCK_ENDS,
            "<div>\na\n</div>\n*b*\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_TABLES_WITHOUT_HEADERS,
            "| a | b |\n",
            Extensions::ENABLE_TABLES,
        ),
        (
            Extensions::ENABLE_ABBREVIATIONS,
            "*[A]: b\n\nA\n",
            Extensions::empty(),
        ),
        (
            Extensions::ENABLE_KRAMDOWN_ESCAPES,
            "\\&\n",
            Extensions::empty(),
        ),
    ];
    let listed = documents
        .iter()
        .fold(Extensions::empty(), |listed, (extension, ..)| {
            listed | *extension
        });
    assert_eq!(
        listed,
        Extensions::all(),
        "every extension needs a test document"
    );

    for (extension, markdown, needs) in documents {
        assert_ne!(
            events(markdown, Extensions::all()),
            events(markdown, Extensions::all() - extension),
            "{extension:?} changes nothing in {markdown:?} when turned off",
        );
        assert_ne!(
            events(markdown, extension | needs),
            events(markdown, needs),
            "{extension:?} changes nothing in {markdown:?} when turned on alone",
        );
    }
}

#[test]
fn cmark_options() {
    // Turning off an extension also turns off the pulldown-cmark option behind
    // it, even when `cmark` asks for that option.
    let options = Options::new(
        Extensions::all() - Extensions::ENABLE_TABLES,
        Cmark::ENABLE_TABLES,
    );
    assert_eq!(html("a | b\n-|-\n", options), "<p>a | b\n-|-</p>\n");

    // Other pulldown-cmark options are passed through, like GFM's
    // `> [!NOTE]` blockquotes.
    let options = Options::new(Extensions::all(), Cmark::ENABLE_GFM);
    let document = Document::new("> [!NOTE]\n> a\n", options);
    assert!(matches!(
        document.events().next(),
        Some((Event::Start(Tag::BlockQuote(Some(BlockQuoteKind::Note))), _))
    ));

    // With no extensions and no options, the events are exactly
    // pulldown-cmark's.
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

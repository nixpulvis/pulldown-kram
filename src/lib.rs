//! A [kramdown] parser built on [pulldown-cmark].
//!
//! kramdown is a Markdown dialect (like GFM) which [Jekyll] uses by default,
//! and supports many extensions which are [listed below](#syntax-support).
//! This crate parses it into [`pulldown_cmark::Event`]s, pairing each event
//! with the [`Attributes`] that kramdown attribute lists such as
//! `{: .note #intro}` assign to it.
//!
//! ```
//! use pulldown_kram::{Document, Extensions, pulldown_cmark::{Event, Tag}};
//!
//! let document = Document::new("# Intro\n\nSee `x`{:.new}.\n", Extensions::all());
//! let events: Vec<_> = document.events().collect();
//!
//! // `# Intro` gets the id `intro`, as in kramdown.
//! assert!(matches!(
//!     &events[0].0,
//!     Event::Start(Tag::Heading { id: Some(id), .. }) if id.as_ref() == "intro"
//! ));
//!
//! // `{:.new}` gives the code span before it the class `new`.
//! let (_, attributes) = events
//!     .iter()
//!     .find(|(event, _)| matches!(event, Event::Code(_)))
//!     .unwrap();
//! assert!(attributes.has_class("new"));
//! ```
//!
//! The input is lightly rewritten where kramdown and CommonMark disagree,
//! parsed by pulldown-cmark, and the resulting events are post-processed to
//! add kramdown's extensions. The output is plain pulldown-cmark events, so
//! existing renderers work unchanged, and can use the attributes if they
//! want them.
//!
//! [`html::push_html`] renders the events as HTML the way kramdown does,
//! attributes included. The [`html`] module mirrors
//! [`pulldown_cmark::html`][cmark-html].
//!
//! [kramdown]: https://kramdown.gettalong.org
//! [cmark-html]: https://docs.rs/pulldown-cmark/0.13/pulldown_cmark/html/index.html
//! [Jekyll]: https://jekyllrb.com
//! [pulldown-cmark]: https://docs.rs/pulldown-cmark
//!
//! # Syntax support
//!
//! What's supported from [kramdown's syntax], following the sections of its
//! documentation. An item marked ☑ works as it does in kramdown. One marked
//! ⊟ does apart from the ☐ items after it. One marked ☐ doesn't, and says
//! what happens instead. Items that need one of the [`Extensions`] start
//! with its name. Each extension can be turned off on its own, and
//! [`Extensions::all()`] turns them all on.
//!
//! [kramdown's syntax]: https://kramdown.gettalong.org/syntax.html
//!
//! ## Source text
//!
//! - ⊟ Tab stops every four columns.
//! - ☐ A tab after a blockquote marker counting toward a code block's
//!   indent, so `> \tcode` is a code block. Instead, it's a paragraph.
//! - ☐ Headers only on a block boundary. kramdown's GFM parser lets an atx
//!   header interrupt a paragraph, as here, but a setext underline after a
//!   paragraph of two or more lines stays text. Instead, the paragraph's
//!   lines become the header.
//! - ☐ Lazy lines that start a block, like `> a\n# b`, kept in the
//!   blockquote or list item, as kramdown's GFM parser keeps them. Instead,
//!   the blockquote or list item ends.
//! - ⊟ [`ENABLE_KRAMDOWN_ESCAPES`]: Backslash escapes, for exactly the
//!   characters kramdown escapes. A backslash before any other character is
//!   kept.
//! - ☐ [`ENABLE_KRAMDOWN_ESCAPES`]: Backslashes in link URLs and titles,
//!   which kramdown keeps. Instead, they escape there too.
//!
//! ## Structural elements
//!
//! - ☑ Hard line breaks with two trailing spaces.
//! - ⊟ Paragraphs, including lazy continuation lines.
//! - ☐ A paragraph that a horizontal rule doesn't interrupt, so
//!   `para\n***` stays one paragraph. Instead, the rule ends it.
//! - ☐ An ordered list interrupting a paragraph whatever its first number,
//!   like `para\n2. two`. Instead, it's text.
//! - ⊟ Setext and atx headers, including the `header\n---` edge case.
//! - ☐ Headers only in the first column, so ` # a`, or an underline
//!   indented by a space, stays text. Instead, they make headers.
//! - ☐ Empty atx headers, like `#`, kept as text. Instead, they're empty
//!   headings.
//! - ⊟ Blockquotes, including nested and lazy ones.
//! - ⊟ Indented code blocks, continuing across blank lines.
//! - ☐ Lazy continuation lines in an indented code block. Instead, they
//!   end the code block and start a paragraph.
//! - ⊟ Fenced code blocks with `~~~`, with an optional language after the
//!   fence.
//! - ☐ Words after a fence's language making the lines a paragraph.
//!   Instead, they're a code block.
//! - ☐ An unclosed fence read as a paragraph. Instead, the code block runs
//!   to the end of the document.
//! - ☐ A fence indented by one to three spaces keeping that indent on its
//!   lines. Instead, it's stripped.
//! - ⊟ Ordered and unordered lists, nested or compact, and `1984\.` to
//!   escape a list marker.
//! - ☐ A list item with nothing on its first line and a code block after
//!   it. Instead, the code keeps two more spaces of indent.
//! - ☐ A list item's text starting wherever it starts after the marker, so
//!   `*       text` is text. Instead, five or more spaces start a code
//!   block, as in CommonMark.
//! - ☐ `1)` read as text. Instead, it's an ordered list marker.
//! - ☐ An ordered item right after an unordered one nested in it.
//!   Instead, it starts a new list.
//! - ☐ An unindented fenced code block on the line right after a list
//!   item's text, which kramdown keeps in the item as a lazy line, with the
//!   item's indent removed from its lines. Instead, it ends the list.
//! - ☐ Keeping the extra indent of a fenced code block indented past a
//!   list item's content. Instead, the indent is stripped from its lines.
//! - ☐ Ordered lists always numbered from 1. Instead, the first number is
//!   kept.
//! - ⊟ Horizontal rules.
//! - ☐ `* * *` right after a list item read as a nested item. Instead,
//!   it's a rule.
//! - ☐ HTML rewritten as kramdown writes it: tag and attribute names
//!   lowercased, unclosed tags closed, and stray end tags made text.
//!   Instead, it's passed through as pulldown-cmark HTML events, for the
//!   program reading them to handle.
//! - ☐ The `parse_block_html` option. Instead, HTML blocks are read as
//!   with it off, kramdown's default.
//! - ☐ Any tag that isn't span-level starting an HTML block, even on a
//!   paragraph's next line. Instead, an unknown tag stays in the paragraph.
//! - ☐ Processing instructions and CDATA sections written as kramdown
//!   writes them. Instead, they're kept as HTML.
//! - ☐ XML-namespaced tags, like `<x:y>`, read as HTML. Instead, they're
//!   text.
//! - ☑ [`ENABLE_BACKSLASH_LINE_BREAKS`]: Hard line breaks with two
//!   trailing backslashes.
//! - ☑ [`ENABLE_LANGUAGE_CLASSES`]: A code block's language from an
//!   attribute list, `{: .language-ruby}`, when the fence doesn't give one.
//! - ☑ [`ENABLE_MIXED_LIST_MARKERS`]: Mixing `*`, `+` and `-` in one list.
//! - ⊟ [`ENABLE_HEADER_IDS`]: Header ids, `# Hello {#id}`. As in kramdown,
//!   `# Hello {: #id}` is left as text.
//! - ☐ [`ENABLE_HEADER_IDS`]: Every id kramdown accepts, any XML name like
//!   `{#a.b}` or `{#_x}`. Instead, those are left as text.
//! - ☐ [`ENABLE_HEADER_IDS`]: An id before an atx header's closing hashes,
//!   `# A {#a} ##`, left as text. Instead, it's the header's id.
//! - ⊟ [`ENABLE_LIST_ITEM_PARAGRAPHS`]: Tight and loose list items decided
//!   per item, not per list: an item's first paragraph isn't wrapped in a
//!   paragraph unless a blank line follows it.
//! - ☐ [`ENABLE_LIST_ITEM_PARAGRAPHS`]: An end-of-block marker after a
//!   list wrapping its last item in a paragraph. Instead, the item stays
//!   tight.
//! - ⊟ [`ENABLE_TASK_LISTS`]: Task list items, `* [x] done`, from
//!   kramdown's GFM parser.
//! - ☐ [`ENABLE_TASK_LISTS`]: Task items as kramdown's GFM parser reads
//!   them: an empty `[ ]` is text, and `* {:.c} [x] a` is a task. Instead,
//!   the empty box is a task, and the box after an attribute list is text.
//! - ⊟ [`ENABLE_DEFINITION_LISTS`]: Definition lists, one term per line. A
//!   definition's first paragraph isn't wrapped in a paragraph unless a
//!   blank line comes before it. At most one blank line may separate the
//!   terms from their first definition, any number may separate
//!   definitions, and a definition with an empty first line is indented by
//!   four spaces.
//! - ☐ [`ENABLE_DEFINITION_LISTS`]: Separate terms on lines joined by one
//!   span, like `*a\nb*`. Instead, they form a single term.
//! - ☐ [`ENABLE_DEFINITION_LISTS`]: A definition's first line with its
//!   leading whitespace stripped, so `:     text` is a paragraph. Instead,
//!   four or more spaces make it code.
//! - ☐ [`ENABLE_DEFINITION_LISTS`]: A space or tab required after the
//!   `:`. Instead, `term\n:text` is a definition list.
//! - ⊟ [`ENABLE_TABLES`]: Tables with a header row.
//! - ☐ [`ENABLE_TABLES`]: Several header rows above the separator line.
//!   Instead, the rows before the last make a paragraph.
//! - ☐ [`ENABLE_TABLES`]: Rows wider than the header, widening the table.
//!   Instead, the extra cells are dropped.
//! - ☐ [`ENABLE_TABLES`]: A table with a header followed by a line that
//!   isn't a row, which makes it all a paragraph. Instead, it's a table,
//!   with the line as another row.
//! - ☐ [`ENABLE_TABLES`]: A header and separator line with no rows after
//!   them read as a paragraph. Instead, they're a table.
//! - ☐ [`ENABLE_TABLES`]: Separator lines in a table body, and footer
//!   separator lines, `|===`. Instead, each is a row of dashes or equals
//!   signs after a header row. Without one, `|===` is a header's separator
//!   line, and a table with a separator after its first rows is a
//!   paragraph.
//! - ☐ [`ENABLE_TABLES`]: Two or more lines of only table separators,
//!   which kramdown doesn't treat as a table. Instead, they're parsed as
//!   one.
//! - ☑ [`ENABLE_TABLES_WITHOUT_HEADERS`]: A separator line with fewer
//!   cells than the header, or `+` between its cells.
//! - ⊟ [`ENABLE_TABLES_WITHOUT_HEADERS`]: Tables without a header row.
//! - ⊟ [`ENABLE_TABLES_WITHOUT_HEADERS`]: A `|` in a code span in a table
//!   cell, kept in that cell.
//! - ☐ [`ENABLE_TABLES_WITHOUT_HEADERS`]: A row starting with `|` after one
//!   that doesn't, given an empty first cell. Instead, its cells don't
//!   shift.
//! - ☐ [`ENABLE_TABLES_WITHOUT_HEADERS`]: Tables without a header row in
//!   blockquotes, list items and footnotes. Instead, they're text, with an
//!   empty table before them.
//! - ☐ [`ENABLE_TABLES_WITHOUT_HEADERS`]: A `\|` in a code span in a table
//!   cell keeping its backslash. Instead, it's dropped.
//! - ☐ [`ENABLE_TABLES_WITHOUT_HEADERS`]: A `|` in HTML `<code>` in a cell
//!   kept in that cell. Instead, it starts another cell.
//! - ☐ [`ENABLE_TABLES_WITHOUT_HEADERS`]: A code span over two lines of a
//!   table, which makes them a paragraph. Instead, they're a table.
//! - ☐ [`ENABLE_TABLES_WITHOUT_HEADERS`]: A line with a `|` only inside
//!   inline math read as a table row. Instead, it's a paragraph.
//! - ⊟ [`ENABLE_MATH`]: Math blocks, `$$…$$` alone in a paragraph.
//! - ☐ [`ENABLE_MATH`]: Blank lines in a math block. Instead, a blank line
//!   splits it into paragraphs.
//! - ☐ [`ENABLE_MATH`]: Math blocks only between block boundaries, so `$$`
//!   right after or right before a fence or header is inline math in a
//!   paragraph. Instead, it's a math block.
//! - ☐ [`ENABLE_MATH`]: `$$…$$` alone in a list item or definition read
//!   as a math block. Instead, it's inline math.
//! - ⊟ [`ENABLE_HTML_BLOCK_ENDS`]: HTML blocks, with Markdown inside them
//!   left as HTML. A block ends at the end tag of the element that opened
//!   it, and text after that is parsed as Markdown, as long as no blank line
//!   comes before the end tag.
//! - ☐ [`ENABLE_HTML_BLOCK_ENDS`]: Blank lines inside an HTML block.
//!   Instead, as in CommonMark, a blank line ends the block, and what
//!   follows is parsed as Markdown.
//! - ☐ [`ENABLE_HTML_BLOCK_ENDS`]: Every element that isn't span-level,
//!   like `<details>` or a custom tag, ending at its end tag. Instead, an
//!   element outside a fixed list runs to the next blank line.
//! - ☐ [`ENABLE_HTML_BLOCK_ENDS`]: An end tag inside a comment or an
//!   attribute value leaving the block open. Instead, it ends the block
//!   early.
//! - ☐ [`ENABLE_HTML_BLOCK_ENDS`]: HTML blocks in a blockquote ending at
//!   their end tag. Instead, they run to the next blank line.
//! - ⊟ [`ENABLE_MARKDOWN_ATTRIBUTE`]: `markdown="1"` on a block element.
//!   Its content is parsed as Markdown and the attribute is removed.
//! - ☐ [`ENABLE_MARKDOWN_ATTRIBUTE`]: `markdown="1"` on elements whose
//!   content kramdown reads as spans, like `<p>` and `<h1>`. Instead, their
//!   content is left as HTML.
//! - ☐ [`ENABLE_MARKDOWN_ATTRIBUTE`]: `markdown="1"` with text on the start
//!   tag's line, or after the end tag. Instead, that text is left as HTML.
//! - ☐ [`ENABLE_MARKDOWN_ATTRIBUTE`]: `markdown='1'`, in single quotes.
//!   Instead, it's ignored.
//! - ☐ [`ENABLE_MARKDOWN_ATTRIBUTE`]: `markdown="block"`,
//!   `markdown="span"` and `markdown="0"`. Instead, the attribute is kept,
//!   and the content is left as HTML in a block element and parsed as
//!   Markdown in a span.
//! - ⊟ [`ENABLE_SPAN_TAG_PARAGRAPHS`]: A line with only a span-level tag,
//!   like `<img>`, starting a paragraph together with the text on the next
//!   line.
//! - ☐ [`ENABLE_SPAN_TAG_PARAGRAPHS`]: A line with only a span-level tag,
//!   followed by a blank line, as a paragraph of its own. Instead, it's an
//!   HTML block.
//! - ☐ [`ENABLE_SPAN_TAG_PARAGRAPHS`]: Lines in `<pre>` left as written.
//!   Instead, a line with only a span-level tag is joined to the next there
//!   too.
//!
//! ## Text markup
//!
//! - ⊟ Automatic links, `<http://example.com>` and `<me@example.com>`.
//! - ☐ `<mailto:me@example.com>` with only the address as the link text.
//!   Instead, the link text includes `mailto:`.
//! - ☐ Automatic links with a space in the URL, or a non-ASCII email
//!   address. Instead, they're text.
//! - ⊟ Inline links, with single- or double-quoted titles.
//! - ☐ Inline link titles as kramdown reads them: an empty title leaves
//!   the whole link as text, and a title in parentheses is part of the URL.
//!   Instead, the first is a link without a title, and the second a title.
//! - ☐ Spaces in an inline link's URL, `[a](b c.html)`. Instead, it's
//!   parsed as text.
//! - ☐ A link inside link text, `[a [b](v) c](u)`, left as text in the
//!   outer link. Instead, the inner one is the link.
//! - ⊟ Reference links, `[text][id]`, `[text][]` and `[text]`, matched
//!   case-insensitively.
//! - ☐ Spaces or a line break between a reference link's brackets,
//!   `[text]\n[id]`. Instead, each pair of brackets is read on its own.
//! - ☐ Brackets in a reference link's text, `[[1]][1]`. Instead, the inner
//!   brackets form the link and the outer ones are text.
//! - ⊟ Link definitions, and `\:` to keep one as text.
//! - ☐ The last link definition of a name winning. Instead, the first
//!   wins.
//! - ☐ Link definitions with a space in the URL, `[a]: b c`, or a title in
//!   parentheses, which kramdown takes as part of the URL. Instead, the
//!   first is text and the second a title.
//! - ⊟ Images, inline and by reference.
//! - ☐ An image's alt text as written, including markup and straight
//!   quotes, with only backslash escapes applied. Instead, its markup is
//!   parsed and its dashes, ellipses and quotes made typographic.
//! - ⊟ Emphasis and strong emphasis. `*` works inside words, `_` doesn't.
//! - ☐ Emphasis delimiter runs kramdown reads differently, like `a*"b"*c`
//!   or `This is **a *star*.` Instead, they're read by CommonMark's rules.
//! - ☐ Empty emphasis, `****`, written as `<em>**</em>`. Instead, it's
//!   text.
//! - ☐ Emphasis inside the same kind of emphasis, `*a _b_ c*` or `_*b*_`,
//!   left literal. Instead, it nests.
//! - ☐ `***a***` and `___a___` as strong emphasis around emphasis.
//!   Instead, they're emphasis around strong emphasis.
//! - ⊟ Code spans.
//! - ☐ A backtick next to a space kept literal, as in `` ` a ` ``.
//!   Instead, it opens a code span.
//! - ⊟ HTML spans, with Markdown parsed inside them.
//! - ☐ The HTML spans kramdown leaves raw beyond its documented ones, like
//!   `<u>`, `<mark>` and custom elements. Instead, Markdown is parsed in
//!   them.
//! - ☐ Unclosed HTML spans closed, and stray end tags and block tags in a
//!   paragraph made text. Instead, they're kept as HTML.
//! - ☐ The `parse_span_html` option. Instead, HTML spans are read as with
//!   it on, kramdown's default.
//! - ☐ Guillemets from `<<` and `>>`. Instead, they're parsed as text or
//!   as an HTML tag.
//! - ☑ [`ENABLE_ABBREVIATIONS`]: Abbreviations, `*[HTML]: …`. Each use
//!   becomes an `<abbr>` in HTML events, with the attributes of any
//!   attribute lists on the lines around the definition.
//! - ⊟ [`ENABLE_FOOTNOTES`]: Footnotes, with block content in their
//!   definitions.
//! - ☐ [`ENABLE_FOOTNOTES`]: The last footnote definition of a name
//!   winning, and a lazy line ending a definition. Instead, the first wins,
//!   and lazy lines continue it.
//! - ☐ [`ENABLE_FOOTNOTES`]: A footnote definition on a paragraph's next
//!   line kept as text. Instead, it's a footnote.
//! - ☐ [`ENABLE_FOOTNOTES`]: A table or math block indented in a footnote
//!   definition. Instead, it's text.
//! - ⊟ [`ENABLE_TYPOGRAPHY`]: Dashes, ellipses and smart quotes.
//! - ☐ [`ENABLE_TYPOGRAPHY`]: Smart quotes where kramdown curls them the
//!   other way, like `'Tis` and `5'10"`. Instead, they're curled as
//!   pulldown-cmark curls them.
//! - ☐ [`ENABLE_TYPOGRAPHY`]: Runs of four or more dashes made into dashes
//!   as kramdown makes them. Instead, they're en dashes.
//! - ⊟ [`ENABLE_MATH`]: Inline math, `$$…$$` in a paragraph with other
//!   content. `$…$` stays text.
//! - ☐ [`ENABLE_MATH`]: `\$$…$$` to make math alone in a paragraph inline.
//!   Instead, it's left literal.
//! - ⊟ [`ENABLE_STRIKETHROUGH`]: Strikethrough, `~~…~~`, from kramdown's
//!   GFM parser. `~…~` stays text.
//! - ☐ [`ENABLE_STRIKETHROUGH`]: Runs of tildes kramdown's GFM parser reads
//!   differently, like `~~a~~~`, and `~~~a~~~` as a paragraph. Instead, the
//!   first is text and the second a fence.
//! - ⊟ [`ENABLE_RAW_HTML_SPANS`]: The HTML spans kramdown leaves raw, like
//!   `<kbd>` and `<code>`, their text passed through unchanged.
//! - ☐ [`ENABLE_RAW_HTML_SPANS`]: A nested tag of the same name in a raw
//!   HTML span, like `<kbd>a <kbd>b</kbd> c</kbd>`. Instead, the first end
//!   tag ends the raw text.
//!
//! ## Non-content elements
//!
//! - ☑ Tags for extensions kramdown doesn't have, like `{::something}`,
//!   left as text.
//! - ☐ Definition attribute lists, `: {:.cls} definition`. Instead,
//!   they're parsed as text.
//! - ☐ Term attribute lists, `{:#id} term`. Instead, they're parsed as
//!   text.
//! - ☐ Attribute list definitions, `{:name: .class}`, and references to
//!   them, including kramdown's `standalone`, `footnotes` and `auto_ids`.
//!   Instead, definitions are removed and references ignored.
//! - ☐ The `options` extension. Instead, its block form is removed, with
//!   any body kept as Markdown, and its span form is left as text.
//! - ⊟ [`ENABLE_END_OF_BLOCK_MARKERS`]: The end-of-block marker `^`,
//!   closing the paragraph, list, blockquote or code block before it.
//! - ☐ [`ENABLE_END_OF_BLOCK_MARKERS`]: An indented `^` kept as text.
//!   Instead, it's a marker.
//! - ☐ [`ENABLE_END_OF_BLOCK_MARKERS`]: A marker after an HTML block.
//!   Instead, it's text.
//! - ⊟ [`ENABLE_BLOCK_ATTRIBUTES`]: Block attribute lists, on the line
//!   before or after a block, or alone between blocks to apply to the next
//!   one.
//! - ⊟ [`ENABLE_BLOCK_ATTRIBUTES`]: Block attribute lists on link
//!   definitions, applied to the links and images that use them.
//! - ☐ [`ENABLE_BLOCK_ATTRIBUTES`]: A block attribute list after a list
//!   item ending the list. Instead, it's applied to the list, but the items
//!   after it stay in the same list.
//! - ☐ [`ENABLE_BLOCK_ATTRIBUTES`]: Block attribute lists on HTML blocks.
//!   Instead, one before the block is dropped, and one after goes to the
//!   next block.
//! - ☐ [`ENABLE_BLOCK_ATTRIBUTES`]: An attribute list indented by four
//!   spaces kept as text. Instead, it's read.
//! - ☐ [`ENABLE_BLOCK_ATTRIBUTES`]: An attribute list's attribute replacing
//!   the element's own of the same name, like an ordered list's `start`.
//!   Instead, both are written.
//! - ⊟ [`ENABLE_SPAN_ATTRIBUTES`]: Span attribute lists, directly after
//!   emphasis, a link, an image or a code span.
//! - ⊟ [`ENABLE_SPAN_ATTRIBUTES`]: The empty span attribute list, `{::}`,
//!   as a separator between spans.
//! - ☐ [`ENABLE_SPAN_ATTRIBUTES`]: Span attribute lists after a collapsed
//!   reference link, `[a][]{:.c}`. Instead, they're text.
//! - ☐ [`ENABLE_SPAN_ATTRIBUTES`]: Span attribute lists after HTML spans
//!   and inline math. Instead, they're text.
//! - ☐ [`ENABLE_SPAN_ATTRIBUTES`]: `{::}` on a line of its own kept as
//!   text. Instead, it's dropped.
//! - ⊟ [`ENABLE_ITEM_ATTRIBUTES`]: List item attribute lists,
//!   `* {:.cls} item`.
//! - ☐ [`ENABLE_ITEM_ATTRIBUTES`]: Only the first of two item attribute
//!   lists read, `* {:.a}{:.b} item`. Instead, both are.
//! - ⊟ [`ENABLE_COMMENTS`]: The `comment` extension, as a block or span,
//!   emitted as an HTML comment.
//! - ☐ [`ENABLE_COMMENTS`]: The `comment` extension in a blockquote.
//!   Instead, it's text.
//! - ⊟ [`ENABLE_NOMARKDOWN`]: The `nomarkdown` extension, as a block or
//!   span, emitted as raw HTML unless its `type` names another converter.
//! - ☐ [`ENABLE_NOMARKDOWN`]: The `nomarkdown` extension in a blockquote.
//!   Instead, it's text.
//!
//! ## Converter
//!
//! Not part of the syntax, but features Jekyll sites rely on.
//!
//! - ☐ The `toc_levels` option. Instead, every heading level is listed.
//! - ⊟ [`ENABLE_AUTO_IDS`]: Automatic header ids, generated from the
//!   heading's text without markup, the same way as kramdown's GFM parser,
//!   Jekyll's default.
//! - ☐ [`ENABLE_AUTO_IDS`]: Ids for headings with images or raw HTML
//!   spans, made from the spans' text and not an image's alt text. Instead,
//!   they're made from the alt text and not the spans' text.
//! - ⊟ [`ENABLE_TABLES_OF_CONTENTS`]: `{:toc}` tables of contents, nested
//!   by heading level. Headings with the class `no_toc` are excluded, and
//!   links inside headings are left out. If there are no headings, the
//!   marked list is removed.
//! - ☐ [`ENABLE_TABLES_OF_CONTENTS`]: Only the first list marked `{:toc}`
//!   made a table of contents. Instead, every one is.
//! - ☐ [`ENABLE_TABLES_OF_CONTENTS`]: kramdown's ids on a table of
//!   contents, `markdown-toc` on the list and `markdown-toc-…` on its links.
//!   Instead, there are none.
//!
//! # How it works
//!
//! Reading a document takes four steps.
//!
//! 1. The Markdown is rewritten wherever kramdown reads a line differently
//!    from CommonMark, so that pulldown-cmark reads it kramdown's way. For
//!    example, a blank line goes in after the end tag of an HTML block,
//!    since kramdown ends the block there and CommonMark doesn't. Syntax
//!    pulldown-cmark can't read at all, like `{::comment}` or the
//!    end-of-block marker, is replaced with a stand-in: a little HTML that
//!    pulldown-cmark reads as a block or a span, and that the reader later
//!    swaps for what it stands for.
//! 2. pulldown-cmark parses the rewritten text.
//! 3. A reader takes the events a block at a time. It takes attribute
//!    lists out of the text and attaches them to the elements they belong
//!    to, puts back what the stand-ins stand for, and corrects the events
//!    where kramdown's structure differs, like in definition lists, list
//!    item paragraphs, header ids and abbreviations.
//! 4. With tables of contents on, all of the events are read first, and
//!    each list marked `{:toc}` is replaced with links to the headings.
//!
//! The ranges from [`Events::into_offset_iter`] are mapped back through the
//! rewrites, so they point into the Markdown as it was given.
//!
//! [`ENABLE_ABBREVIATIONS`]: Extensions::ENABLE_ABBREVIATIONS
//! [`ENABLE_AUTO_IDS`]: Extensions::ENABLE_AUTO_IDS
//! [`ENABLE_BACKSLASH_LINE_BREAKS`]: Extensions::ENABLE_BACKSLASH_LINE_BREAKS
//! [`ENABLE_BLOCK_ATTRIBUTES`]: Extensions::ENABLE_BLOCK_ATTRIBUTES
//! [`ENABLE_COMMENTS`]: Extensions::ENABLE_COMMENTS
//! [`ENABLE_DEFINITION_LISTS`]: Extensions::ENABLE_DEFINITION_LISTS
//! [`ENABLE_END_OF_BLOCK_MARKERS`]: Extensions::ENABLE_END_OF_BLOCK_MARKERS
//! [`ENABLE_FOOTNOTES`]: Extensions::ENABLE_FOOTNOTES
//! [`ENABLE_HEADER_IDS`]: Extensions::ENABLE_HEADER_IDS
//! [`ENABLE_HTML_BLOCK_ENDS`]: Extensions::ENABLE_HTML_BLOCK_ENDS
//! [`ENABLE_ITEM_ATTRIBUTES`]: Extensions::ENABLE_ITEM_ATTRIBUTES
//! [`ENABLE_KRAMDOWN_ESCAPES`]: Extensions::ENABLE_KRAMDOWN_ESCAPES
//! [`ENABLE_LANGUAGE_CLASSES`]: Extensions::ENABLE_LANGUAGE_CLASSES
//! [`ENABLE_LIST_ITEM_PARAGRAPHS`]: Extensions::ENABLE_LIST_ITEM_PARAGRAPHS
//! [`ENABLE_MARKDOWN_ATTRIBUTE`]: Extensions::ENABLE_MARKDOWN_ATTRIBUTE
//! [`ENABLE_MATH`]: Extensions::ENABLE_MATH
//! [`ENABLE_MIXED_LIST_MARKERS`]: Extensions::ENABLE_MIXED_LIST_MARKERS
//! [`ENABLE_NOMARKDOWN`]: Extensions::ENABLE_NOMARKDOWN
//! [`ENABLE_RAW_HTML_SPANS`]: Extensions::ENABLE_RAW_HTML_SPANS
//! [`ENABLE_SPAN_ATTRIBUTES`]: Extensions::ENABLE_SPAN_ATTRIBUTES
//! [`ENABLE_SPAN_TAG_PARAGRAPHS`]: Extensions::ENABLE_SPAN_TAG_PARAGRAPHS
//! [`ENABLE_STRIKETHROUGH`]: Extensions::ENABLE_STRIKETHROUGH
//! [`ENABLE_TABLES`]: Extensions::ENABLE_TABLES
//! [`ENABLE_TABLES_OF_CONTENTS`]: Extensions::ENABLE_TABLES_OF_CONTENTS
//! [`ENABLE_TABLES_WITHOUT_HEADERS`]: Extensions::ENABLE_TABLES_WITHOUT_HEADERS
//! [`ENABLE_TASK_LISTS`]: Extensions::ENABLE_TASK_LISTS
//! [`ENABLE_TYPOGRAPHY`]: Extensions::ENABLE_TYPOGRAPHY

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

mod abbreviations;
mod attributes;
mod definition_lists;
mod headings;
pub mod html;
mod list_items;
mod options;
mod read;
mod source;
mod text;
mod toc;

use std::borrow::Cow;
use std::fmt;
use std::iter::FusedIterator;
use std::ops::Range;

use abbreviations::Abbreviation;
pub use attributes::Attributes;
pub use options::{Extensions, Options};
pub use pulldown_cmark;
use pulldown_cmark::Event;
use source::{Mark, Marked, Offsets, REWRITES, StandIns};

/// A kramdown document, ready to parse.
///
/// [`Document::new`] rewrites the source once. Each call to
/// [`Document::events`] parses that from the start, so the document can be
/// read more than once without redoing the rewriting.
///
/// The events borrow the document, so it has to outlive them.
///
/// ```
/// use pulldown_kram::{Document, Extensions, html, pulldown_cmark::{Event, Tag}};
///
/// let document = Document::new("# Intro\n\n## Usage\n", Extensions::all());
///
/// // Collect the heading ids...
/// let ids: Vec<String> = document
///     .events()
///     .filter_map(|(event, _)| match event {
///         Event::Start(Tag::Heading { id, .. }) => id.map(|id| id.to_string()),
///         _ => None,
///     })
///     .collect();
/// assert_eq!(ids, ["intro", "usage"]);
///
/// // ...then parse it again to render it.
/// let mut out = String::new();
/// html::push_html(&mut out, document.events());
/// assert_eq!(out, "<h1 id=\"intro\">Intro</h1>\n<h2 id=\"usage\">Usage</h2>\n");
/// ```
#[derive(Debug, Clone)]
pub struct Document<'a> {
    text: Cow<'a, str>,
    /// Where the rewritten text was in the markdown.
    offsets: Offsets,
    /// Where the stand-ins are in the text, and what each stands for.
    marks: Vec<Mark<'a>>,
    abbreviations: Vec<Abbreviation<'a>>,
    options: Options,
}

impl<'a> Document<'a> {
    /// Prepares `markdown` for parsing with `options`, which can be full
    /// [`Options`] or just [`Extensions`].
    ///
    /// Only the enabled extensions are read. Without one, its syntax is read
    /// as CommonMark would.
    ///
    /// pulldown-cmark reads some of kramdown's syntax differently, so the
    /// source is rewritten first for it to read kramdown's way. Most
    /// rewrites are small: kramdown ends an HTML block at its end tag, for
    /// example, so a blank line is added there for CommonMark to end it too.
    /// Syntax CommonMark has no way to read at all, like an end-of-block
    /// marker, is replaced by a stand-in, and [`Document::events`] gives out
    /// what it stands for instead.
    ///
    /// None of this shows in the events: no stand-in is given out, and the
    /// ranges from [`Events::into_offset_iter`] point into `markdown`, not the
    /// rewritten text. Markdown that needs no rewriting is borrowed, not
    /// copied.
    ///
    /// ```
    /// use pulldown_kram::{Document, Extensions, html};
    ///
    /// let render = |extensions| {
    ///     let document = Document::new("*a*{:.x}\n", extensions);
    ///     let mut out = String::new();
    ///     html::push_html(&mut out, document.events());
    ///     out
    /// };
    /// assert_eq!(render(Extensions::all()), "<p><em class=\"x\">a</em></p>\n");
    ///
    /// // Without span attribute lists, `{:.x}` is text.
    /// let extensions = Extensions::all().difference(Extensions::ENABLE_SPAN_ATTRIBUTES);
    /// assert_eq!(render(extensions), "<p><em>a</em>{:.x}</p>\n");
    /// ```
    pub fn new(markdown: &'a str, options: impl Into<Options>) -> Self {
        let options = options.into();
        // The stand-ins go in first, as what they stand for borrows from
        // `markdown`, not from a rewrite of it.
        let Marked {
            mut text,
            mut offsets,
            mut marks,
            abbreviations,
        } = StandIns::rewrite(markdown, options.kramdown);
        for (extension, rewrite) in REWRITES {
            if !options.kramdown.contains(extension) {
                continue;
            }
            let (Cow::Owned(rewritten), edits) = rewrite(&text) else {
                continue;
            };
            for mark in &mut marks {
                mark.at = edits.moved(mark.at);
            }
            offsets = offsets.then(edits);
            text = Cow::Owned(rewritten);
        }
        Self {
            text,
            offsets,
            marks,
            abbreviations,
            options,
        }
    }

    /// Returns the document's events, each with its attributes.
    ///
    /// Events are returned a block at a time, as each block is finished,
    /// since an attribute list after a block still applies to it. With
    /// [`Extensions::ENABLE_TABLES_OF_CONTENTS`], every event is collected
    /// before the first is returned instead, so tables of contents can link
    /// to headings that come after them. [`Extensions::streamable()`] turns
    /// on everything else.
    ///
    /// [`Events::into_offset_iter`] also yields each event's byte range in
    /// the markdown.
    ///
    /// ```
    /// use pulldown_kram::{Document, Extensions, pulldown_cmark::{Event, Tag}};
    ///
    /// // The attribute list on the line after the paragraph applies to it.
    /// let document = Document::new("A paragraph.\n{: .note}\n", Extensions::all());
    /// let (_, attributes) = document
    ///     .events()
    ///     .find(|(event, _)| matches!(event, Event::Start(Tag::Paragraph)))
    ///     .unwrap();
    /// assert!(attributes.has_class("note"));
    /// ```
    pub fn events(&self) -> Events<'_> {
        let reader = read::Reader::new(&self.text, self.options, &self.marks)
            .abbreviating(&self.abbreviations);
        let items = if self
            .options
            .kramdown
            .contains(Extensions::ENABLE_TABLES_OF_CONTENTS)
        {
            Items::Read(toc::Toc::replace_lists(reader.collect()).into_iter())
        } else {
            Items::Streamed(Box::new(reader))
        };
        Events {
            items,
            offsets: &self.offsets,
        }
    }
}

/// An iterator over the events of a [`Document`] and their attributes,
/// returned by [`Document::events`].
///
/// Each item pairs a [`pulldown_cmark::Event`] with its [`Attributes`]. The
/// event is pulldown-cmark's own type, not a wrapper, so dropping the
/// attributes leaves events that anything built for pulldown-cmark accepts.
///
/// ```
/// use pulldown_kram::{Document, Extensions, pulldown_cmark::Event};
///
/// let document = Document::new("*a*{:.x}\n", Extensions::all());
/// let events: Vec<Event> = document.events().map(|(event, _)| event).collect();
/// assert_eq!(events.len(), 5);
/// ```
///
/// The attributes say what kramdown's attribute lists assigned to each
/// event:
///
/// ```
/// use pulldown_kram::{Document, Extensions, pulldown_cmark::Event};
///
/// let document = Document::new("*a*{:.x}\n", Extensions::all());
/// let classes: Vec<_> = document
///     .events()
///     .filter_map(|(event, attributes)| match event {
///         Event::Start(_) => Some(attributes.classes),
///         _ => None,
///     })
///     .collect();
/// assert_eq!(classes, [vec![], vec!["x".into()]]);
/// ```
pub struct Events<'a> {
    items: Items<'a>,
    /// Where the items' ranges were in the document's markdown.
    offsets: &'a Offsets,
}

/// Where the events come from.
enum Items<'a> {
    /// Read as they're asked for.
    Streamed(Box<read::Reader<'a>>),
    /// Read all at once.
    Read(std::vec::IntoIter<read::Item<'a>>),
}

impl<'a> Events<'a> {
    /// Converts this into an iterator that also yields each event's byte
    /// range in the document's markdown, like pulldown-cmark's
    /// [`Parser::into_offset_iter`](pulldown_cmark::Parser::into_offset_iter).
    pub fn into_offset_iter(self) -> OffsetIter<'a> {
        OffsetIter(self)
    }

    /// The next item, with its range in the rewritten text.
    fn item(&mut self) -> Option<read::Item<'a>> {
        match &mut self.items {
            Items::Streamed(reader) => reader.next(),
            Items::Read(items) => items.next(),
        }
    }
}

impl<'a> Iterator for Events<'a> {
    type Item = (Event<'a>, Attributes<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.item()
            .map(|item| (item.event, item.attribute_lists.attributes))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.items {
            Items::Streamed(reader) => reader.size_hint(),
            Items::Read(items) => items.size_hint(),
        }
    }
}

impl FusedIterator for Events<'_> {}

impl fmt::Debug for Events<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut events = f.debug_struct("Events");
        match &self.items {
            Items::Streamed(reader) => events
                .field("streamed", &true)
                .field("ready", &reader.size_hint().0),
            Items::Read(items) => events
                .field("streamed", &false)
                .field("ready", &items.len()),
        };
        events.finish()
    }
}

/// An iterator over the events of a [`Document`], with their attributes and
/// byte ranges in the markdown the document was created from.
///
/// Before parsing, the markdown may be rewritten slightly so pulldown-cmark
/// parses it the way kramdown does. Ranges still point into the original
/// text: a range that overlaps a rewritten part covers all of it, and a
/// range inside an inserted line is empty, at the point where the line was
/// inserted. Every range has `start <= end`, falls on char boundaries, and
/// lies within the markdown.
///
/// ```
/// use pulldown_kram::{Document, Extensions, pulldown_cmark::{Event, Tag}};
///
/// let markdown = "<div markdown=\"1\">\n*a*{:.x} b\n</div>\n";
/// let document = Document::new(markdown, Extensions::all());
/// let (_, attributes, range) = document
///     .events()
///     .into_offset_iter()
///     .find(|(event, _, _)| matches!(event, Event::Start(Tag::Emphasis)))
///     .unwrap();
/// assert_eq!(&markdown[range], "*a*");
/// assert!(attributes.has_class("x"));
/// ```
pub struct OffsetIter<'a>(Events<'a>);

impl<'a> Iterator for OffsetIter<'a> {
    type Item = (Event<'a>, Attributes<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.0.item()?;
        let range = self.0.offsets.original(item.range);
        Some((item.event, item.attribute_lists.attributes, range))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl FusedIterator for OffsetIter<'_> {}

impl fmt::Debug for OffsetIter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OffsetIter")
            .field("events", &self.0)
            .field("rewritten", &!self.0.offsets.is_empty())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_events_read_afresh_each_time() {
        let document = Document::new("# A\n# A\n", Extensions::all());
        let ids = || -> Vec<String> {
            document
                .events()
                .filter_map(|(event, _)| match event {
                    Event::Start(pulldown_cmark::Tag::Heading { id, .. }) => {
                        id.map(|id| id.to_string())
                    }
                    _ => None,
                })
                .collect()
        };
        assert_eq!(ids(), ["a", "a-1"]);
        assert_eq!(ids(), ["a", "a-1"]);
    }

    #[test]
    fn document_events_reading_all_first_reads_what_streaming_does() {
        let markdown = "# A {#x}\n\npara\n{:.y}\n\n* *b*{:.z}\n\n> q\n\n{:.w}\n\n---\n";
        let all = Document::new(markdown, Extensions::all());
        let streamed = Document::new(markdown, Extensions::streamable());
        assert!(all.events().eq(streamed.events()));
    }

    #[test]
    fn events_into_offset_iter_offsets_are_into_the_markdown() {
        let documents = [
            // Nothing rewritten.
            "# Héading {#h}\n\nSome *tëxt*{:.x} and `code`.\n\n* one\n* two\n{:.list}\n\n> quote\n",
            // Blank lines added inside an element.
            "<div markdown=\"1\">\n# Inside\n\n*ä*{:.x} b\n</div>\nafter\n",
            // A span tag's line joined to the next.
            "<span class=\"g\">  \n*ä* b\n</span>\n\nafter ü\n",
            // Both.
            "<div markdown=\"1\">\n<img src=\"é\" />\nnaïve *text*\nand `code`\n</div>\n\nafter ü\n",
        ];
        let options = [Extensions::all(), Extensions::streamable()];
        for markdown in documents {
            for options in options {
                let document = Document::new(markdown, options);
                let mut checked = 0;
                for (event, _, range) in document.events().into_offset_iter() {
                    let Some(source) = markdown.get(range.clone()) else {
                        panic!("{event:?} at {range:?} isn't in {markdown:?}");
                    };
                    let (Event::Text(text) | Event::Code(text) | Event::InlineHtml(text)) = &event
                    else {
                        continue;
                    };
                    if !document.offsets.touches(&range) {
                        assert!(
                            source.contains(text.as_ref()),
                            "{event:?} at {range:?} is {source:?}"
                        );
                        checked += 1;
                    }
                }
                assert!(checked >= 3, "only {checked} checked in {markdown:?}");
                // The offsets don't change the events.
                assert!(
                    document
                        .events()
                        .into_offset_iter()
                        .map(|(event, attributes, _)| (event, attributes))
                        .eq(document.events())
                );
            }
        }
    }

    #[test]
    fn events_into_offset_iter_finds_text_after_rewrites() {
        let markdown = "<div markdown=\"1\">\n<b>  \n*x*{:.y}\n</div>\n\nafter ü\n";
        let document = Document::new(markdown, Extensions::all());
        assert!(!document.offsets.is_empty());
        let ranges: Vec<_> = document
            .events()
            .into_offset_iter()
            .filter_map(|(event, _, range)| match event {
                Event::Text(_) | Event::InlineHtml(_) => Some(&markdown[range]),
                _ => None,
            })
            .collect();
        assert_eq!(ranges, ["<b>", "  \n", "x", "after ü"]);
    }
}

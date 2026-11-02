# Changelog

## Unreleased

The first release.

- `Document` reads Markdown written for kramdown as pulldown-cmark events,
  each with its `Attributes`.
- `Extensions` turns each of kramdown's extensions on or off, and `Options`
  adds pulldown-cmark's options for what kramdown doesn't read.
  `Options::default()` reads what kramdown reads.
- `Events::into_offset_iter` gives each event's byte range in the original
  text, past the rewriting done before pulldown-cmark reads it.
- `html::push_html`, `html::write_html_fmt` and `html::write_html_io`
  write events as HTML, the way kramdown writes the same elements,
  attributes and footnotes included.
- `Attributes::into_static` keeps attributes after the document is gone.
- Tables of contents stop being made once they'd hold more than
  100,000 items, or as many as the document has if more. The lists marked
  `{:toc}` past that stay as written.
- An attribute list with a word like `#aé` no longer panics.
- The tags of extensions kramdown doesn't have, like `{::something}`, stay
  text, as in kramdown, rather than being dropped.
- A list marked `{:toc}` is dropped when there are no headings to list,
  as in kramdown, rather than left an empty list.
- Math is without the whitespace inside its `$$`, as in kramdown.
- `html` writes a fence's language without the highlighter options after
  a `?`, so `php?start_inline=1` is the class `language-php`.
- A header id with a `_` before punctuation, like `{#a_:b}`, is read.
- HTML events are without the `markdown="1"` in their tags, which kramdown
  reads and leaves out.
- Definition lists read blank lines and indentation as kramdown does: a
  `:` more than one blank line after the terms starts no definition, an
  empty definition doesn't end its list, and a definition with nothing on
  its first line is indented by four.
- A block attribute list on the line before a definition list's first
  term, or lazily after its last definition, is for the list, rather than
  an empty term before it or the last definition.
- An indented code block right after a line of block attribute lists is
  read as code, as in kramdown, rather than as more of the paragraph.
- Block attribute lists on the line before or after a link definition, or
  alone between blocks before one, go to the links and images that use
  it, as in kramdown. One before a definition no longer keeps it from
  being read, and one after no longer goes to the next block.
- `Extensions::ENABLE_END_OF_BLOCK_MARKERS` reads kramdown's end-of-block
  marker, a line of only `^`, ending the paragraph, list, quote or code
  block before it.
- `Extensions::ENABLE_LIST_ITEM_PARAGRAPHS` decides whether a list item's
  first paragraph is bare for each item, as kramdown does: bare unless a
  blank line comes after it. pulldown-cmark decides for the whole list.
- `Extensions::ENABLE_MIXED_LIST_MARKERS` reads a list whose items are
  marked with any of `*`, `+` and `-` as one list, as kramdown does.
- `Extensions::ENABLE_COMMENT_EXTENSION` reads kramdown's `comment`
  extension, giving its text out as an HTML comment, where its text was
  read as Markdown, and `Extensions::ENABLE_NOMARKDOWN_EXTENSION` its
  `nomarkdown`, giving its text out as HTML as it is. Their tags left
  unclosed stay text, as in kramdown.
- `Extensions::ENABLE_BACKSLASH_LINE_BREAKS` reads two backslashes ending
  a line as a hard line break, as kramdown does.
- `Extensions::ENABLE_RAW_HTML_SPANS` gives the text of the HTML spans
  kramdown leaves raw, like `<kbd>`, `<code>` and `<var>`, out as it is,
  where Markdown and typography were read in it, so
  `<kbd>--help</kbd>` no longer has a dash.
- `Extensions::ENABLE_HTML_BLOCK_ENDS` ends an HTML block with the end tag
  of the element starting it, or the end of a comment, as kramdown does,
  reading the text after it as Markdown, where it was left in the block
  up to the next blank line.

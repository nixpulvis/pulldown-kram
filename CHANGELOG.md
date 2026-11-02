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

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

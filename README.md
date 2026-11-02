# pulldown-kram

Reads Markdown written for [kramdown], Jekyll's default, as [pulldown-cmark]
events, each with the attributes kramdown's attribute lists give it.

```rust
use pulldown_kram::{Document, Extensions, pulldown_cmark::Event};

let document = Document::new("# Intro\n\nSee `x`{:.new}.\n", Extensions::all());
for (event, attributes) in document.events() {
    if let Event::Code(code) = &event {
        assert_eq!(code.as_ref(), "x");
        assert!(attributes.has_class("new"));
    }
}
```

pulldown-cmark reads the document as CommonMark, after a little rewriting
where kramdown reads a line differently, and the events it gives are worked
over for kramdown's additions: attribute lists, header ids, definition lists,
`$$` math, `markdown="1"` HTML and `{:toc}` tables of contents, among others.
So a program that renders pulldown-cmark's events renders these too, and can
pick up the attributes where it wants them.

- Each of kramdown's extensions can be left off with `Extensions`.
  `Extensions::all()` reads what kramdown reads.
- `Events::into_offset_iter` gives each event's place in the original text.
- `html::push_html` writes the events as HTML, the way kramdown writes the
  same elements, attributes included.
- The [documentation] lists, section by section, which parts of kramdown's
  syntax are read as kramdown means them and what happens with the rest.

## Tests

`tests/kramdown.rs` checks each extension with it alone and with all but it.
`tests/kramdown_suite/` runs kramdown's own test cases, with their expected
HTML made by kramdown 2.5.2 reading GFM, as Jekyll does. The cases
pulldown-kram reads differently are listed in
`tests/kramdown_suite/known-failures.txt`.

## License

MIT or Apache-2.0, at your option. kramdown's test cases, under
`tests/kramdown_suite/testcases/`, are kramdown's, under its MIT license.

[kramdown]: https://kramdown.gettalong.org
[pulldown-cmark]: https://docs.rs/pulldown-cmark
[documentation]: https://docs.rs/pulldown-kram

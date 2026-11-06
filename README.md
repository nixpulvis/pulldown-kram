# pulldown-kram

A [kramdown] parser built on [`pulldown_cmark`].

kramdown is a Markdown dialect (like GFM) which [Jekyll] uses by default. This
crate parses it into `pulldown_cmark` events, pairing each event with the
attributes that kramdown attribute lists such as `{: .note #intro}` assign to
it.

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

Before parsing, the input is lightly rewritten where kramdown and CommonMark
disagree, then `pulldown_cmark` parses the result as CommonMark. Its events are
then post-processed to add kramdown's extensions: attribute lists, header ids,
definition lists, `$$` math, `markdown="1"` HTML and `{:toc}` tables of
contents, among others. The output is plain `pulldown_cmark` events, so existing
renderers work unchanged and can use the attributes where they want them.

- Each of kramdown's extensions can be left off with `Extensions`.
  `Extensions::all()` reads what kramdown reads.
- `Events::into_offset_iter` gives each event's place in the original text.
- `html::push_html` writes the events as HTML, the way kramdown writes the same
  elements, attributes included.
- The [documentation] lists, section by section, which parts of kramdown's
  syntax are read as kramdown means them and what happens with the rest.

## Testing

`cargo test` runs four kinds of tests:

- Unit tests, beside the code in `src/`.
- Doctests, the examples in the documentation.
- `tests/kramdown.rs`, which covers the public API with a typical Jekyll post,
  and checks that each extension can be turned off.
- `tests/kramdown_suite/`, which reads Markdown with pulldown-kram and compares
  its HTML with what kramdown 2.5.2 makes of the same Markdown, reading GFM as
  Jekyll does. It runs kramdown's own test cases, in `testcases/`, and
  pulldown-kram's: a directory for each extension in `extensions/`, and `core/`
  for what's read with no extension.

Each case in the suite is a test named by its path, so one extension's cases can
be run on their own:

```console
$ cargo test --test kramdown_suite extensions/auto_ids
running 3 tests
test                 extensions/auto_ids/auto_id              ... ok
test                 extensions/auto_ids/repeats_numbered     ... ok
test [known failure] extensions/auto_ids/images_and_raw_spans ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 362 filtered out; finished in 0.00s
```

A case pulldown-kram doesn't yet read as kramdown does is listed in
`tests/kramdown_suite/known-failures.txt`, with why:

```text
extensions/auto_ids/images_and_raw_spans # header ids include an image's alt text and leave out raw HTML spans' text; kramdown does the opposite
```

Its test is marked `[known failure]` and passes while the case still fails, so
the suite shows what's missing without failing. It fails once the case passes,
to be taken off the list.
[`tests/kramdown_suite/README.md`](tests/kramdown_suite/README.md) says how to
add a case, regenerate kramdown's HTML, and see how a known failure differs.

## License

MIT or Apache-2.0, at your option. kramdown's test cases, under
`tests/kramdown_suite/testcases/`, are kramdown's, under its MIT license.

[kramdown]: https://kramdown.gettalong.org
[Jekyll]: https://jekyllrb.com
[`pulldown_cmark`]: https://docs.rs/pulldown-cmark
[documentation]: https://docs.rs/pulldown-kram

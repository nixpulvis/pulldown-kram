# kramdown's test cases

kramdown's own test cases, and pulldown-kram's, read with pulldown-kram and
checked against the HTML kramdown makes of them, as pulldown-cmark checks
itself against the CommonMark spec. `cargo test --test kramdown_suite` runs
them, each case a test named by its path, so
`cargo test --test kramdown_suite block/04_header` runs only those, and
`cargo test --test kramdown_suite extensions/tables/` one extension's.

## Where they're from

`testcases/` holds the inputs of [kramdown 2.5.2's test cases][testcases]:
every `.text` file, with the `.options` files and the directory-level
`options` file, copied unchanged from the `kramdown` 2.5.2 gem's
`test/testcases`. kramdown is by Thomas Leitner, under the MIT license; its
notice is in [`COPYING`](COPYING).

The `.html` beside each `.text` isn't kramdown's own. It's made by
[`generate.rb`](generate.rb) with kramdown 2.5.2 and
[kramdown-parser-gfm 1.1.0][gfm], reading GFM as Jekyll does, so it's what a
Jekyll site would show. Its header says exactly how.

pulldown-kram's own cases are in two places, each named by its path under
the suite:

- `extensions/` has a directory for each of pulldown-kram's extensions,
  named after it without `ENABLE_`, so `extensions/tables/` holds what
  `ENABLE_TABLES` reads. Its cases cover what the extension gets right and
  what it doesn't yet, so the known failures under a directory show how
  much of that extension is still missing. A directory that names no
  extension stops the suite.
- `core/` holds what's read with no extension, where pulldown-cmark's
  CommonMark and kramdown agree or differ.

Each is converted with kramdown's defaults, as Jekyll converts, unless an
`.options` file beside it or an `options` file in its directory gives
others. To add one, write its `.text` there and regenerate.

[testcases]: https://github.com/gettalong/kramdown/tree/REL_2_5_2/test/testcases
[gfm]: https://github.com/kramdown/parser-gfm/tree/REL_1_1_0

## Files

| File | What it is |
|---|---|
| `testcases/` | kramdown's inputs and options, and the HTML made of them |
| `extensions/` | pulldown-kram's inputs for each extension, and the HTML kramdown makes of them |
| `core/` | pulldown-kram's inputs for what's read with no extension, and the HTML kramdown makes of them |
| `COPYING` | kramdown's license |
| `generate.rb` | makes the HTML, `cases.txt` and `entities.txt` |
| `cases.txt` | each case, the extensions to read it without, and the unsupported options kramdown used its defaults for |
| `entities.txt` | kramdown's named entities, to decode its HTML |
| `known-failures.txt` | the cases pulldown-kram reads differently, with why |
| `main.rs` | the test harness, making a test of each case |
| `normalise.rs` | makes both sides' HTML comparable, erasing only formatting |
| `diff.rs` | shows how a case differs |

## Regenerating

With the `kramdown` 2.5.2 and `kramdown-parser-gfm` 1.1.0 gems installed:

```sh
ruby tests/kramdown_suite/generate.rb
```

`generate.rb` sorts every kramdown option the cases give into those
pulldown-kram follows with an extension, those that change nothing compared,
and those it doesn't support, for which kramdown uses its default instead.
An option it hasn't sorted stops it. When pulldown-kram comes to support one
with an extension, moving it into `EXTENSIONS` and running this again is all
it takes.

## Known failures

A case listed in `known-failures.txt` has its test marked `[known failure]`,
which passes while the case fails. The suite fails when a case fails that
isn't listed, a listed case passes, or a listed case isn't one. To accept
the cases failing now, keeping the reasons already given:

```sh
UPDATE_KRAMDOWN_SUITE=1 cargo test --test kramdown_suite
```

A case that fails unlisted shows its Markdown, each line after a `|`, and
how pulldown-kram's HTML differs from kramdown's, both normalised: `-` for
kramdown's, `+` for pulldown-kram's. To see a listed case's, name it in
`KRAMDOWN_SUITE_SHOW`, with commas between cases, to check it as if it
weren't listed:

```sh
KRAMDOWN_SUITE_SHOW=man/example cargo test --test kramdown_suite -- --exact man/example
```

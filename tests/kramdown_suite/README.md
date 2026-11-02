# kramdown's test cases

kramdown's own test cases, read with pulldown-kram and checked against the
HTML kramdown makes of them, as pulldown-cmark checks itself against the
CommonMark spec. `cargo test --test kramdown_suite` runs them, each case a
test named by its path, so `cargo test --test kramdown_suite block/04_header`
runs only those.

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

[testcases]: https://github.com/gettalong/kramdown/tree/REL_2_5_2/test/testcases
[gfm]: https://github.com/kramdown/parser-gfm/tree/REL_1_1_0

## Files

| File | What it is |
|---|---|
| `testcases/` | kramdown's inputs and options, and the HTML made of them |
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
kramdown's, `+` for pulldown-kram's. To see a listed case's, take it off
the list for a run:

```sh
sed -i '' '/^man\/example /d' tests/kramdown_suite/known-failures.txt
cargo test --test kramdown_suite -- --exact man/example
git checkout tests/kramdown_suite/known-failures.txt
```

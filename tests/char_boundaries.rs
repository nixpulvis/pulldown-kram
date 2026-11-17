use pulldown_kram::{Document, Extensions};

#[test]
fn raw_html_span_with_non_ascii_text_does_not_panic() {
    for input in [
        "<kbd>Ctrl</b>日本</kbd>\n",
        "<code>x</em>日本</code>\n",
        "<kbd>x</p>é</kbd>\n",
    ] {
        Document::new(input, Extensions::all())
            .events()
            .for_each(drop);
    }
}

#[test]
fn link_definition_ending_with_non_ascii_url_does_not_panic() {
    for input in ["{:.x}\n[b]: /café\n", "[b]: /café\n\n{:.x}\n"] {
        Document::new(input, Extensions::all())
            .events()
            .for_each(drop);
    }
}

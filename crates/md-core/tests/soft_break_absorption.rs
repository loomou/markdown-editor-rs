use md_core::doc::Doc;
use md_core::document::{Caret, Command, Sel, editor_options, load_markdown};

fn end_of_first_leaf(doc: &Doc) -> Caret {
    let block = doc.text_leaves()[0];
    let offset = doc.text(block).map(|text| text.len()).unwrap_or(0);
    Caret { block, offset }
}

fn leaves(source: &str, keys: &[&str]) -> Vec<String> {
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    doc.enable_trailing_blank();
    let mut at = end_of_first_leaf(&doc);
    for key in keys {
        at = match *key {
            "se" => doc.apply(Sel::collapsed(at), Command::SoftBreak),
            "en" => doc.apply(Sel::collapsed(at), Command::Break),
            "l2" => Caret {
                block: doc.text_leaves()[0],
                offset: 4,
            },
            text => doc.apply(
                Sel::collapsed(at),
                Command::Insert {
                    text: text.to_string(),
                },
            ),
        };
    }
    doc.text_leaves()
        .into_iter()
        .filter_map(|id| doc.text(id).map(|text| text.to_string()))
        .collect()
}

#[test]
fn a_soft_break_next_to_another_newline_splits_the_paragraph() {
    for (source, keys, want) in [
        ("abc", &["se", "se", "xyz"][..], &["abc", "xyz", ""][..]),
        ("abc", &["se", "se"][..], &["abc", ""][..]),
        ("", &["se", "se", "xyz"][..], &["", "xyz", ""][..]),
        ("abc\nxyz", &["l2", "se"][..], &["abc", "xyz", ""][..]),
        ("- abc", &["se", "se", "xyz"][..], &["abc", "xyz", ""][..]),
        ("abc", &["se", "se", "se"][..], &["abc", "\n", ""][..]),
        (
            "abc",
            &["se", "se", "se", "xyz"][..],
            &["abc", "\nxyz", ""][..],
        ),
    ] {
        assert_eq!(leaves(source, keys), want, "{source:?} {keys:?}");
    }
}

#[test]
fn a_soft_break_that_stands_alone_keeps_its_paragraph() {
    for (source, keys, want) in [
        ("abc", &["se", "xyz"][..], &["abc\nxyz", ""][..]),
        ("abc", &["se", "en", "xyz"][..], &["abc\n", "xyz", ""][..]),
        ("abc", &["en", "se", "xyz"][..], &["abc", "\nxyz", ""][..]),
        ("- abc", &["se", "xyz"][..], &["abc\nxyz", ""][..]),
        ("> abc", &["se", "xyz"][..], &["abc\nxyz", ""][..]),
    ] {
        assert_eq!(leaves(source, keys), want, "{source:?} {keys:?}");
    }
}

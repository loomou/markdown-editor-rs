use crate::document::{Document, editor_options, load_markdown};

fn fixtures() -> Vec<(&'static str, &'static str)> {
    vec![
        ("empty doc", ""),
        ("single paragraph", "only one\n"),
        ("two paragraphs", "first\n\nsecond\n"),
        ("heading and paragraph", "# h1\n\npara\n\n## h2\n\ntail\n"),
        ("flat list", "- a\n- b\n- c\n"),
        ("nested list", "- a\n  - b\n    - c\n- d\n"),
        ("deep quote", "> > > deep\n\nafter\n"),
        ("multi-paragraph quote", "> one\n>\n> two\n\nout\n"),
        ("table", "| a | b |\n| --- | --- |\n| c | d |\n| e | f |\n"),
        (
            "table between paragraphs",
            "before\n\n| a | b |\n| --- | --- |\n| c | d |\n\nafter\n",
        ),
        ("code block", "para\n\n```rust\nfn main() {}\n```\n\ntail\n"),
        (
            "footnote definition",
            "text[^1]\n\n[^1]: the note\n\nafter\n",
        ),
        (
            "paragraph with a soft break",
            "term\n: definition\n\nafter\n",
        ),
        ("horizontal rule", "a\n\n---\n\nb\n"),
        ("image on its own line", "![alt](x.png)\n\nafter\n"),
        (
            "mixed content",
            "# h\n\npara\n\n- item\n  - nested\n\n> quote\n\n| a |\n| --- |\n| b |\n\n```\ncode\n```\n",
        ),
    ]
}

fn doc_of(md: &str) -> Document {
    load_markdown(md, editor_options())
}

#[test]
fn forward_walk_matches_text_leaves() {
    for (name, md) in fixtures() {
        let doc = doc_of(md);
        let expected = doc.text_leaves();

        let mut got = Vec::new();
        if let Some(first) = doc.first_text_leaf() {
            got.push(first);
            let mut cur = doc.live_id(first).expect("live first");
            while let Some(next) = doc.next_text_leaf(cur) {
                got.push(next.index);
                cur = next;
            }
        }

        assert_eq!(
            got, expected,
            "{name}: walking next does not match text_leaves"
        );
    }
}

#[test]
fn backward_walk_is_the_reverse() {
    for (name, md) in fixtures() {
        let doc = doc_of(md);
        let mut expected = doc.text_leaves();
        expected.reverse();

        let mut got = Vec::new();
        if let Some(&last) = doc.text_leaves().last() {
            got.push(last);
            let mut cur = doc.live_id(last).expect("live last");
            while let Some(prev) = doc.prev_text_leaf(cur) {
                got.push(prev.index);
                cur = prev;
            }
        }

        assert_eq!(
            got, expected,
            "{name}: walking prev is not the reverse order"
        );
    }
}

#[test]
fn next_then_prev_returns() {
    for (name, md) in fixtures() {
        let doc = doc_of(md);
        for leaf in doc.text_leaves() {
            let id = doc.live_id(leaf).expect("live");
            if let Some(next) = doc.next_text_leaf(id) {
                assert_eq!(
                    doc.prev_text_leaf(next).map(|n| n.index),
                    Some(leaf),
                    "{name}: leaf {leaf} did not return after next then prev"
                );
            }
        }
    }
}

#[test]
fn ends_have_no_neighbour() {
    for (name, md) in fixtures() {
        let doc = doc_of(md);
        let leaves = doc.text_leaves();
        let Some((&first, &last)) = leaves.first().zip(leaves.last()) else {
            assert!(
                doc.first_text_leaf().is_none(),
                "{name}: an empty doc must not have a first leaf"
            );
            continue;
        };
        let first_id = doc.live_id(first).expect("live");
        let last_id = doc.live_id(last).expect("live");
        assert!(
            doc.prev_text_leaf(first_id).is_none(),
            "{name}: the first leaf must not have a predecessor"
        );
        assert!(
            doc.next_text_leaf(last_id).is_none(),
            "{name}: the last leaf must not have a successor"
        );
    }
}

#[test]
fn first_text_leaf_matches_the_table() {
    for (name, md) in fixtures() {
        let doc = doc_of(md);
        assert_eq!(
            doc.first_text_leaf(),
            doc.text_leaves().first().copied(),
            "{name}"
        );
    }
}

#[test]
fn nth_from_matches_index_arithmetic() {
    for (name, md) in fixtures() {
        let doc = doc_of(md);
        let leaves = doc.text_leaves();
        for (i, &leaf) in leaves.iter().enumerate() {
            for delta in [-3i32, -2, -1, 0, 1, 2, 3] {
                let want = usize::try_from(i as i64 + i64::from(delta))
                    .ok()
                    .and_then(|j| leaves.get(j).copied());
                assert_eq!(
                    doc.nth_text_leaf_from(leaf, delta),
                    want,
                    "{name}: offset {delta} from leaf {i}"
                );
            }
        }
    }
}

#[test]
fn walk_is_consistent_after_an_edit() {
    use crate::document::{Caret, Command, Sel, apply};

    let mut doc = doc_of("one\n\ntwo\n\nthree\n\nfour\n");
    let leaves = doc.text_leaves();

    apply(
        &mut doc,
        Sel {
            anchor: Caret {
                block: leaves[1],
                offset: 0,
            },
            head: Caret {
                block: leaves[2],
                offset: 0,
            },
        },
        Command::DeleteBackward,
    );

    let after = doc.text_leaves();
    let mut got = Vec::new();
    if let Some(first) = doc.first_text_leaf() {
        got.push(first);
        let mut cur = doc.live_id(first).expect("live");
        while let Some(next) = doc.next_text_leaf(cur) {
            got.push(next.index);
            cur = next;
        }
    }
    assert_eq!(
        got, after,
        "after the edit, walking next does not match text_leaves"
    );
}

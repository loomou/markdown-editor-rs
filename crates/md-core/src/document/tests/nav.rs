use crate::document::{Document, editor_options, load_markdown};

fn fixtures() -> Vec<(&'static str, &'static str)> {
    vec![
        ("empty document", ""),
        ("one paragraph", "only one\n"),
        ("two paragraphs", "first\n\nsecond\n"),
        ("headings and paragraphs", "# h1\n\npara\n\n## h2\n\ntail\n"),
        ("flat list", "- a\n- b\n- c\n"),
        ("nested list", "- a\n  - b\n    - c\n- d\n"),
        ("deep quote", "> > > deep\n\nafter\n"),
        ("multi-paragraph quote", "> one\n>\n> two\n\nout\n"),
        ("table", "| a | b |\n| --- | --- |\n| c | d |\n| e | f |\n"),
        (
            "table sandwiched between paragraphs",
            "before\n\n| a | b |\n| --- | --- |\n| c | d |\n\nafter\n",
        ),
        ("code block", "para\n\n```rust\nfn main() {}\n```\n\ntail\n"),
        (
            "footnote definition",
            "text[^1]\n\n[^1]: the note\n\nafter\n",
        ),
        ("paragraph with soft break", "term\n: definition\n\nafter\n"),
        ("thematic break", "a\n\n---\n\nb\n"),
        ("image on its own line", "![alt](x.png)\n\nafter\n"),
        (
            "mixed",
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
            "{name}: the next-walk sequence disagrees with text_leaves"
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
            "{name}: the prev-walk sequence is not the reverse"
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
                    "{name}: from leaf {leaf}, forward then back did not return"
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
                "{name}: an empty document must have no first leaf"
            );
            continue;
        };
        let first_id = doc.live_id(first).expect("live");
        let last_id = doc.live_id(last).expect("live");
        assert!(
            doc.prev_text_leaf(first_id).is_none(),
            "{name}: the first leaf must have no predecessor"
        );
        assert!(
            doc.next_text_leaf(last_id).is_none(),
            "{name}: the last leaf must have no successor"
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
                    "{name}: offset {delta} from the {i}th leaf"
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
        "after the edit, the next-walk sequence disagrees with text_leaves"
    );
}

#[test]
fn reading_order_matches_the_leaf_table() {
    for (name, md) in fixtures() {
        let doc = doc_of(md);
        let leaves = doc.text_leaves();
        for (i, &a) in leaves.iter().enumerate() {
            for (j, &b) in leaves.iter().enumerate() {
                let want = i.cmp(&j);
                assert_eq!(
                    doc.cmp_reading_order(a, b),
                    want,
                    "{name}: leaves {i} and {j}"
                );
            }
        }
    }
}

#[test]
fn front_split_sorts_before_despite_higher_slot() {
    let mut doc = doc_of(
        &(0..8)
            .map(|i| format!("paragraph {i}\n\n"))
            .collect::<String>(),
    );
    let first = doc.first_text_leaf().expect("leaf");
    let (_, inserted) = doc.split_leaf(first, 0);
    let leaves = doc.text_leaves();
    let last = *leaves.last().expect("last");
    assert!(
        inserted > last,
        "fixture premise: the new block's slot index must exceed the old tail block's"
    );
    assert_eq!(
        doc.cmp_reading_order(inserted, last),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        doc.cmp_reading_order(last, inserted),
        std::cmp::Ordering::Greater
    );
    assert_eq!(leaves.iter().position(|&id| id == inserted), Some(1));
    assert_eq!(
        doc.cmp_reading_order(first, inserted),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        doc.cmp_reading_order(inserted, leaves[2]),
        std::cmp::Ordering::Less
    );
}

#[test]
fn ancestor_precedes_descendant() {
    let doc = doc_of("before\n\n> one\n>\n> two\n\nafter\n");
    let quote = doc
        .arena
        .children(doc.root)
        .find(|id| doc.arena.get(*id).expect("live").kind == crate::block::BlockKind::BlockQuote)
        .expect("quote");
    let inside = doc.text_leaves()[1];
    let before = doc.text_leaves()[0];
    let after = doc.text_leaves()[3];
    assert_eq!(
        doc.cmp_reading_order(quote.index, inside),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        doc.cmp_reading_order(before, quote.index),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        doc.cmp_reading_order(quote.index, after),
        std::cmp::Ordering::Less
    );
}

#[test]
fn dead_block_falls_back_to_slot_order() {
    let doc = doc_of("a\n\nb\n");
    let live = doc.first_text_leaf().expect("leaf");
    let dead = doc.text_leaves().iter().max().copied().unwrap() + 1000;
    assert_eq!(
        doc.cmp_reading_order(dead, live),
        std::cmp::Ordering::Greater
    );
    assert_eq!(doc.cmp_reading_order(dead, dead), std::cmp::Ordering::Equal);
}

use md_core::doc::Doc;
use md_core::document::{Caret, Command, Sel, editor_options, load_markdown};

#[test]
fn legal_references_edit_at_display_offset() {
    for (name, source) in [
        ("collapsed", "[label][] tail\n\n[label]: /target\n"),
        ("case", "[label][REF] tail\n\n[ref]: /target\n"),
        (
            "spaces",
            "[label][two   words] tail\n\n[two words]: /target\n",
        ),
        ("exact-control", "[label][ref] tail\n\n[ref]: /target\n"),
    ] {
        let mut doc = Doc::new(load_markdown(source, editor_options()));
        let block = doc.first_text_leaf().unwrap();
        let at = doc.retarget_focus(Caret { block, offset: 7 });
        doc.apply(Sel::collapsed(at), Command::Insert { text: "X".into() });
        let after = doc.text(block).unwrap();
        let src = doc
            .document
            .block_source(doc.document.live_id(block).unwrap());
        println!("CASE {name}\nafter={after:?}\nsource={src:?}");
        assert_eq!(
            after, "label tXail",
            "case {name} must edit inside the visible text (source={src:?})"
        );
        assert!(
            !src.contains("]X["),
            "case {name} must not break the link (source={src:?})"
        );
    }
}

#[test]
fn reference_case_mapping_keeps_edit_outside_link() {
    let mut d = load_markdown("[a][REF] tail\n\n[ref]: /target\n", editor_options());
    let id = d.first_text_leaf().unwrap();
    assert_eq!(d.text_of(id), Some("a tail"));
    d.replace_text(id, 2..3, "T");
    assert_eq!(d.text_of(id), Some("a Tail"));
    assert_eq!(d.link_at(d.live_id(id).unwrap(), 0), Some("/target"));
}

#[test]
fn collapsed_reference_mapping_keeps_edit_outside_link() {
    let mut d = load_markdown("[ref][] tail\n\n[ref]: /target\n", editor_options());
    let id = d.first_text_leaf().unwrap();
    assert_eq!(d.text_of(id), Some("ref tail"));
    d.replace_text(id, 4..5, "T");
    assert_eq!(d.text_of(id), Some("ref Tail"));
    assert_eq!(d.link_at(d.live_id(id).unwrap(), 0), Some("/target"));
}

use super::super::{DocShapeMaps, jobs::extra_snapshots};
use md_core::block::BlockKind;
use md_core::doc::Doc;
use md_core::document::edit::{Command, Sel, apply};
use md_core::document::{Caret, Document, editor_options, load_markdown};
use std::collections::HashMap;
use std::path::PathBuf;

fn doc_of(src: &str) -> Doc {
    Doc::new(load_markdown(src, editor_options()))
}

fn synced(src: &str) -> (Doc, DocShapeMaps) {
    let doc = doc_of(src);
    let mut maps = DocShapeMaps::empty();
    maps.sync(&doc);
    (doc, maps)
}

fn type_at(doc: &mut Doc, leaf: u32, text: &str) {
    let at = Caret {
        block: leaf,
        offset: 0,
    };
    let _ = apply(
        &mut doc.document,
        Sel::collapsed(at),
        Command::Insert { text: text.into() },
    );
}

fn first_of(doc: &Doc, kind: BlockKind) -> u32 {
    doc.document
        .preorder()
        .into_iter()
        .find(|id| doc.document.arena.get(*id).map(|n| n.kind) == Some(kind))
        .map(|id| id.index)
        .expect("block of that kind")
}

fn rebuilt(doc: &Doc) -> (HashMap<u32, String>, HashMap<u32, String>) {
    extra_snapshots(doc)
}

#[test]
fn text_edit_leaves_the_extra_maps_identical() {
    let src = "```rust\nlet a = 1;\n```\n\npara with ![alt](https://i/1.png) inline\n\n![solo](https://i/2.png)\n";
    let (mut doc, mut maps) = synced(src);
    let before = (maps.block_image_dest.clone(), maps.block_code_lang.clone());
    assert!(!before.1.is_empty(), "the lang table must not be empty");

    let leaf = first_of(&doc, BlockKind::Paragraph);
    type_at(&mut doc, leaf, "X");
    assert!(
        doc.document.pending_changes().is_text_only(),
        "this edit must count as plain text"
    );
    maps.sync(&doc);

    let after = rebuilt(&doc);
    assert_eq!(*maps.block_image_dest, after.0);
    assert_eq!(*maps.block_code_lang, after.1);
    assert!(std::rc::Rc::ptr_eq(&maps.block_image_dest, &before.0));
    assert!(std::rc::Rc::ptr_eq(&maps.block_code_lang, &before.1));
}

#[test]
fn opening_a_fence_updates_the_lang_map() {
    let (mut doc, mut maps) = synced("text\n");
    assert!(maps.block_code_lang.is_empty());

    let leaf = first_of(&doc, BlockKind::Paragraph);
    let n = doc.document.text_of(leaf).map(str::len).unwrap_or(0);
    let at = Caret {
        block: leaf,
        offset: 0,
    };
    let _ = apply(
        &mut doc.document,
        Sel {
            anchor: at,
            head: Caret {
                block: leaf,
                offset: n,
            },
        },
        Command::Insert {
            text: "```rust".into(),
        },
    );
    let at = Caret {
        block: leaf,
        offset: 7,
    };
    let _ = apply(&mut doc.document, Sel::collapsed(at), Command::Break);
    assert_eq!(doc.document.kind(leaf), Some(BlockKind::CodeBlock));

    maps.sync(&doc);
    assert_eq!(
        maps.block_code_lang.get(&leaf).map(String::as_str),
        Some("rust")
    );
    assert_eq!(*maps.block_code_lang, rebuilt(&doc).1);
}

#[test]
fn link_dests_grow_by_append_and_match_a_full_rebuild() {
    let (mut doc, mut maps) = synced("see [a](https://e/1) and ![i](https://i/1.png)\n");
    let start = maps.link_dests.len();
    assert_eq!(start, doc.document.links.len());

    let leaf = first_of(&doc, BlockKind::Paragraph);
    for i in 0..5 {
        type_at(&mut doc, leaf, if i % 2 == 0 { "X" } else { "Y" });
        maps.sync(&doc);
        assert_eq!(
            maps.link_dests.len(),
            doc.document.links.len(),
            "the level did not keep up after step {i}"
        );
    }
    assert!(
        doc.document.links.len() > start,
        "typing must grow the links table, or this case tests nothing"
    );

    let mut full = HashMap::new();
    let path = doc.source_path.as_deref();
    for (i, link) in doc.document.links.iter().enumerate() {
        full.insert(i as u32, md_content::images::cache_key(&link.dest, path));
    }
    assert_eq!(*maps.link_dests, full);
}

#[test]
fn data_urls_keep_only_a_compact_key_in_shape_maps() {
    let raw = "data:image/png;base64,AAAA";
    let (_doc, maps) = synced(&format!("![]({raw})\n"));
    let key = maps.link_dests.get(&0).expect("data key");
    assert_eq!(key.len(), 21);
    assert_ne!(key, raw);
    assert_eq!(maps.data_source_links.get(key), Some(&0));
    assert_eq!(maps.link_raw.get(&0).map(|v| v.0.as_str()), Some(raw));
}

#[test]
fn a_slot_that_changes_kind_drops_its_stale_lang() {
    let src = "para\n\n```rust\nlet a = 1;\n```\n\ntail\n";
    let (mut doc, mut maps) = synced(src);
    let code = first_of(&doc, BlockKind::CodeBlock);
    assert_eq!(
        maps.block_code_lang.get(&code).map(String::as_str),
        Some("rust")
    );

    let n = doc.document.text_of(code).map(str::len).unwrap_or(0);
    let _ = apply(
        &mut doc.document,
        Sel {
            anchor: Caret {
                block: code,
                offset: 0,
            },
            head: Caret {
                block: code,
                offset: n,
            },
        },
        Command::DeleteBackward,
    );
    let _ = apply(
        &mut doc.document,
        Sel::collapsed(Caret {
            block: code,
            offset: 0,
        }),
        Command::DeleteBackward,
    );
    assert_eq!(
        doc.document.kind(code),
        Some(BlockKind::Paragraph),
        "an emptied code block must revert to a paragraph and keep its old slot"
    );

    maps.sync(&doc);
    assert!(
        !maps.block_code_lang.contains_key(&code),
        "slot {code} is already a paragraph, yet the lang table still lists rust — the new paragraph would be highlighted as rust"
    );
    assert_eq!(*maps.block_code_lang, rebuilt(&doc).1);
}

#[test]
fn changing_the_source_path_recomputes_every_link() {
    let mut doc = Doc::with_path(
        load_markdown("see [a](./sub/x.md)\n", editor_options()),
        Some(PathBuf::from("/one/here.md")),
    );
    let mut maps = DocShapeMaps::empty();
    maps.sync(&doc);
    let first = maps.link_dests.get(&0).cloned().expect("one link");

    doc.source_path = Some(PathBuf::from("/two/there.md"));
    maps.sync(&doc);
    let second = maps.link_dests.get(&0).cloned().expect("one link");
    assert_ne!(
        first, second,
        "changing the source directory must change the relative-link keys"
    );

    let mut full = HashMap::new();
    for (i, link) in doc.document.links.iter().enumerate() {
        full.insert(
            i as u32,
            md_content::images::cache_key(&link.dest, doc.source_path.as_deref()),
        );
    }
    assert_eq!(*maps.link_dests, full);
}

#[test]
fn appending_does_not_clone_the_whole_table() {
    let (mut doc, mut maps) = synced("see [a](https://e/1)\n");
    let before = maps.link_dests.clone();
    let leaf = first_of(&doc, BlockKind::Paragraph);
    type_at(&mut doc, leaf, "X");
    let grew = doc.document.links.len() > before.len();
    drop(before);
    assert!(grew, "typing must grow the links table");

    let ptr = std::rc::Rc::as_ptr(&maps.link_dests);
    maps.sync(&doc);
    assert_eq!(
        ptr,
        std::rc::Rc::as_ptr(&maps.link_dests),
        "with no outside holder, appending must grow the table in place instead of cloning it"
    );
}

#[test]
fn an_unchanged_document_is_not_touched_twice() {
    let (doc, mut maps) = synced("```rust\nx\n```\n\npara ![i](https://i/1.png)\n");
    let before = (
        maps.link_dests.clone(),
        maps.block_image_dest.clone(),
        maps.block_code_lang.clone(),
    );
    maps.sync(&doc);
    assert!(std::rc::Rc::ptr_eq(&maps.link_dests, &before.0));
    assert!(std::rc::Rc::ptr_eq(&maps.block_image_dest, &before.1));
    assert!(std::rc::Rc::ptr_eq(&maps.block_code_lang, &before.2));
}

#[test]
fn a_pasted_code_block_lands_in_the_lang_map() {
    let (mut doc, mut maps) = synced("head\n");
    assert!(maps.block_code_lang.is_empty());

    let leaf = first_of(&doc, BlockKind::Paragraph);
    let _ = doc.document.paste(
        leaf,
        4..4,
        "```python\nprint(1)\n```\n",
        md_core::document::PasteIntent::IndependentFragment,
    );
    maps.sync(&doc);

    assert_eq!(*maps.block_code_lang, rebuilt(&doc).1);
    assert!(
        maps.block_code_lang.values().any(|l| l == "python"),
        "the pasted code block must bring python into the table"
    );
}

#[test]
fn a_fresh_document_matches_the_old_three_pass_answer() {
    let src = "```rust\na\n```\n\n![solo](https://i/1.png)\n\npara [l](https://e/1) ![i](https://i/2.png)\n\n```\nno lang\n```\n\n> quote ![q](https://i/3.png)\n\n| a | b |\n| - | - |\n| c | ![t](https://i/4.png) |\n";
    let (doc, maps) = synced(src);

    let path = doc.source_path.as_deref();
    let mut links = HashMap::new();
    for (i, link) in doc.document.links.iter().enumerate() {
        links.insert(i as u32, md_content::images::cache_key(&link.dest, path));
    }
    let mut images = HashMap::new();
    for id in doc.document.preorder() {
        if let Some(d) = doc.document.extra(id).image_dest() {
            let dest = doc.document.link_dest(d).unwrap_or("");
            images.insert(id.index, md_content::images::cache_key(dest, path));
        }
    }
    let mut langs = HashMap::new();
    for id in doc.document.preorder() {
        if let Some(l) = doc.document.extra(id).code_fence_lang() {
            langs.insert(
                id.index,
                doc.document
                    .lang(l)
                    .unwrap_or("")
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase(),
            );
        }
    }

    assert_eq!(*maps.link_dests, links);
    assert_eq!(*maps.block_image_dest, images);
    assert_eq!(*maps.block_code_lang, langs);
    assert!(
        !images.is_empty(),
        "the fixture must contain a standalone image"
    );
    assert!(
        !langs.is_empty(),
        "the fixture must contain a code block with a lang"
    );
}

#[test]
fn for_each_extra_visits_in_preorder() {
    let src = "# h\n\n> q\n\n- a\n  - b\n\n| x | y |\n| - | - |\n| 1 | 2 |\n\n```rust\nc\n```\n";
    let doc: Document = load_markdown(src, editor_options());
    let expect: Vec<u32> = doc.preorder().into_iter().map(|id| id.index).collect();
    let mut got = Vec::new();
    doc.for_each_extra(|index, _| got.push(index));
    assert_eq!(got, expect);
}

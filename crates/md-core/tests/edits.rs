use md_core::doc::Doc;
use md_core::document::{Caret, Command, Sel, editor_options, load_markdown};
use pulldown_cmark::{Event, Parser};
use std::time::Instant;

#[test]
fn multiline_indent_reveals_complete_construct() {
    let mut d = Doc::new(load_markdown("plain\n**bold**\n", editor_options()));
    let id = d.first_text_leaf().unwrap();
    let len = d.text(id).unwrap().len();
    d.apply(
        Sel {
            anchor: Caret {
                block: id,
                offset: 0,
            },
            head: Caret {
                block: id,
                offset: len,
            },
        },
        Command::Indent,
    );
    let off = d.text(id).unwrap().find("bold").unwrap() + 1;
    d.retarget_focus(Caret {
        block: id,
        offset: off,
    });
    println!(
        "focused saved={:?}; display={:?}",
        d.document.to_markdown(),
        d.text(id)
    );
    assert_eq!(d.text(id), Some("\tplain\n\t**bold**"));
}

#[test]
fn multiline_indent_reveals_complete_chinese_construct() {
    let mut d = Doc::new(load_markdown("plain\n**€€** tail\n", editor_options()));
    let id = d.first_text_leaf().unwrap();
    let len = d.text(id).unwrap().len();
    d.apply(
        Sel {
            anchor: Caret {
                block: id,
                offset: 0,
            },
            head: Caret {
                block: id,
                offset: len,
            },
        },
        Command::Indent,
    );
    let off = d.text(id).unwrap().find("€€").unwrap() + 1;
    d.retarget_focus(Caret {
        block: id,
        offset: off,
    });
    println!("chinese display={:?}", d.text(id));
    assert_eq!(d.text(id), Some("\tplain\n\t**€€** tail"));
}

#[test]
fn typing_after_multiline_reveal_keeps_delimiters() {
    let mut d = Doc::new(load_markdown("plain\n**bold**\n", editor_options()));
    let id = d.first_text_leaf().unwrap();
    let len = d.text(id).unwrap().len();
    d.apply(
        Sel {
            anchor: Caret {
                block: id,
                offset: 0,
            },
            head: Caret {
                block: id,
                offset: len,
            },
        },
        Command::Indent,
    );
    let off = d.text(id).unwrap().find("bold").unwrap() + 1;
    d.retarget_focus(Caret {
        block: id,
        offset: off,
    });
    assert_eq!(d.text(id), Some("\tplain\n\t**bold**"));
    let revealed_off = d.text(id).unwrap().find("bold").unwrap() + 1;
    let cursor = d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: revealed_off,
        }),
        Command::Insert { text: "X".into() },
    );
    println!("after typing display={:?}; cursor={cursor:?}", d.text(id));
    assert_eq!(d.text(id), Some("\tplain\n\t**bXold**"));
}

#[test]
fn indent_refreshes_active_inline_focus() {
    let mut d = Doc::new(load_markdown("a **b** €\n", editor_options()));
    let block = d.first_text_leaf().unwrap();
    let focus = d.retarget_focus(Caret { block, offset: 2 });
    assert_eq!(
        d.text(block),
        Some("a **b** €"),
        "fixture: construct revealed"
    );
    let caret = d.apply(
        Sel {
            anchor: Caret { block, offset: 0 },
            head: Caret { block, offset: 8 },
        },
        Command::Indent,
    );
    println!(
        "focus={focus:?}; caret={caret:?}; display={:?}; saved={:?}",
        d.text(block),
        d.document.to_markdown()
    );
    assert_eq!(d.text(block), Some("\ta **b** €"));
    assert!(
        d.caret_text(caret.block)
            .unwrap()
            .is_char_boundary(caret.offset)
    );
}

#[test]
fn delete_returns_utf8_boundary_after_reprojection() {
    let mut d = Doc::new(load_markdown("**€€** tail\n", editor_options()));
    let block = d.first_text_leaf().unwrap();
    assert!(d.text(block).unwrap().is_char_boundary(3));
    let caret = d.apply(
        Sel::collapsed(Caret { block, offset: 3 }),
        Command::DeleteForward,
    );
    let text = d.caret_text(caret.block).unwrap();
    println!(
        "caret={caret:?}; display={text:?}; saved={:?}",
        d.document.to_markdown()
    );
    assert!(text.is_char_boundary(caret.offset));
}

#[test]
fn selection_delete_returns_utf8_boundary_after_reprojection() {
    let mut d = Doc::new(load_markdown("**€€** tail\n", editor_options()));
    let block = d.first_text_leaf().unwrap();
    let sel = Sel {
        anchor: Caret { block, offset: 3 },
        head: Caret { block, offset: 6 },
    };
    let caret = d.apply(sel, Command::DeleteForward);
    let text = d.caret_text(caret.block).unwrap();
    println!("sel caret={caret:?}; display={text:?}");
    assert!(text.is_char_boundary(caret.offset));
}

#[test]
fn cell_delete_returns_utf8_boundary() {
    let mut d = Doc::new(load_markdown(
        "| h |\n| --- |\n| **ǟ**x |\n",
        editor_options(),
    ));
    let cell = d.text_leaves()[1];
    let text = d.text(cell).unwrap().to_string();
    assert_eq!(text, "ǟx");
    let caret = d.apply(
        Sel::collapsed(Caret {
            block: cell,
            offset: 0,
        }),
        Command::DeleteForward,
    );
    let text = d.caret_text(caret.block).unwrap();
    println!("cell caret={caret:?}; display={text:?}");
    assert!(text.is_char_boundary(caret.offset));
}

#[test]
fn image_inside_emphasis_keeps_its_own_focus() {
    let mut doc = Doc::new(load_markdown(
        "**pre ![alt](/image) post**\n",
        editor_options(),
    ));
    let block = doc.first_text_leaf().unwrap();
    let offset = doc.text(block).unwrap().find("alt").unwrap() + 1;
    doc.retarget_focus(Caret { block, offset });
    println!(
        "A visual={:?}\nimage={:?}",
        doc.text(block),
        doc.revealed_image()
    );
    let image = doc
        .revealed_image()
        .expect("revealing must keep the image preview");
    assert_eq!(
        image.display,
        6..20,
        "the preview must cover exactly the image"
    );
    assert_eq!(image.dest, "/image");

    let mut doc = Doc::new(load_markdown("**![alt](/image) post**\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    let offset = doc.text(block).unwrap().find("alt").unwrap() + 1;
    doc.retarget_focus(Caret { block, offset });
    let image = doc.revealed_image().expect("image focus must resolve");
    println!("B visual={:?}\nimage={image:?}", doc.text(block));
    assert_eq!(
        image.display,
        2..16,
        "the image range must not cover the whole emphasis span"
    );
    assert_eq!(image.dest, "/image");

    let mut doc = Doc::new(load_markdown("![alt](/image) tail\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    let offset = doc.text(block).unwrap().find("alt").unwrap() + 1;
    doc.retarget_focus(Caret { block, offset });
    let image = doc.revealed_image().expect("plain inline image");
    println!("C visual={:?}\nimage={image:?}", doc.text(block));
    assert_eq!(image.display, 0..14);
    assert_eq!(image.dest, "/image");
}

#[test]
fn image_inside_emphasis_keeps_reveal_preview() {
    let mut d = Doc::new(load_markdown(
        "**before ![alt](image.png) after**\n",
        editor_options(),
    ));
    let id = d.first_text_leaf().unwrap();
    let _c = d.retarget_focus(Caret {
        block: id,
        offset: 8,
    });
    assert_eq!(
        d.revealed_image().map(|image| image.dest),
        Some("image.png")
    );
}

#[test]
fn image_url_edit_invalidates_extra_maps() {
    let mut d = load_markdown("![alt](old.png)\n", editor_options());
    let id = d.first_text_leaf().unwrap();
    let node = d.live_id(id).unwrap();
    assert_eq!(d.kind(id), Some(md_core::block::BlockKind::Image));
    d.take_changes();
    let change = d.replace_text(id, 7..14, "new.png");
    let dest = d.link_dest(d.extra(node).image_dest().unwrap());
    println!(
        "saved={:?}; dest={dest:?}; change={change:#?}",
        d.to_markdown()
    );
    assert_eq!(dest, Some("new.png"), "the extra itself is updated");
    assert!(
        !change.is_text_only(),
        "image extra changed, so extra-map consumers must be invalidated"
    );
}

#[test]
fn undo_redo_image_url_edit_round_trips_dest() {
    let mut d = Doc::new(load_markdown("![alt](old.png)\n", editor_options()));
    let id = d.first_text_leaf().unwrap();
    let node = d.document.live_id(id).unwrap();
    d.take_changes();
    d.apply(
        Sel {
            anchor: Caret {
                block: id,
                offset: 7,
            },
            head: Caret {
                block: id,
                offset: 14,
            },
        },
        Command::Insert {
            text: "new.png".into(),
        },
    );
    assert_eq!(
        d.document
            .link_dest(d.document.extra(node).image_dest().unwrap()),
        Some("new.png")
    );

    let undone = d.undo();
    println!("undone={undone:#?}");
    assert!(undone.is_some());
    assert!(
        !d.document.pending_changes().is_text_only(),
        "undo must also invalidate extra maps"
    );
    assert_eq!(
        d.document
            .link_dest(d.document.extra(node).image_dest().unwrap()),
        Some("old.png"),
        "undo restores the old dest in the extra"
    );

    let redone = d.redo();
    assert!(redone.is_some());
    assert!(
        !d.document.pending_changes().is_text_only(),
        "redo must also invalidate extra maps"
    );
    assert_eq!(
        d.document
            .link_dest(d.document.extra(node).image_dest().unwrap()),
        Some("new.png"),
        "redo reapplies the new dest in the extra"
    );
}

#[test]
fn half_typed_url_flips_extra_both_ways() {
    let mut d = Doc::new(load_markdown("![alt](old.png)\n", editor_options()));
    let id = d.first_text_leaf().unwrap();
    let node = d.document.live_id(id).unwrap();
    d.take_changes();
    let change = d.document.replace_text(id, 7..15, "");
    println!("half={change:#?}");
    assert!(
        !change.is_text_only(),
        "losing the dest must invalidate extra maps"
    );
    assert!(d.document.extra(node).image_dest().is_none());

    let change = d.document.replace_text(id, 7..7, "new.png)");
    println!("completed={change:#?}");
    assert!(
        !change.is_text_only(),
        "regaining the dest must invalidate extra maps"
    );
    assert_eq!(
        d.document
            .link_dest(d.document.extra(node).image_dest().unwrap()),
        Some("new.png")
    );
    d.take_changes();
    d.apply(
        Sel {
            anchor: Caret {
                block: id,
                offset: 7,
            },
            head: Caret {
                block: id,
                offset: 15,
            },
        },
        Command::DeleteBackward,
    );
    assert!(d.document.extra(node).image_dest().is_none());
    assert!(
        !d.document.pending_changes().is_text_only(),
        "deleting the whole URL must invalidate extra maps"
    );
    assert!(d.undo().is_some());
    assert!(
        !d.document.pending_changes().is_text_only(),
        "undoing the URL deletion must invalidate extra maps"
    );
    assert_eq!(
        d.document
            .link_dest(d.document.extra(node).image_dest().unwrap()),
        Some("new.png"),
        "undo restores the dest that existed before the URL deletion"
    );
}

#[test]
fn copy_plain_prefix_excludes_unselected_reference() {
    let d = Doc::new(load_markdown(
        "public [link][r]\n\n[r]: /private/path\n",
        editor_options(),
    ));
    let id = d.first_text_leaf().unwrap();
    let copied = d.copy_markdown(Sel {
        anchor: Caret {
            block: id,
            offset: 0,
        },
        head: Caret {
            block: id,
            offset: 6,
        },
    });
    println!("copied={copied:?}");
    assert_eq!(copied, "public");
}

#[test]
fn copy_link_keeps_its_reference_definition() {
    let d = Doc::new(load_markdown(
        "public [link][r]\n\n[r]: /private/path\n",
        editor_options(),
    ));
    let id = d.first_text_leaf().unwrap();
    let len = d.text(id).unwrap().len();
    let copied = d.copy_markdown(Sel {
        anchor: Caret {
            block: id,
            offset: 0,
        },
        head: Caret {
            block: id,
            offset: len,
        },
    });
    println!("full={copied:?}");
    assert!(
        copied.contains("[r]: /private/path"),
        "copying the link itself must carry its definition"
    );
}

#[test]
fn copy_span_keeps_only_selected_links_definitions() {
    let d = Doc::new(load_markdown(
        "a [x][rx]\n\nb [y][ry]\n\n[rx]: /x\n\n[ry]: /y\n",
        editor_options(),
    ));
    let ids = d.text_leaves();
    let first = d.text(ids[0]).unwrap();
    let link_start = first.find('x').unwrap();
    let copied = d.copy_markdown(Sel {
        anchor: Caret {
            block: ids[0],
            offset: link_start,
        },
        head: Caret {
            block: ids[0],
            offset: link_start + 1,
        },
    });
    println!("partial={copied:?}");
    assert!(
        copied.contains("[rx]: /x"),
        "the selected link's definition must ride along"
    );
    assert!(
        !copied.contains("[ry]: /y"),
        "the other leaf's unselected link must not leak its definition"
    );
}

#[test]
fn copy_maximum_ordered_list_keeps_two_items() {
    let d = Doc::new(load_markdown(
        "999999999. a\n999999999. b\n",
        editor_options(),
    ));
    let ids = d.text_leaves();
    let copied = d.copy_markdown(Sel {
        anchor: Caret {
            block: ids[0],
            offset: 0,
        },
        head: Caret {
            block: ids[1],
            offset: 1,
        },
    });
    let items = Parser::new_ext(&copied, editor_options())
        .filter(|e| matches!(e, Event::Start(pulldown_cmark::Tag::Item)))
        .count();
    println!("copied={copied:?}; items={items}");
    assert_eq!(items, 2);
}

#[test]
fn copy_ordered_list_increments_normally() {
    let d = Doc::new(load_markdown("3. a\n3. b\n3. c\n", editor_options()));
    let ids = d.text_leaves();
    let copied = d.copy_markdown(Sel {
        anchor: Caret {
            block: ids[0],
            offset: 0,
        },
        head: Caret {
            block: ids[2],
            offset: 1,
        },
    });
    println!("normal={copied:?}");
    assert_eq!(copied, "3. a\n4. b\n5. c");
}

#[test]
fn capped_copy_is_a_fixed_point() {
    let d = Doc::new(load_markdown(
        "999999999. a\n999999999. b\n",
        editor_options(),
    ));
    let ids = d.text_leaves();
    let copied = d.copy_markdown(Sel {
        anchor: Caret {
            block: ids[0],
            offset: 0,
        },
        head: Caret {
            block: ids[1],
            offset: 1,
        },
    });
    assert_eq!(copied, "999999999. a\n999999999. b");
    let d2 = Doc::new(load_markdown(&copied, editor_options()));
    let ids2 = d2.text_leaves();
    let again = d2.copy_markdown(Sel {
        anchor: Caret {
            block: ids2[0],
            offset: 0,
        },
        head: Caret {
            block: ids2[1],
            offset: 1,
        },
    });
    println!("again={again:?}");
    assert_eq!(again, copied, "capped copy must be a fixed point");
}

#[test]
fn fence_length_probing_stays_linear() {
    for size in [4000usize, 8000, 16000, 32000] {
        let source = format!("```\nx{}y\n```\n", "`".repeat(size));
        let d = load_markdown(&source, editor_options());
        let start = Instant::now();
        let saved = d.to_markdown();
        let elapsed = start.elapsed();
        println!(
            "FENCE body_ticks={size}; output_bytes={}; elapsed_us={}",
            saved.len(),
            elapsed.as_micros()
        );
        assert!(!saved.is_empty());
        assert!(
            elapsed.as_millis() < 1000,
            "serializing a {size}-tick body must stay linear (took {elapsed:?})"
        );
        let fence_len = 4001.max(size + 1);
        assert_eq!(
            saved,
            format!(
                "{}``\nx{}y\n{}``\n",
                "`".repeat(fence_len - 2),
                "`".repeat(size),
                "`".repeat(fence_len - 2)
            ),
            "fence must exceed the longest in-body run by one (N={size})"
        );
    }
}

#[test]
fn fence_keeps_minimum_length_without_long_runs() {
    let d = load_markdown("```\nplain body\n```\n", editor_options());
    let saved = d.to_markdown();
    println!("plain={saved:?}");
    assert_eq!(saved, "```\nplain body\n```\n");
}

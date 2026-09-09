use md_core::block::{AlertKind, BlockKind};
use md_core::doc::Doc;
use md_core::document::{Caret, Command, PasteIntent, Sel, editor_options, load_markdown};
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

#[test]
fn html_block_insert_uses_the_visible_text_offset() {
    let mut doc = Doc::new(load_markdown("<div>hello</div>\n", editor_options()));
    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Caret { block, offset: 1 });
    doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    assert_eq!(doc.text(block), Some("hXello"));
}

#[test]
fn html_block_backspace_preserves_closing_tag() {
    let mut doc = Doc::new(load_markdown("<div>hello</div>\n", editor_options()));
    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Caret { block, offset: 1 });
    doc.apply(Sel::collapsed(caret), Command::DeleteBackward);
    assert_eq!(doc.document.to_markdown(), "<div>ello</div>\n");
}

#[test]
fn html_block_delete_forward_eats_visible_text() {
    let mut doc = Doc::new(load_markdown("<div>hello</div>\n", editor_options()));
    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Caret { block, offset: 0 });
    doc.apply(Sel::collapsed(caret), Command::DeleteForward);
    assert_eq!(doc.document.to_markdown(), "<div>ello</div>\n");
}

#[test]
fn html_block_multiline_maps_to_collapsed_visible_text() {
    let mut doc = Doc::new(load_markdown("<div>\nhello\n</div>\n", editor_options()));
    let block = doc.text_leaves()[0];
    assert_eq!(doc.text(block), Some("hello"));
    let caret = doc.retarget_focus(Caret { block, offset: 1 });
    doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    assert_eq!(doc.document.to_markdown(), "<div>\nhXello\n</div>\n");
}

#[test]
fn html_block_unicode_caret_lands_on_char_boundary() {
    let mut doc = Doc::new(load_markdown("<div>中文</div>\n", editor_options()));
    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Caret { block, offset: 3 });
    doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    assert_eq!(doc.document.to_markdown(), "<div>中X文</div>\n");
}

#[test]
fn html_block_undo_redo_preserves_tags() {
    let mut doc = Doc::new(load_markdown("<div>hello</div>\n", editor_options()));
    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Caret { block, offset: 1 });
    doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    assert_eq!(doc.document.to_markdown(), "<div>hXello</div>\n");
    doc.undo();
    assert_eq!(doc.document.to_markdown(), "<div>hello</div>\n");
    doc.redo();
    assert_eq!(doc.document.to_markdown(), "<div>hXello</div>\n");
}

#[test]
fn inline_html_text_edit_misses_the_tags() {
    let mut doc = Doc::new(load_markdown("a <b>bold</b> tail\n", editor_options()));
    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Caret { block, offset: 2 });
    doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    assert_eq!(doc.document.to_markdown(), "a <b>Xbold</b> tail\n");
}

#[test]
fn copied_footnote_reference_keeps_its_definition() {
    let d = Doc::new(load_markdown("note[^a]\n\n[^a]: body\n", editor_options()));
    let block = d.first_text_leaf().unwrap();
    let len = d.text(block).unwrap().len();
    let copied = d.copy_markdown(Sel {
        anchor: Caret { block, offset: 0 },
        head: Caret { block, offset: len },
    });
    println!("copied={copied:?}");
    assert!(
        copied.contains("[^a]: body"),
        "clipboard must carry the definition, got {copied:?}"
    );
}

#[test]
fn copied_footnote_definition_keeps_its_container() {
    let d = Doc::new(load_markdown("note[^a]\n\n[^a]: body\n", editor_options()));
    let ids = d.text_leaves();
    let last = ids.last().unwrap();
    let len = d.text(*last).unwrap().len();
    let copied = d.copy_markdown(Sel {
        anchor: Caret {
            block: ids[0],
            offset: 0,
        },
        head: Caret {
            block: *last,
            offset: len,
        },
    });
    println!("copied={copied:?}");
    assert!(
        copied.contains("[^a]: body"),
        "directly selected definition must keep its container, got {copied:?}"
    );
}

#[test]
fn copied_repeated_footnote_references_carry_one_definition() {
    let d = Doc::new(load_markdown(
        "x[^a] y[^a]\n\n[^a]: body\n",
        editor_options(),
    ));
    let block = d.first_text_leaf().unwrap();
    let len = d.text(block).unwrap().len();
    let copied = d.copy_markdown(Sel {
        anchor: Caret { block, offset: 0 },
        head: Caret { block, offset: len },
    });
    println!("copied={copied:?}");
    assert_eq!(
        copied.matches("[^a]: body").count(),
        1,
        "one definition per label, got {copied:?}"
    );
}

#[test]
fn copied_footnote_body_transits_its_own_references() {
    let d = Doc::new(load_markdown(
        "top[^a]\n\n[^a]: uses [^b]\n\n[^b]: leaf\n",
        editor_options(),
    ));
    let block = d.first_text_leaf().unwrap();
    let len = d.text(block).unwrap().len();
    let copied = d.copy_markdown(Sel {
        anchor: Caret { block, offset: 0 },
        head: Caret { block, offset: len },
    });
    println!("copied={copied:?}");
    assert!(
        copied.contains("[^a]: uses [^b]"),
        "definition a must be carried, got {copied:?}"
    );
    assert!(
        copied.contains("[^b]: leaf"),
        "transitive dependency b must be carried, got {copied:?}"
    );
}

#[test]
fn copied_self_referential_footnote_terminates() {
    let d = Doc::new(load_markdown(
        "top[^a]\n\n[^a]: loop [^a]\n",
        editor_options(),
    ));
    let block = d.first_text_leaf().unwrap();
    let len = d.text(block).unwrap().len();
    let copied = d.copy_markdown(Sel {
        anchor: Caret { block, offset: 0 },
        head: Caret { block, offset: len },
    });
    println!("copied={copied:?}");
    assert_eq!(
        copied.matches("[^a]: loop [^a]").count(),
        1,
        "cycle must terminate with exactly one definition, got {copied:?}"
    );
}

#[test]
fn copied_footnote_fragment_reloads_self_contained() {
    let d = Doc::new(load_markdown("note[^a]\n\n[^a]: body\n", editor_options()));
    let block = d.first_text_leaf().unwrap();
    let len = d.text(block).unwrap().len();
    let copied = d.copy_markdown(Sel {
        anchor: Caret { block, offset: 0 },
        head: Caret { block, offset: len },
    });
    let reloaded = load_markdown(&copied, editor_options());
    let saved = reloaded.to_markdown();
    println!("reloaded={saved:?}");
    assert!(
        saved.contains("[^a]: body"),
        "reloaded fragment must keep the definition, got {saved:?}"
    );
}

#[test]
fn copied_footnote_body_carries_its_link_definitions() {
    let d = Doc::new(load_markdown(
        "top[^a]\n\n[^a]: uses [l][r]\n\n[r]: /t\n",
        editor_options(),
    ));
    let block = d.first_text_leaf().unwrap();
    let len = d.text(block).unwrap().len();
    let copied = d.copy_markdown(Sel {
        anchor: Caret { block, offset: 0 },
        head: Caret { block, offset: len },
    });
    println!("copied={copied:?}");
    assert!(
        copied.contains("[^a]: uses [l][r]"),
        "footnote definition must be carried, got {copied:?}"
    );
    assert!(
        copied.contains("[r]: /t"),
        "link definition inside the note body must be carried, got {copied:?}"
    );
}

#[test]
fn copied_list_item_footnote_carries_definition() {
    let d = Doc::new(load_markdown(
        "- item[^a]\n\n[^a]: body\n",
        editor_options(),
    ));
    let block = d.first_text_leaf().unwrap();
    let len = d.text(block).unwrap().len();
    let copied = d.copy_markdown(Sel {
        anchor: Caret { block, offset: 0 },
        head: Caret { block, offset: len },
    });
    println!("copied={copied:?}");
    assert!(
        copied.contains("- item[^a]"),
        "list item must keep its marker, got {copied:?}"
    );
    assert!(
        copied.contains("[^a]: body"),
        "footnote referenced inside a list item must carry its definition, got {copied:?}"
    );
}

#[test]
fn collapsed_selection_still_copies_nothing() {
    let d = Doc::new(load_markdown("note[^a]\n\n[^a]: body\n", editor_options()));
    let block = d.first_text_leaf().unwrap();
    let copied = d.copy_markdown(Sel {
        anchor: Caret { block, offset: 3 },
        head: Caret { block, offset: 3 },
    });
    println!("copied={copied:?}");
    assert_eq!(copied, "");
}

#[test]
fn quote_promotion_preserves_alert_kind() {
    let source = "> [!NOTE]";
    let loaded = load_markdown(source, editor_options());
    let loaded_quote = loaded
        .preorder()
        .into_iter()
        .find(|&id| loaded.arena.get(id).unwrap().kind == BlockKind::BlockQuote)
        .unwrap();
    assert_eq!(
        loaded.extra(loaded_quote).quote_alert(),
        Some(AlertKind::Note)
    );

    let mut doc = Doc::new(load_markdown("", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert {
            text: source.into(),
        },
    );
    let quote = doc
        .document
        .preorder()
        .into_iter()
        .find(|&id| doc.document.arena.get(id).unwrap().kind == BlockKind::BlockQuote)
        .unwrap();
    assert_eq!(
        doc.document.extra(quote).quote_alert(),
        Some(AlertKind::Note),
        "saved={:?}",
        doc.document.to_markdown()
    );
    assert!(
        doc.document.to_markdown().contains("[!NOTE]"),
        "saved={:?}",
        doc.document.to_markdown()
    );
}

#[test]
fn quote_promotion_preserves_alert_marker_case() {
    let mut doc = Doc::new(load_markdown("", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert {
            text: "> [!warning]".into(),
        },
    );
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    assert!(
        saved.contains("[!warning]"),
        "lowercase marker must survive, saved={saved:?}"
    );
}

#[test]
fn quote_promotion_alert_survives_undo_redo() {
    let mut doc = Doc::new(load_markdown("", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert {
            text: "> [!NOTE]".into(),
        },
    );
    doc.undo();
    let after_undo = doc.document.to_markdown();
    println!("after_undo={after_undo:?}");
    doc.redo();
    let quote = doc
        .document
        .preorder()
        .into_iter()
        .find(|&id| doc.document.arena.get(id).unwrap().kind == BlockKind::BlockQuote)
        .unwrap();
    assert_eq!(
        doc.document.extra(quote).quote_alert(),
        Some(AlertKind::Note),
        "saved={:?}",
        doc.document.to_markdown()
    );
}

#[test]
fn quote_promotion_plain_quote_keeps_text() {
    let mut doc = Doc::new(load_markdown("", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert {
            text: "> hello".into(),
        },
    );
    let quote = doc
        .document
        .preorder()
        .into_iter()
        .find(|&id| doc.document.arena.get(id).unwrap().kind == BlockKind::BlockQuote)
        .unwrap();
    assert_eq!(doc.document.extra(quote).quote_alert(), None);
    assert!(doc.document.to_markdown().contains("hello"));
}

#[test]
fn undo_rich_paste_inside_image_restores_source() {
    let source = "![long alternative text](/target)\n\ntail\n";
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Caret { block, offset: 16 });
    doc.apply(
        Sel::collapsed(caret),
        Command::Paste {
            text: "# inserted\n\n- child\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    println!("after paste={:?}", doc.document.to_markdown());
    assert!(doc.undo().is_some());
    assert_eq!(doc.document.to_markdown(), source);
}

#[test]
fn rich_paste_next_to_image_keeps_the_image_intact() {
    let mut doc = Doc::new(load_markdown(
        "![long alternative text](/target)\n\ntail\n",
        editor_options(),
    ));
    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Caret { block, offset: 16 });
    doc.apply(
        Sel::collapsed(caret),
        Command::Paste {
            text: "# inserted\n\n- child\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    assert!(
        saved.contains("![long alternative text](/target)"),
        "the image must survive structurally, saved={saved:?}"
    );
    assert!(saved.contains("# inserted") && saved.contains("- child"));
}

#[test]
fn rich_paste_at_image_url_segment_keeps_the_image_intact() {
    let source = "![alt](/target)\n\ntail\n";
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Caret { block, offset: 9 });
    doc.apply(
        Sel::collapsed(caret),
        Command::Paste {
            text: "# inserted\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    assert!(
        saved.contains("![alt](/target)"),
        "the image must survive, saved={saved:?}"
    );
    assert!(doc.undo().is_some());
    assert_eq!(doc.document.to_markdown(), source);
}

#[test]
fn rich_paste_replacing_image_selection_restores_source() {
    let source = "![alt text](/target)\n\ntail\n";
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let block = doc.text_leaves()[0];
    let a = doc.retarget_focus(Caret { block, offset: 3 });
    let b = doc.retarget_focus(Caret { block, offset: 6 });
    doc.apply(
        Sel { anchor: a, head: b },
        Command::Paste {
            text: "# inserted\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    println!("saved={:?}", doc.document.to_markdown());
    assert!(doc.undo().is_some());
    assert_eq!(doc.document.to_markdown(), source);
}

#[test]
fn plain_text_paste_inside_image_still_restores() {
    let source = "![alt text](/target)\n\ntail\n";
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Caret { block, offset: 5 });
    doc.apply(
        Sel::collapsed(caret),
        Command::Paste {
            text: "X".into(),
            intent: PasteIntent::PlainText,
        },
    );
    assert!(doc.undo().is_some());
    assert_eq!(doc.document.to_markdown(), source);
}

#[test]
fn multiline_insert_alert_keeps_one_quote() {
    let mut doc = Doc::new(load_markdown("", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert {
            text: "> [!TIP]\n> body".into(),
        },
    );
    let quote = doc
        .document
        .preorder()
        .into_iter()
        .find(|&id| doc.document.arena.get(id).unwrap().kind == BlockKind::BlockQuote)
        .unwrap();
    assert_eq!(
        doc.document.extra(quote).quote_alert(),
        Some(AlertKind::Tip),
        "saved={:?}",
        doc.document.to_markdown()
    );
    assert!(
        doc.document.to_markdown().contains("[!TIP]"),
        "saved={:?}",
        doc.document.to_markdown()
    );
    assert!(
        doc.document.to_markdown().contains("body"),
        "saved={:?}",
        doc.document.to_markdown()
    );
}

#[test]
fn multiline_insert_plain_quote_keeps_one_quote() {
    let mut doc = Doc::new(load_markdown("", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert {
            text: "> hello\n> world".into(),
        },
    );
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    assert_eq!(saved, "> hello\n> world\n", "saved={saved:?}");
}

#[test]
fn multiline_insert_plain_paragraphs_stay_literal() {
    let mut doc = Doc::new(load_markdown("", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert {
            text: "a\n**b**".into(),
        },
    );
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    assert_eq!(saved, "a\n**b**\n");
    let leaf = doc.first_text_leaf().unwrap();
    assert_eq!(doc.text(leaf), Some("a\n**b**"));
}

#[test]
fn multiline_insert_alert_undo_restores_empty() {
    let mut doc = Doc::new(load_markdown("", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert {
            text: "> [!TIP]\n> body".into(),
        },
    );
    assert!(doc.undo().is_some());
    let saved = doc.document.to_markdown();
    println!("after undo={saved:?}");
    let blocks = doc
        .document
        .preorder()
        .into_iter()
        .filter(|&id| {
            doc.document.arena.get(id).unwrap().kind != BlockKind::DocRoot
                && doc.document.arena.get(id).unwrap().kind != BlockKind::DocStart
        })
        .count();
    assert_eq!(
        blocks, 1,
        "only the original empty paragraph, saved={saved:?}"
    );
    assert!(!saved.contains("TIP") && !saved.contains("body"));
}

#[test]
fn multiline_insert_list_keeps_continuation() {
    let mut doc = Doc::new(load_markdown("", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert {
            text: "- item\n  cont".into(),
        },
    );
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    assert!(
        doc.document
            .preorder()
            .into_iter()
            .any(|id| doc.document.arena.get(id).unwrap().kind == BlockKind::List),
        "one batched list item must land as a real list, saved={saved:?}"
    );
    assert_eq!(saved, "- item\n  cont\n");
}

#[test]
fn multiline_insert_fence_lands_as_code_block() {
    let mut doc = Doc::new(load_markdown("", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert {
            text: "```\ncode\n```".into(),
        },
    );
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    assert!(
        doc.document
            .preorder()
            .into_iter()
            .any(|id| doc.document.arena.get(id).unwrap().kind == BlockKind::CodeBlock),
        "one batched fence must land as a code block, saved={saved:?}"
    );
    assert_eq!(saved, "```\ncode\n```\n");
}

#[test]
fn multiline_insert_into_heading_roundtrips() {
    let mut doc = Doc::new(load_markdown("# title\n", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 2 }),
        Command::Insert {
            text: "a\nb".into(),
        },
    );
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    let reloaded = load_markdown(&saved, editor_options());
    assert_eq!(reloaded.to_markdown(), saved, "resave must be byte-stable");
    let kinds: Vec<_> = reloaded
        .preorder()
        .into_iter()
        .filter_map(|id| reloaded.arena.get(id).map(|n| n.kind))
        .collect();
    assert!(
        !kinds.contains(&BlockKind::Paragraph),
        "heading must not split on reload, kinds={kinds:?}, saved={saved:?}"
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|k| matches!(k, BlockKind::Heading(_)))
            .count(),
        1
    );
}

#[test]
fn paste_into_heading_flattens_soft_breaks() {
    let mut doc = Doc::new(load_markdown("# title\n", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 2 }),
        Command::Paste {
            text: "a\nb".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    assert_eq!(doc.document.to_markdown(), "# tia btle\n");
}

#[test]
fn multiline_insert_with_blanks_into_heading_roundtrips() {
    let mut doc = Doc::new(load_markdown("# title\n", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 2 }),
        Command::Insert {
            text: "a\nb\n\nc d".into(),
        },
    );
    let saved = doc.document.to_markdown();
    assert_eq!(saved, "# tia b\n\nc dtle\n");
    let reloaded = load_markdown(&saved, editor_options());
    assert_eq!(reloaded.to_markdown(), saved);
}

#[test]
fn multiline_insert_splice_in_list_item_roundtrips() {
    let mut doc = Doc::new(load_markdown("- item\n", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 2 }),
        Command::Insert {
            text: "> [!TIP]\n> body".into(),
        },
    );
    let saved = doc.document.to_markdown();
    assert_eq!(saved, "- it\n  \n  > [!TIP]\n  > body\n  \n  em\n");
    let reloaded = load_markdown(&saved, editor_options());
    assert_eq!(reloaded.to_markdown(), saved, "resave must be byte-stable");
    assert_loose_in_sync(&doc.document, &reloaded);
}

#[test]
fn paste_fragment_splice_in_list_item_roundtrips() {
    let mut doc = Doc::new(load_markdown("- item\n", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 2 }),
        Command::Paste {
            text: "> [!TIP]\n> body".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    let saved = doc.document.to_markdown();
    assert_eq!(saved, "- it\n  \n  > [!TIP]\n  > body\n  \n  em\n");
    let reloaded = load_markdown(&saved, editor_options());
    assert_eq!(reloaded.to_markdown(), saved, "resave must be byte-stable");
    assert_loose_in_sync(&doc.document, &reloaded);
}

#[test]
fn multiline_insert_nested_list_in_item_roundtrips() {
    let mut doc = Doc::new(load_markdown("- item\n", editor_options()));
    let block = doc.text_leaves()[0];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 2 }),
        Command::Insert {
            text: "- n1\n- n2".into(),
        },
    );
    let saved = doc.document.to_markdown();
    assert_eq!(saved, "- it\n  \n  - n1\n  - n2\n  \n  em\n");
    let reloaded = load_markdown(&saved, editor_options());
    assert_eq!(reloaded.to_markdown(), saved, "resave must be byte-stable");
    assert_loose_in_sync(&doc.document, &reloaded);
}

fn assert_loose_in_sync(mem: &md_core::document::Document, reloaded: &md_core::document::Document) {
    let loose_of = |doc: &md_core::document::Document| {
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).is_some_and(|n| n.kind == BlockKind::List))
            .map(|id| doc.extra(id).list_loose())
            .collect::<Vec<_>>()
    };
    assert_eq!(loose_of(mem), loose_of(reloaded));
}

#[test]
fn collapsed_replace_keeps_code_span_closer() {
    let mut doc = Doc::new(load_markdown("`code` tail\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    doc.apply(
        Sel {
            anchor: Caret { block, offset: 0 },
            head: Caret { block, offset: 4 },
        },
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.document.to_markdown(), "`x` tail\n");
}

#[test]
fn collapsed_replace_keeps_strong_closer() {
    let mut doc = Doc::new(load_markdown("**bold** tail\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    doc.apply(
        Sel {
            anchor: Caret { block, offset: 0 },
            head: Caret { block, offset: 4 },
        },
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.document.to_markdown(), "**x** tail\n");
}

#[test]
fn collapsed_replace_keeps_link_destination() {
    let mut doc = Doc::new(load_markdown("[link](url) tail\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    doc.apply(
        Sel {
            anchor: Caret { block, offset: 0 },
            head: Caret { block, offset: 4 },
        },
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.document.to_markdown(), "[x](url) tail\n");
}

#[test]
fn collapsed_replace_keeps_html_closing_tag() {
    let mut doc = Doc::new(load_markdown("a<b>hello</b> tail\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    doc.apply(
        Sel {
            anchor: Caret { block, offset: 1 },
            head: Caret { block, offset: 6 },
        },
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.document.to_markdown(), "a<b>x</b> tail\n");
}

#[test]
fn collapsed_replace_keeps_adjacent_construct() {
    let mut doc = Doc::new(load_markdown("**bold**`code` tail\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    doc.apply(
        Sel {
            anchor: Caret { block, offset: 0 },
            head: Caret { block, offset: 4 },
        },
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.document.to_markdown(), "**x**`code` tail\n");
}

#[test]
fn collapsed_replace_utf8_keeps_closer() {
    let mut doc = Doc::new(load_markdown("**中文** tail\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    doc.apply(
        Sel {
            anchor: Caret { block, offset: 0 },
            head: Caret { block, offset: 3 },
        },
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.document.to_markdown(), "**x文** tail\n");
}

#[test]
fn collapsed_replace_at_block_end_consumes_hidden_syntax() {
    let mut doc = Doc::new(load_markdown("a<b>hello</b>\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    doc.apply(
        Sel {
            anchor: Caret { block, offset: 0 },
            head: Caret { block, offset: 6 },
        },
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.document.to_markdown(), "x\n");
}

#[test]
fn collapsed_caret_insert_stays_outside_span() {
    let mut doc = Doc::new(load_markdown("`code` tail\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    doc.apply(
        Sel::collapsed(Caret { block, offset: 4 }),
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.document.to_markdown(), "`code`x tail\n");
}

#[test]
fn collapsed_replace_undo_restores_source() {
    let mut doc = Doc::new(load_markdown("`code` tail\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    doc.apply(
        Sel {
            anchor: Caret { block, offset: 0 },
            head: Caret { block, offset: 4 },
        },
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.document.to_markdown(), "`x` tail\n");
    while doc.undo().is_some() {}
    assert_eq!(doc.document.to_markdown(), "`code` tail\n");
}

#[test]
fn collapsed_replace_across_line_break_token_consumes_whole_br() {
    let mut doc = Doc::new(load_markdown(
        "| a | b |\n| --- | --- |\n| c<br>d | e |\n",
        editor_options(),
    ));
    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.text(id) == Some("c\nd"))
        .expect("cell with a soft break");
    doc.apply(
        Sel {
            anchor: Caret {
                block: cell,
                offset: 1,
            },
            head: Caret {
                block: cell,
                offset: 2,
            },
        },
        Command::Insert { text: "x".into() },
    );
    assert_eq!(
        doc.document.to_markdown(),
        "| a | b |\n| --- | --- |\n| cxd | e |\n"
    );
}

#[test]
fn collapsed_replace_entity_consumes_whole_token() {
    let mut doc = Doc::new(load_markdown("a &amp; b\n", editor_options()));
    let block = doc.first_text_leaf().unwrap();
    assert_eq!(doc.text(block), Some("a & b"));
    doc.apply(
        Sel {
            anchor: Caret { block, offset: 2 },
            head: Caret { block, offset: 3 },
        },
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.document.to_markdown(), "a x b\n");
}

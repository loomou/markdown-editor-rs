use super::support::{flow_metrics, heading_spacing_theme, layout, spacing_theme};
use crate::box_tree::TypeSlot;
use crate::compose::{FlowMetrics, LayoutTheme, compose, dump_compose};
use md_core::block::BlockKind;
use md_core::document::{Document, editor_options, load_markdown};

fn quote_spacing_theme() -> LayoutTheme {
    spacing_theme(|kind| match kind {
        BlockKind::Paragraph => 20.0,
        BlockKind::BlockQuote => 32.0,
        BlockKind::List => 20.0,
        _ => 0.0,
    })
}

fn frame_kids(
    tree: &crate::box_tree::BoxTree,
    kind: md_core::block::BlockKind,
) -> &[crate::box_tree::LayoutBoxId] {
    let host = tree
        .nodes
        .values()
        .find(|n| n.kind == kind && n.id.role == crate::box_tree::BoxRole::Frame)
        .unwrap_or_else(|| panic!("{kind:?} frame"));
    let crate::box_tree::BoxChildren::Vertical(kids) = &host.children else {
        panic!("{kind:?} should stack children");
    };
    kids
}

fn para_tops(
    tree: &crate::box_tree::BoxTree,
    kids: &[crate::box_tree::LayoutBoxId],
) -> Vec<md_core::Px> {
    kids.iter()
        .filter_map(|id| tree.nodes.get(id))
        .filter(|n| n.kind == md_core::block::BlockKind::Paragraph)
        .map(|n| tree.style_of(n).margin.top)
        .collect()
}

#[test]
fn compose_quote_paragraphs_drop_top_margin() {
    let theme = quote_spacing_theme();
    let tree = compose(
        &load_markdown("> first\n>\n> second\n", editor_options()),
        &theme,
    );
    assert_eq!(
        para_tops(
            &tree,
            frame_kids(&tree, md_core::block::BlockKind::BlockQuote)
        ),
        vec![0.0, 0.0]
    );
    let kids = frame_kids(&tree, md_core::block::BlockKind::BlockQuote);
    let slots: Vec<_> = kids
        .iter()
        .filter_map(|id| tree.nodes.get(id))
        .filter(|n| n.kind == md_core::block::BlockKind::Paragraph)
        .map(|n| n.type_slot)
        .collect();
    assert_eq!(slots, vec![TypeSlot::Quote, TypeSlot::Quote]);
}

#[test]
fn compose_quote_paragraph_top_is_theme_driven() {
    let theme = quote_spacing_theme().with_flow_metrics(FlowMetrics {
        quote_paragraph_top: 7.0,
        quote_paragraph_slot: TypeSlot::FromKind,
        ..flow_metrics(20.0)
    });
    let tree = compose(
        &load_markdown("> first\n>\n> second\n", editor_options()),
        &theme,
    );
    assert_eq!(
        para_tops(
            &tree,
            frame_kids(&tree, md_core::block::BlockKind::BlockQuote)
        ),
        vec![7.0, 7.0]
    );
    let kids = frame_kids(&tree, md_core::block::BlockKind::BlockQuote);
    let slots: Vec<_> = kids
        .iter()
        .filter_map(|id| tree.nodes.get(id))
        .filter(|n| n.kind == md_core::block::BlockKind::Paragraph)
        .map(|n| n.type_slot)
        .collect();
    assert_eq!(slots, vec![TypeSlot::FromKind, TypeSlot::FromKind]);
}

#[test]
fn compose_paragraph_after_h1_uses_lead_top() {
    let theme = quote_spacing_theme().with_flow_metrics(flow_metrics(24.0));
    let tree = compose(
        &load_markdown("# Title\n\nlead para\n\nnext para\n", editor_options()),
        &theme,
    );
    let mut tops: Vec<_> = tree
        .nodes
        .values()
        .filter(|n| {
            n.kind == md_core::block::BlockKind::Paragraph
                && n.id.role == crate::box_tree::BoxRole::Frame
        })
        .map(|n| tree.style_of(n).margin.top)
        .collect();
    tops.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(tops, vec![20.0, 24.0]);
}

#[test]
fn compose_nested_quote_keeps_quote_top_margin() {
    let theme = quote_spacing_theme();
    let tree = compose(
        &load_markdown("> outer\n>\n> > inner\n", editor_options()),
        &theme,
    );
    let mut quotes: Vec<_> = tree
        .nodes
        .values()
        .filter(|n| {
            n.kind == md_core::block::BlockKind::BlockQuote
                && n.id.role == crate::box_tree::BoxRole::Frame
        })
        .collect();
    quotes.sort_by_key(|n| tree.style_of(n).margin.top as i64);
    assert_eq!(quotes.len(), 2);
    assert_eq!(tree.style_of(quotes[0]).margin.top, 32.0);
    assert_eq!(tree.style_of(quotes[1]).margin.top, 32.0);
    for q in quotes {
        let crate::box_tree::BoxChildren::Vertical(kids) = &q.children else {
            panic!("quote should stack children");
        };
        assert!(
            para_tops(&tree, kids).iter().all(|t| *t == 0.0),
            "blockquote p {{ margin-top: 0 }}"
        );
    }
}

#[test]
fn compose_alert_quote_reserves_a_label_band() {
    let theme = quote_spacing_theme().with_flow_metrics(FlowMetrics {
        quote_alert_lead: 9.0,
        ..flow_metrics(20.0)
    });
    let alert = compose(
        &load_markdown("> [!NOTE]\n> text\n", editor_options()),
        &theme,
    );
    let quote = alert
        .nodes
        .values()
        .find(|n| n.kind == BlockKind::BlockQuote && n.id.role == crate::box_tree::BoxRole::Frame)
        .expect("alert fixture has one quote");
    assert_eq!(alert.style_of(quote).padding.top, 9.0);

    let plain = compose(&load_markdown("> text\n", editor_options()), &theme);
    let quote = plain
        .nodes
        .values()
        .find(|n| n.kind == BlockKind::BlockQuote && n.id.role == crate::box_tree::BoxRole::Frame)
        .expect("plain fixture has one quote");
    assert_eq!(plain.style_of(quote).padding.top, 0.0);
}

#[test]
fn compose_quote_list_keeps_list_top_margin() {
    let theme = quote_spacing_theme();
    let tree = compose(&load_markdown("> - item\n", editor_options()), &theme);
    let list = tree
        .nodes
        .values()
        .find(|n| {
            n.kind == md_core::block::BlockKind::List
                && n.id.role == crate::box_tree::BoxRole::Frame
        })
        .expect("list");
    assert_eq!(tree.style_of(list).margin.top, 20.0);
}

#[test]
fn compose_quote_and_list_emit_chrome_slots() {
    let doc = load_markdown("> quoted\n\n- item\n", editor_options());
    let tree = compose(&doc, &layout());
    let bars = tree
        .nodes
        .values()
        .filter(|n| n.id.role == crate::box_tree::BoxRole::Bar)
        .count();
    let slots = tree
        .nodes
        .values()
        .filter(|n| n.id.role == crate::box_tree::BoxRole::Slot)
        .count();
    assert_eq!(bars, 1);
    assert_eq!(slots, 1);
    let dump = dump_compose(&tree);
    assert!(dump.contains("Bar"));
    assert!(dump.contains("Slot"));
    for n in tree.nodes.values() {
        if n.id.role == crate::box_tree::BoxRole::Bar {
            assert_eq!(n.kind, md_core::block::BlockKind::BlockQuote);
        }
        if n.id.role == crate::box_tree::BoxRole::Slot {
            assert_eq!(n.kind, md_core::block::BlockKind::ListItem);
        }
    }
}

fn first_list_extra(doc: &Document) -> md_core::block::NodeExtra {
    doc.preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(md_core::block::BlockKind::List))
        .map(|id| doc.extra(id))
        .expect("list")
}

#[test]
fn list_tight_has_no_paragraph_events() {
    let tight = load_markdown("- a\n- b\n", editor_options());
    assert!(
        !first_list_extra(&tight).list_loose(),
        "tight list should not be loose"
    );
    let loose = load_markdown("- a\n\n- b\n", editor_options());
    assert!(first_list_extra(&loose).list_loose());
    let ordered = load_markdown("1. a\n", editor_options());
    assert_eq!(first_list_extra(&ordered).ordered_start(), Some(1));
    assert!(!first_list_extra(&ordered).list_loose());
    let task = load_markdown("1. [ ] t\n", editor_options());
    assert_eq!(first_list_extra(&task).ordered_start(), Some(1));
    assert!(
        task.preorder()
            .into_iter()
            .any(|id| task.extra(id).task_checked() == Some(false))
    );
}

#[test]
fn compose_ordered_gutter_grows_with_digits() {
    let layout = layout();
    let one = compose(&load_markdown("1. a\n", editor_options()), &layout);
    let wide = compose(&load_markdown("99. a\n100. b\n", editor_options()), &layout);
    let pad = |tree: &crate::box_tree::BoxTree| {
        tree.nodes
            .values()
            .find(|n| {
                n.kind == md_core::block::BlockKind::List
                    && n.id.role == crate::box_tree::BoxRole::Frame
            })
            .map(|n| tree.style_of(n).padding.left)
            .expect("list pad")
    };
    assert!(pad(&wide) > pad(&one));
    let tight = compose(&load_markdown("- a\n- b\n", editor_options()), &layout);
    let loose = compose(&load_markdown("- a\n\n- b\n", editor_options()), &layout);
    let gap = |tree: &crate::box_tree::BoxTree| {
        tree.nodes
            .values()
            .find(|n| {
                n.kind == md_core::block::BlockKind::List
                    && n.id.role == crate::box_tree::BoxRole::Frame
            })
            .map(|n| tree.style_of(n).gap)
            .expect("gap")
    };
    assert_eq!(gap(&tight), layout.list_tight_gap);
    assert_eq!(gap(&loose), layout.list_loose_gap);
}

#[test]
fn compose_nested_list_uses_nested_top() {
    let layout = layout();
    let tree = compose(&load_markdown("- a\n  - b\n", editor_options()), &layout);
    let mut lists: Vec<_> = tree
        .nodes
        .values()
        .filter(|n| {
            n.kind == md_core::block::BlockKind::List
                && n.id.role == crate::box_tree::BoxRole::Frame
        })
        .collect();
    lists.sort_by_key(|n| tree.style_of(n).margin.top as i64);
    assert!(lists.len() >= 2);
    assert_eq!(tree.style_of(lists[0]).margin.top, 0.0);
    assert_eq!(tree.style_of(lists[1]).margin.top, layout.list_nested_top);
}

#[test]
fn equal_final_styles_share_one_interned_entry() {
    let tree = compose(
        &load_markdown("first\n\nsecond\n\nthird\n", editor_options()),
        &layout(),
    );
    let paragraphs: Vec<_> = tree
        .nodes
        .values()
        .filter(|n| n.kind == md_core::block::BlockKind::Paragraph)
        .collect();
    assert_eq!(paragraphs.len(), 3);
    assert!(
        paragraphs
            .windows(2)
            .all(|pair| pair[0].style_id == pair[1].style_id)
    );
    assert_eq!(tree.style_count(), 1, "uniform test theme needs one style");
}

#[test]
fn derived_list_styles_do_not_mutate_other_lists() {
    let layout = layout();

    let tree = compose(
        &load_markdown("- plain\n\nbetween\n\n- [ ] task\n", editor_options()),
        &layout,
    );
    let mut lists: Vec<_> = tree
        .nodes
        .values()
        .filter(|n| n.kind == md_core::block::BlockKind::List)
        .collect();
    assert_eq!(lists.len(), 2);
    lists.sort_by_key(|n| tree.style_of(n).padding.left as i64);
    let plain = tree.style_of(lists[0]);
    let task = tree.style_of(lists[1]);
    assert_eq!(plain.padding.left, layout.list_gutter_min);
    assert_eq!(
        task.padding.left,
        layout.list_gutter_min + layout.list_task_extra
    );
    assert_ne!(lists[0].style_id, lists[1].style_id);
}

#[test]
fn loose_colon_lines_compose_without_chrome() {
    let tree = compose(
        &load_markdown("term\n\n: desc\n", editor_options()),
        &layout(),
    );
    assert!(
        tree.nodes
            .values()
            .all(|n| n.id.role != crate::box_tree::BoxRole::Slot)
    );
}

#[test]
fn compose_checked_task_uses_task_done_slot() {
    let tree = compose(
        &load_markdown("- [x] done\n- [ ] open\n", editor_options()),
        &layout(),
    );
    let mut paras: Vec<_> = tree
        .nodes
        .values()
        .filter(|n| {
            n.kind == md_core::block::BlockKind::Paragraph
                && n.id.role == crate::box_tree::BoxRole::Frame
        })
        .collect();
    paras.sort_by_key(|n| match n.id.owner {
        crate::box_tree::BoxOwner::Block(b) => b,
        crate::box_tree::BoxOwner::DocStart => 0,
    });
    assert_eq!(paras.len(), 2);
    assert_eq!(paras[0].type_slot, TypeSlot::TaskDone);
    assert_eq!(paras[1].type_slot, TypeSlot::FromKind);
}

fn first_doc_frame(tree: &crate::box_tree::BoxTree) -> &crate::box_tree::BoxNode {
    let crate::box_tree::BoxChildren::Vertical(kids) = &tree.get(tree.root).children else {
        panic!("doc root should stack children");
    };
    kids.iter()
        .filter_map(|id| tree.nodes.get(id))
        .find(|n| {
            n.id.role == crate::box_tree::BoxRole::Frame
                && n.kind != md_core::block::BlockKind::DocStart
        })
        .expect("first frame")
}

#[test]
fn compose_doc_lead_paragraph_and_headings_share_zero_top() {
    let theme = heading_spacing_theme();
    for md in [
        "hello\n",
        "# hello\n",
        "## hello\n",
        "### hello\n",
        "#### hello\n",
        "##### hello\n",
        "###### hello\n",
    ] {
        let tree = compose(&load_markdown(md, editor_options()), &theme);
        assert_eq!(
            tree.style_of(first_doc_frame(&tree)).margin.top,
            0.0,
            "lead of {md:?}"
        );
    }
    let tree = compose(
        &load_markdown("para\n\n## later\n", editor_options()),
        &theme,
    );
    let h2 = tree
        .nodes
        .values()
        .find(|n| {
            n.kind == md_core::block::BlockKind::Heading(2)
                && n.id.role == crate::box_tree::BoxRole::Frame
        })
        .expect("h2");
    assert_eq!(tree.style_of(h2).margin.top, 20.0);
}

fn table_spacing_theme() -> LayoutTheme {
    spacing_theme(|kind| match kind {
        BlockKind::Paragraph => 20.0,
        BlockKind::Table => 32.0,
        _ => 0.0,
    })
    .with_flow_metrics(flow_metrics(20.0))
}

#[test]
fn compose_doc_lead_table_shares_paragraph_zero_top() {
    let theme = table_spacing_theme();
    let tree = compose(
        &load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options()),
        &theme,
    );
    let table = first_doc_frame(&tree);
    assert_eq!(table.kind, md_core::block::BlockKind::Table);
    assert_eq!(tree.style_of(table).margin.top, 0.0);
}

#[test]
fn compose_table_after_paragraph_shares_paragraph_top() {
    let theme = table_spacing_theme();
    let tree = compose(
        &load_markdown(
            "hello\n\n| a | b |\n| --- | --- |\n| c | d |\n",
            editor_options(),
        ),
        &theme,
    );
    assert_eq!(tree.style_of(first_doc_frame(&tree)).margin.top, 0.0);
    assert_eq!(frame_top(&tree, md_core::block::BlockKind::Table), 20.0);
}

#[test]
fn compose_table_after_table_shares_paragraph_top() {
    let theme = table_spacing_theme();
    let tree = compose(
        &load_markdown(
            "| a | b |\n| --- | --- |\n| c | d |\n\n| e | f |\n| --- | --- |\n| g | h |\n",
            editor_options(),
        ),
        &theme,
    );
    let mut tops: Vec<_> = tree
        .nodes
        .values()
        .filter(|n| {
            n.kind == md_core::block::BlockKind::Table
                && n.id.role == crate::box_tree::BoxRole::Frame
        })
        .map(|n| tree.style_of(n).margin.top)
        .collect();
    tops.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    assert_eq!(tops, vec![0.0, 20.0]);
}

fn frame_top(tree: &crate::box_tree::BoxTree, kind: md_core::block::BlockKind) -> md_core::Px {
    tree.nodes
        .values()
        .find(|n| n.kind == kind && n.id.role == crate::box_tree::BoxRole::Frame)
        .map(|n| tree.style_of(n).margin.top)
        .unwrap_or_else(|| panic!("{kind:?} frame"))
}

#[test]
fn compose_flow_headings_share_paragraph_top() {
    let theme = heading_spacing_theme();
    for (md, kind) in [
        ("para\n\n# later\n", md_core::block::BlockKind::Heading(1)),
        ("para\n\n## later\n", md_core::block::BlockKind::Heading(2)),
        ("para\n\n### later\n", md_core::block::BlockKind::Heading(3)),
        (
            "para\n\n#### later\n",
            md_core::block::BlockKind::Heading(4),
        ),
        (
            "para\n\n##### later\n",
            md_core::block::BlockKind::Heading(5),
        ),
        (
            "para\n\n###### later\n",
            md_core::block::BlockKind::Heading(6),
        ),
    ] {
        let tree = compose(&load_markdown(md, editor_options()), &theme);
        assert_eq!(frame_top(&tree, kind), 20.0, "{md:?}");
    }
    let tree = compose(
        &load_markdown("# Title\n\n## later\n", editor_options()),
        &theme,
    );
    assert_eq!(
        frame_top(&tree, md_core::block::BlockKind::Heading(2)),
        24.0
    );
}

#[test]
fn compose_quote_heading_uses_quote_paragraph_top() {
    let theme = heading_spacing_theme();
    let tree = compose(
        &load_markdown("> first\n>\n> ## later\n", editor_options()),
        &theme,
    );
    assert_eq!(frame_top(&tree, md_core::block::BlockKind::Heading(2)), 0.0);
}

#[test]
#[should_panic(expected = "DocStart box carries no block index")]
fn shape_ident_refuses_the_doc_start_box() {
    let tree = compose(&load_markdown("x\n", editor_options()), &layout());
    let mut node = tree
        .nodes
        .values()
        .find(|n| n.kind == BlockKind::Paragraph)
        .expect("fixture should hold a paragraph box")
        .clone();
    node.id = crate::box_tree::LayoutBoxId::doc_start();
    let _ = node.shape_ident();
}

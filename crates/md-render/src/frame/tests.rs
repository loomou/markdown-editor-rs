use super::probe::{first_line_height, first_line_top, first_text_box};
use crate::boxtree::block_id_of;
use md_core::block::BlockKind;
use md_layout::assembly::Assembly;
use md_layout::box_tree::{BoxRole, LayoutBoxId};
use md_layout::flow::HeightState;
use md_layout::spine::FlowSpine;
use md_theme::DocumentTheme;
use std::rc::Rc;

#[test]
fn chrome_slot_hit_block_is_text_node() {
    let doc = md_core::document::load_markdown(
        "> quoted\n\n- item\n",
        md_core::document::editor_options(),
    );
    let layout =
        md_layout::compose::LayoutTheme::from_resolver(|_| md_layout::style::BoxLayoutStyle {
            display: md_layout::style::BoxDisplay::FlowStack,
            margin: md_layout::style::Edges::ZERO,
            padding: md_layout::style::Edges::ZERO,
            border: md_layout::style::Edges {
                left: 4.0,
                ..md_layout::style::Edges::ZERO
            },
            gap: 0.0,
        });
    let tree = md_layout::compose::compose(&doc, &layout);
    let quote = tree
        .nodes()
        .values()
        .find(|n| n.kind() == BlockKind::BlockQuote && n.id().role == BoxRole::Frame)
        .expect("quote");
    let hit = first_text_box(&tree, quote.id()).expect("leaf");
    assert_eq!(
        doc.kind(block_id_of(hit).expect("block owner")),
        Some(BlockKind::Paragraph)
    );
    assert!(tree.nodes().contains_key(&LayoutBoxId::bar(
        block_id_of(quote.id()).expect("quote owner")
    )));
    let item = tree
        .nodes()
        .values()
        .find(|n| n.kind() == BlockKind::ListItem && n.id().role == BoxRole::Frame)
        .expect("item");
    let hit_item = first_text_box(&tree, item.id()).expect("item leaf");
    assert_eq!(
        doc.kind(block_id_of(hit_item).expect("block owner")),
        Some(BlockKind::Paragraph)
    );
    assert!(tree.nodes().contains_key(&LayoutBoxId::slot(
        block_id_of(item.id()).expect("item owner")
    )));
}

#[test]
fn list_item_starting_with_table_keeps_cell_box_role() {
    let doc = md_core::document::load_markdown(
        "- | a | b |\n  | --- | --- |\n  | 1 | 2 |\n",
        md_core::document::editor_options(),
    );
    let layout =
        md_layout::compose::LayoutTheme::from_resolver(|_| md_layout::style::BoxLayoutStyle {
            display: md_layout::style::BoxDisplay::FlowStack,
            margin: md_layout::style::Edges::ZERO,
            padding: md_layout::style::Edges::ZERO,
            border: md_layout::style::Edges::ZERO,
            gap: 0.0,
        });
    let tree = md_layout::compose::compose(&doc, &layout);
    let item_id = tree
        .nodes()
        .values()
        .find(|n| n.kind() == BlockKind::ListItem && n.id().role == BoxRole::Frame)
        .expect("list item")
        .id();
    let leaf = first_text_box(&tree, item_id).expect("table cell leaf");
    let block = block_id_of(leaf).expect("cell owner");

    assert_eq!(doc.kind(block), Some(BlockKind::TableCell));
    assert_eq!(leaf.role, BoxRole::Cell);
    assert!(!tree.nodes().contains_key(&LayoutBoxId::frame(block)));

    let assembly = Assembly {
        tree: Rc::new(tree),
        table_cons: Rc::new(Default::default()),
        heights: Default::default(),
        geometries: Default::default(),
        window: None,
        island_stats: Default::default(),
    };
    assert!(first_line_height(&assembly, item_id, &DocumentTheme::one_dark()) > 0.0);
}

#[test]
fn list_item_starting_with_table_takes_line_top_from_the_row() {
    let doc = md_core::document::load_markdown(
        "- | a | b |\n  | --- | --- |\n  | 1 | 2 |\n\n- text\n",
        md_core::document::editor_options(),
    );
    let layout = md_layout::compose::LayoutTheme::from_resolver(|kind| {
        let padding = if kind == BlockKind::ListItem {
            8.0
        } else {
            0.0
        };
        md_layout::style::BoxLayoutStyle {
            display: md_layout::style::BoxDisplay::FlowStack,
            margin: md_layout::style::Edges::ZERO,
            padding: md_layout::style::Edges {
                top: padding,
                ..md_layout::style::Edges::ZERO
            },
            border: md_layout::style::Edges::ZERO,
            gap: 0.0,
        }
    });
    let tree = md_layout::compose::compose(&doc, &layout);
    let heights = |id: LayoutBoxId, _| {
        HeightState::Estimated(match tree.get(id).kind() {
            BlockKind::TableRow => 40.0,
            _ => 20.0,
        })
    };
    let item_id = tree
        .nodes()
        .values()
        .find(|n| n.kind() == BlockKind::ListItem && n.id().role == BoxRole::Frame)
        .expect("list item")
        .id();
    let spine = FlowSpine::flatten(&tree, 800.0, &heights);
    let mut spine = spine;
    let _ = spine.expand_visible(&tree, -100.0, 10000.0, &heights);
    let window = spine.window(&tree, -100.0, 10000.0, &[]);
    let item_top = window
        .container_spans
        .iter()
        .find(|s| s.box_id == item_id)
        .map(|s| s.top)
        .expect("list item span");
    let assembly = Assembly {
        tree: Rc::new(tree),
        table_cons: Rc::new(Default::default()),
        heights: Default::default(),
        geometries: Default::default(),
        window: Some(window),
        island_stats: Default::default(),
    };

    let line_top = first_line_top(&assembly, item_id, item_top);
    assert!(
        line_top > item_top,
        "the line island has border+padding, so the first text line's top must sit below the item top: {line_top} vs {item_top}"
    );
    assert_eq!(first_line_top(&assembly, item_id, item_top), line_top);
}

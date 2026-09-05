use crate::snapshot::{DecorationPiece, LayoutSnapshot, TextPiece};
use md_content::shaper::ShapeArtifact;
use md_core::Px;
use md_core::block::BlockKind;
use md_core::inline::InlineAlign;
use md_layout::box_tree::{BoxRole, LayoutBoxId};
use std::collections::BTreeMap;

use super::{Aligned, a11y_bounds, hit_list_item_slot};

fn dummy_artifact(height: Px) -> ShapeArtifact {
    ShapeArtifact::plain(Vec::new(), 1, height, 16.0, 20.0)
}

fn snapshot_with(geometry_revision: u64, texts: Vec<TextPiece>) -> LayoutSnapshot {
    LayoutSnapshot {
        document_revision: 1,
        layout_revision: 1,
        viewport_revision: 1,
        geometry_revision,
        total_height: 400.0,
        scroll: 0.0,
        viewport: (800.0, 600.0),
        texts,
        decorations: Vec::new(),
        cells: Vec::new(),
        caret_device: None,
        caret_logical_y: Some(12.0),
        selection_device: Vec::new(),
        inline_code_device: Vec::new(),
        search_device: Vec::new(),
        search_active_device: Vec::new(),
        ime_device: Vec::new(),
        spans: BTreeMap::new(),
        content_atoms_painted: 0,
        absent_visible: Vec::new(),
    }
}

#[test]
fn mismatched_geometry_revision_rejects_consumers() {
    let painted = snapshot_with(
        11,
        vec![TextPiece {
            box_id: LayoutBoxId::frame(1),
            block: 1,
            kind: BlockKind::Paragraph,
            edit_source: false,
            content_origin_device: (0.0, 0.0),
            content_width: 100.0,
            view_height: 20.0,
            art: std::rc::Rc::new(dummy_artifact(20.0)),
            align: InlineAlign::Start,
        }],
    );
    let other = snapshot_with(12, Vec::new());
    assert!(Aligned::new(&painted, painted.geometry_revision).is_some());
    assert!(Aligned::new(&painted, other.geometry_revision).is_none());
    assert_eq!(a11y_bounds(&painted, painted.geometry_revision).len(), 1);
    assert!(a11y_bounds(&painted, other.geometry_revision).is_empty());
    assert!(painted.caret_logical_y.is_some());
    assert_ne!(painted.geometry_revision, other.geometry_revision);
}

fn list_slot(hit_block: md_core::block::BlockId, task: Option<bool>) -> DecorationPiece {
    DecorationPiece {
        rect_device: (0.0, 0.0, 40.0, 20.0),
        clip_device: None,
        kind: BlockKind::ListItem,
        role: BoxRole::Slot,
        hit_block,
        gutter_dot: Some((0.0, 0.0)),
        gutter_label: None,
        gutter_label_at: None,
        gutter_label_size: 16.0,
        list_nest: 0,
        task,
        alert: None,
    }
}

#[test]
fn list_gutter_click_toggles_only_task_items() {
    let mut ul = snapshot_with(1, Vec::new());
    ul.decorations = vec![list_slot(10, None)];
    assert_eq!(hit_list_item_slot(&ul, 1, (10.0, 10.0)), None);

    let mut ol = snapshot_with(1, Vec::new());
    ol.decorations = vec![DecorationPiece {
        gutter_label: Some("1.".into()),
        ..list_slot(11, None)
    }];
    assert_eq!(hit_list_item_slot(&ol, 1, (10.0, 10.0)), None);

    let mut task = snapshot_with(1, Vec::new());
    task.decorations = vec![list_slot(20, Some(false))];
    assert_eq!(hit_list_item_slot(&task, 1, (10.0, 10.0)), Some(20));
}

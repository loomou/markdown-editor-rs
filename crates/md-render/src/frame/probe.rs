use md_content::shaper::GpuiShaper;
use md_core::Px;
use md_core::block::BlockKind;
use md_layout::assembly::Assembly;
use md_layout::box_tree::{BoxChildren, BoxRole, BoxTree, LayoutBoxId};
use md_layout::spine::FlowItemKind;
use md_layout::style::BoxLayoutEnvironment;
use md_theme::DocumentTheme;

pub(super) fn first_text_box(tree: &BoxTree, id: LayoutBoxId) -> Option<LayoutBoxId> {
    let node = tree.nodes().get(&id)?;
    match node.children() {
        BoxChildren::None => {
            if matches!(
                node.kind(),
                BlockKind::Paragraph
                    | BlockKind::Heading(_)
                    | BlockKind::CodeBlock
                    | BlockKind::MetadataBlock
                    | BlockKind::TableCell
                    | BlockKind::Image
                    | BlockKind::Mermaid
                    | BlockKind::Math
            ) {
                Some(id)
            } else {
                None
            }
        }
        BoxChildren::Island(cells) => cells.iter().copied().find_map(|c| first_text_box(tree, c)),
        BoxChildren::Vertical(c) => c.iter().copied().find_map(|c| first_text_box(tree, c)),
    }
}

pub(super) fn first_line_top(assembly: &Assembly, item: LayoutBoxId, fallback: Px) -> Px {
    let Some(leaf) = first_text_box(&assembly.tree, item) else {
        return fallback;
    };
    let Some(window) = assembly.window.as_ref() else {
        return fallback;
    };
    let row = assembly.tree.get(leaf).parent();
    let top = [Some(leaf), row].into_iter().flatten().find_map(|id| {
        window.entries.iter().find_map(|e| match e.kind {
            FlowItemKind::Content { box_id } if box_id == id => Some(e.top),
            _ => None,
        })
    });
    let Some(top) = top else {
        return fallback;
    };
    top + assembly.tree.style(leaf).top_border_padding()
}

pub(super) fn first_line_height(
    assembly: &Assembly,
    item: LayoutBoxId,
    theme: &DocumentTheme,
) -> Px {
    let role = first_text_box(&assembly.tree, item)
        .map(|leaf| {
            let node = assembly.tree.get(leaf);
            theme.type_role_for(node.shape_kind(), node.type_slot())
        })
        .unwrap_or(theme.type_scale.body);
    role.size_px as Px * role.line_height_em as Px
}

pub(super) fn first_line_ink(
    assembly: &Assembly,
    item: LayoutBoxId,
    shaper: &GpuiShaper,
    env: BoxLayoutEnvironment,
) -> Option<(Px, Px)> {
    let leaf = first_text_box(&assembly.tree, item)?;
    let node = assembly.tree.get(leaf);
    let inner = text_inner_width(assembly, leaf, env.viewport_width)?;
    let art = shaper.artifact(
        assembly.tree.text_of(node),
        assembly.tree.runs_of(node),
        inner,
        node.kind(),
        node.shape_ident(),
    );
    let (dy, h) = shaper.caret_ink(node.shape_kind(), node.type_slot(), &art, 0);
    Some((dy, h))
}

fn text_inner_width(assembly: &Assembly, leaf: LayoutBoxId, viewport_width: Px) -> Option<Px> {
    let node = assembly.tree.get(leaf);
    let style = assembly.tree.style_of(node);
    let width = if leaf.role == BoxRole::Cell {
        let row = node.parent()?;
        let g = assembly.geometries.get(&row)?;
        let cg = g.cells.iter().find(|c| c.cell_box == leaf)?;
        cg.width
    } else {
        assembly.tree.avail_width(leaf, viewport_width)
    };
    Some((width - style.inline_border_padding()).max(0.0))
}

pub(super) fn list_nest_depth(tree: &BoxTree, item: LayoutBoxId) -> u8 {
    let mut n = 0u8;
    let mut cur = Some(item);
    while let Some(id) = cur {
        let Some(node) = tree.nodes().get(&id) else {
            break;
        };
        if node.kind() == BlockKind::List {
            n = n.saturating_add(1);
        }
        cur = node.parent();
    }
    n.saturating_sub(1)
}

pub(super) fn ordered_label(assembly: &Assembly, item: LayoutBoxId) -> Option<String> {
    let parent = assembly.tree.get(item).parent()?;
    let p = assembly.tree.get(parent);
    let start = p.extra().ordered_start()?;
    let BoxChildren::Vertical(kids) = p.children() else {
        return Some(format!("{start}."));
    };
    let idx = kids.iter().position(|k| *k == item)? as u64;
    Some(format!("{}.", start + idx))
}

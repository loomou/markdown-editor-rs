use md_core::Px;
use md_core::block::BlockKind;
use md_layout::box_tree::{BoxTree, LayoutBoxId};

#[derive(Clone, Copy, Debug)]
pub struct Estimator {
    pub line_height: Px,

    pub em_width: Px,

    pub heading1_mult: Px,

    pub heading_mult: Px,

    pub table_row_mult: Px,
    pub mermaid_max_height: Px,
    pub image_placeholder_height: Px,
    pub code_max_height: Px,
    pub math_max_height: Px,
    pub image_max_height: Px,
}

impl Estimator {
    pub fn from_theme(theme: &md_theme::DocumentTheme) -> Self {
        let body_role = theme.type_role(BlockKind::Paragraph);
        let body = if body_role.size_px > 0.0 {
            body_role.size_px as Px
        } else {
            16.0
        };
        Estimator {
            line_height: body * body_role.line_height_em as Px,
            em_width: body,
            heading1_mult: theme.type_role(BlockKind::Heading(1)).size_px as Px / body,
            heading_mult: theme.type_role(BlockKind::Heading(2)).size_px as Px / body,
            table_row_mult: 1.2,
            mermaid_max_height: theme.decoration.mermaid_max_height,
            image_placeholder_height: theme.decoration.image_placeholder_height,
            code_max_height: theme.decoration.code_max_height,
            math_max_height: theme.decoration.math_max_height,
            image_max_height: theme.decoration.image_max_height,
        }
    }

    pub fn leaf_metrics(self) -> md_layout::compose::LeafMetrics {
        md_layout::compose::LeafMetrics {
            line_height: self.line_height,
            em_width: self.em_width,
            heading1_mult: self.heading1_mult,
            heading_mult: self.heading_mult,
            table_row_mult: self.table_row_mult,
            mermaid_max_height: self.mermaid_max_height,
            image_placeholder_height: self.image_placeholder_height,
            code_max_height: self.code_max_height,
            math_max_height: self.math_max_height,
            image_max_height: self.image_max_height,
        }
    }

    pub fn estimate(&self, tree: &BoxTree, id: LayoutBoxId, avail_width: Px) -> Px {
        let node = tree.get(id);
        let style = tree.style_of(node);
        if node.kind() == BlockKind::Mermaid && !node.edit_source() {
            return self.mermaid_max_height
                + style.top_border_padding()
                + style.bottom_border_padding();
        }
        if node.kind() == BlockKind::Image && !node.edit_source() {
            let inner = (avail_width - style.inline_border_padding()).max(1.0);
            let text_width = self.estimated_text_width(tree.text_of(node));
            let cap_rows = if text_width == 0.0 {
                0.0
            } else {
                (text_width / inner).ceil().max(1.0)
            };
            return self.image_placeholder_height.min(self.image_max_height)
                + cap_rows * self.line_height
                + style.top_border_padding()
                + style.bottom_border_padding();
        }
        if node.kind() == BlockKind::Math && !node.edit_source() {
            return (self.line_height * 2.0).min(self.math_max_height)
                + style.top_border_padding()
                + style.bottom_border_padding();
        }
        let inner = (avail_width - style.inline_border_padding()).max(1.0);
        let text_width = self.estimated_text_width(tree.text_of(node));
        let est_rows = if node.edit_source() {
            (tree.text_of(node).lines().count() as Px).max(1.0)
        } else if text_width == 0.0 {
            1.0
        } else {
            (text_width / inner).ceil().max(1.0)
        };
        let kind_mult = match node.kind() {
            BlockKind::Heading(1) => self.heading1_mult,
            BlockKind::Heading(_) => self.heading_mult,
            BlockKind::TableRow => self.table_row_mult,
            _ => 1.0,
        };
        let mut content = est_rows * self.line_height * kind_mult;
        if node.kind() == BlockKind::CodeBlock || node.edit_source() {
            content = content.min(self.code_max_height);
        }
        content + style.top_border_padding() + style.bottom_border_padding()
    }

    fn estimated_text_width(&self, text: &str) -> Px {
        let ems: Px = text
            .chars()
            .map(|ch| if ch.is_ascii() { 0.5 } else { 1.0 })
            .sum();
        ems * self.em_width
    }
}

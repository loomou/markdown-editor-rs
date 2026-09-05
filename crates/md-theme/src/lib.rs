mod app;
mod appearance;
mod box_scale;
mod color;
mod decoration;
mod flow_scale;
mod font;
mod inline;
mod paint;
mod syntax;
mod type_scale;

pub use app::AppTokens;
pub use appearance::{Appearance, BodyFamily, Density, ThemeVariant};
pub use box_scale::BoxScale;
pub use color::{ColorGroup, ColorOverrides, ColorSlot};
pub use decoration::DecorationTokens;
pub use flow_scale::FlowScale;
pub use font::{FontStyle, FontWeight, SYSTEM_MONO, SYSTEM_SERIF, SYSTEM_UI};
pub use inline::InlineTokens;
pub use md_layout::box_tree::TypeSlot;
pub use paint::{ChromeTokens, PaintTokens, ThemeColor};
pub use syntax::{SyntaxRole, SyntaxTokens};
pub use type_scale::{TypeRole, TypeScale};

use md_core::block::BlockKind;
use md_layout::compose::FlowMetrics;
use md_layout::compose::LayoutTheme;
use md_layout::compose::ListMetrics;
use md_layout::style::{BoxLayoutStyle, Edges};

use paint::Palette;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DocumentTheme {
    pub edge_scale: f64,
    pub paint: PaintTokens,

    pub app: AppTokens,
    pub chrome: ChromeTokens,
    pub type_scale: TypeScale,
    pub boxes: BoxScale,
    pub flow: FlowScale,
    pub inline: InlineTokens,
    pub decoration: DecorationTokens,
    pub syntax: SyntaxTokens,
}

impl DocumentTheme {
    pub fn formal() -> Self {
        let palette = Palette::formal();
        let syntax = SyntaxTokens::formal();
        let mut paint = PaintTokens::from_palette(&palette);
        paint.code_fill = syntax::mocha_base();
        let mut type_scale = TypeScale::from_palette(&palette);
        type_scale.code.color = syntax.default;
        let mut inline = InlineTokens::from_palette(&palette);
        inline.image_fill = paint.code_fill;
        Self {
            edge_scale: 1.0,
            paint,
            app: AppTokens::from_palette(&palette),
            chrome: ChromeTokens::from_palette(&palette),
            type_scale,
            boxes: BoxScale::formal(),
            flow: FlowScale::formal(),
            inline,
            decoration: DecorationTokens::from_palette(&palette),
            syntax,
        }
    }

    pub fn one_dark() -> Self {
        Self::from_variant(Palette::one_dark(), SyntaxTokens::one_dark())
    }

    pub fn one_light() -> Self {
        Self::from_variant(Palette::one_light(), SyntaxTokens::one_light())
    }

    fn from_variant(palette: Palette, syntax: SyntaxTokens) -> Self {
        let mut paint = PaintTokens::from_palette(&palette);
        paint.canvas = palette.canvas;
        paint.caret = palette.caret;
        paint.selection = palette.selection;
        paint.quote_bar = palette.slate_200;
        paint.code_fill = palette.slate_800;
        paint.code_border = palette.slate_300;
        paint.table_grid = palette.slate_300;
        paint.table_border = palette.slate_200;
        paint.table_head_fill = palette.slate_800;
        paint.list_marker = palette.slate_500;
        paint.task_border = palette.slate_200;
        paint.task_checked = palette.link;
        paint.rule = palette.slate_200;
        paint.search_match = palette.search_match;
        paint.search_match_active = palette.search_match_active;

        let mut type_scale = TypeScale::from_palette(&palette);
        type_scale.body.color = palette.slate_700;
        for role in &mut type_scale.heading {
            role.color = palette.slate_700;
        }
        type_scale.heading[4].color = palette.slate_600;
        type_scale.heading[5].color = palette.slate_600;
        type_scale.code.color = palette.code_fg;
        type_scale.quote.color = palette.slate_600;
        type_scale.table.color = palette.slate_600;
        type_scale.table_header.color = palette.slate_700;
        type_scale.image.color = palette.slate_500;
        type_scale.footnote.color = palette.slate_600;
        type_scale.task_done.color = palette.slate_600;

        let mut inline = InlineTokens::from_palette(&palette);
        inline.inline_code = palette.inline_code;
        inline.inline_code_fill = palette.inline_code_fill;
        inline.image_fill = palette.slate_800;
        inline.image_border = palette.slate_200;
        inline.strikethrough = palette.slate_600;
        inline.task_strike = ThemeColor {
            a: 0.5,
            ..palette.slate_600
        };
        inline.link_underline = ThemeColor {
            a: 0.4,
            ..palette.link
        };

        let mut decoration = DecorationTokens::from_palette(&palette);
        decoration.alert_important = palette.bad;

        Self {
            edge_scale: 1.0,
            paint,
            app: AppTokens::from_palette(&palette),
            chrome: ChromeTokens::from_palette(&palette),
            type_scale,
            boxes: BoxScale::formal(),
            flow: FlowScale::formal(),
            inline,
            decoration,
            syntax,
        }
    }

    pub fn is_dark(&self) -> bool {
        self.paint.canvas.l < 0.5
    }

    pub fn layout_metrics_eq(&self, other: &Self) -> bool {
        self.layout_theme() == other.layout_theme()
            && self.type_scale.without_colors() == other.type_scale.without_colors()
            && self.inline.without_colors() == other.inline.without_colors()
            && self.decoration.without_colors() == other.decoration.without_colors()
    }

    pub fn type_role(&self, kind: BlockKind) -> TypeRole {
        match kind {
            BlockKind::Heading(n) => self.type_scale.heading[n.clamp(1, 6) as usize - 1],
            BlockKind::CodeBlock => self.type_scale.code,
            BlockKind::BlockQuote => self.type_scale.quote,
            BlockKind::FootnoteDefinition => self.type_scale.footnote,
            BlockKind::TableCell => self.type_scale.table,
            BlockKind::Image => self.type_scale.image,
            BlockKind::MetadataBlock => self.type_scale.code,
            BlockKind::DocRoot
            | BlockKind::DocStart
            | BlockKind::Paragraph
            | BlockKind::List
            | BlockKind::ListItem
            | BlockKind::Table
            | BlockKind::TableRow
            | BlockKind::ThematicBreak
            | BlockKind::Mermaid
            | BlockKind::Math => self.type_scale.body,
        }
    }

    pub fn type_role_for(&self, kind: BlockKind, slot: TypeSlot) -> TypeRole {
        match slot {
            TypeSlot::FromKind => self.type_role(kind),
            TypeSlot::Quote => self.type_scale.quote,
            TypeSlot::TableHeader => self.type_scale.table_header,
            TypeSlot::Footnote => self.type_scale.footnote,
            TypeSlot::TaskDone => self.type_scale.task_done,
        }
    }

    pub fn box_style(&self, kind: BlockKind) -> BoxLayoutStyle {
        let mut style = match kind {
            BlockKind::DocRoot => self.boxes.doc_root,
            BlockKind::DocStart => self.boxes.doc_start,
            BlockKind::Paragraph => self.boxes.paragraph,
            BlockKind::Heading(n) => self.boxes.heading[n.clamp(1, 6) as usize - 1],
            BlockKind::CodeBlock | BlockKind::MetadataBlock => self.boxes.code,
            BlockKind::BlockQuote => self.boxes.quote,
            BlockKind::FootnoteDefinition => self.boxes.footnote,
            BlockKind::List => self.boxes.list,
            BlockKind::ListItem => self.boxes.list_item,
            BlockKind::Table => self.boxes.table,
            BlockKind::TableRow => self.boxes.table_row,
            BlockKind::TableCell => self.boxes.table_cell,
            BlockKind::ThematicBreak => self.boxes.rule,
            BlockKind::Image => self.boxes.image,
            BlockKind::Mermaid => self.boxes.mermaid,
            BlockKind::Math => self.boxes.math,
        };
        let k = self.edge_scale;
        if k != 1.0 {
            let scale_edges = |e: Edges| Edges {
                top: e.top * k,
                right: e.right * k,
                bottom: e.bottom * k,
                left: e.left * k,
            };
            style.margin = scale_edges(style.margin);
            style.padding = scale_edges(style.padding);
            style.border = scale_edges(style.border);
            style.gap *= k;
        }
        style
    }

    pub fn layout_theme(&self) -> LayoutTheme {
        let k = self.edge_scale;
        LayoutTheme::from_resolver(|kind| self.box_style(kind))
            .with_list_metrics(ListMetrics {
                list_gutter_min: self.decoration.list_gutter_min * k,
                list_digit_width: self.decoration.list_digit_width * k,
                list_task_extra: self.decoration.list_task_extra * k,
                list_marker_gap: self.decoration.list_marker_gap * k,
                list_tight_gap: self.boxes.list.gap * k,
                list_loose_gap: self.boxes.paragraph.margin.top * k,
                list_nested_top: self.decoration.list_nested_top * k,
                list_nested_indent: self.boxes.list.padding.left * k,
            })
            .with_flow_metrics(FlowMetrics {
                paragraph_lead_top: self.flow.paragraph_lead_top * k,
                quote_paragraph_top: self.flow.quote_paragraph_top * k,
                quote_alert_lead: self.flow.quote_alert_lead * k,
                quote_paragraph_slot: self.flow.quote_paragraph_slot,
                list_item_lead_zero: self.flow.list_item_lead_zero,
                doc_lead_zero: self.flow.doc_lead_zero,
                footnote_item_top: self.flow.footnote_item_top * k,
            })
    }
}
#[cfg(test)]
mod tests {
    use super::{DocumentTheme, FontStyle, FontWeight, TypeSlot};
    use md_core::block::BlockKind;

    #[test]
    fn v2_variants_keep_theme_selection_in_the_token_layer() {
        let dark = DocumentTheme::one_dark();
        assert_eq!(dark.paint.canvas.to_css_hex(), "#282c33");
        assert_eq!(dark.paint.code_fill.to_css_hex(), "#21252c");
        assert_eq!(dark.inline.image_fill.to_css_hex(), "#21252c");
        assert_eq!(dark.inline.image_border.to_css_hex(), "#464b57");
        assert_eq!(dark.paint.task_border.to_css_hex(), "#464b57");
        assert_eq!(dark.paint.task_checked.to_css_hex(), "#74ade8");
        assert_eq!(dark.paint.task_check.to_css_hex(), "#ffffff");
        assert_eq!(dark.decoration.task_size, 16.0);
        assert_eq!(dark.decoration.task_radius, 4.0);
        assert_eq!(dark.decoration.task_border, 1.5);
        assert_eq!(dark.decoration.task_gap, 10.0);
        assert_eq!(dark.type_scale.task_done.color.to_css_hex(), "#a9afbc");
        assert!((dark.inline.task_strike.a - 0.5).abs() < f32::EPSILON);
        assert_eq!(dark.decoration.placeholder_label_size, 13.0);
        assert_eq!(dark.paint.table_head_fill.to_css_hex(), "#21252c");
        assert_eq!(dark.paint.code_border.to_css_hex(), "#363c46");
        assert_eq!(dark.decoration.code_radius, 6.0);
        assert_eq!(dark.decoration.code_border, 1.0);
        assert_eq!(dark.decoration.code_max_height, 420.0);
        assert_eq!(dark.decoration.math_max_height, 420.0);
        assert_eq!(
            dark.decoration
                .well_max_height(md_core::block::BlockKind::CodeBlock, false),
            Some(420.0)
        );
        assert_eq!(
            dark.decoration
                .well_max_height(md_core::block::BlockKind::Math, false),
            Some(420.0)
        );
        assert_eq!(
            dark.decoration
                .well_max_height(md_core::block::BlockKind::Mermaid, false),
            Some(420.0)
        );
        assert_eq!(
            dark.decoration
                .well_max_height(md_core::block::BlockKind::Image, false),
            Some(720.0)
        );
        assert_eq!(
            dark.decoration
                .well_max_height(md_core::block::BlockKind::Mermaid, true),
            Some(420.0)
        );
        assert_eq!(
            dark.decoration
                .well_max_height(md_core::block::BlockKind::Paragraph, false),
            None
        );
        assert_eq!(dark.decoration.well_lang_size, 10.5);
        assert_eq!(dark.decoration.well_lang_right, 10.0);
        assert_eq!(dark.decoration.well_lang_top, 6.0);
        assert_eq!(dark.decoration.well_lang.to_css_hex(), "#878a98");
        assert_eq!(dark.type_scale.code.color.to_css_hex(), "#acb2be");
        assert_eq!(dark.inline.inline_code.to_css_hex(), "#e06c75");
        assert_eq!(dark.inline.inline_code_fill.to_css_hex(), "#343a45");
        assert_eq!(dark.inline.inline_code_radius, 4.0);

        assert_eq!(dark.inline.inline_code_pad_x, 3.0);
        assert_eq!(dark.inline.inline_code_pad_y, 1.0);
        assert!((dark.paint.search_match.a - 0.22).abs() < f32::EPSILON);
        assert!((dark.paint.search_match_active.a - 0.45).abs() < f32::EPSILON);

        let light = DocumentTheme::one_light();
        assert_eq!(light.paint.canvas.to_css_hex(), "#fafafa");
        assert_eq!(light.paint.code_fill.to_css_hex(), "#efefef");
        assert_eq!(light.inline.image_fill.to_css_hex(), "#efefef");
        assert_eq!(light.inline.image_border.to_css_hex(), "#c9c9ca");
        assert_eq!(light.paint.task_border.to_css_hex(), "#c9c9ca");
        assert_eq!(light.paint.task_checked.to_css_hex(), "#5c78e2");
        assert_eq!(light.type_scale.task_done.color.to_css_hex(), "#58585a");
        assert_eq!(light.paint.code_border.to_css_hex(), "#dfdfe0");
        assert_eq!(light.decoration.code_radius, 6.0);
        assert_eq!(light.decoration.code_border, 1.0);
        assert_eq!(light.decoration.code_max_height, 420.0);
        assert_eq!(light.decoration.math_max_height, 420.0);
        assert_eq!(light.type_scale.code.color.to_css_hex(), "#383a40");
        assert_eq!(light.inline.inline_code.to_css_hex(), "#d36151");
        assert_eq!(light.inline.inline_code_fill.to_css_hex(), "#ececec");
        assert!((light.paint.search_match.a - 0.16).abs() < f32::EPSILON);
        assert!((light.paint.search_match_active.a - 0.34).abs() < f32::EPSILON);

        assert_eq!(DocumentTheme::formal().paint.canvas.to_css_hex(), "#ffffff");
    }

    #[test]
    fn is_dark_reads_the_canvas_not_the_whole_struct() {
        assert!(DocumentTheme::one_dark().is_dark());
        assert!(!DocumentTheme::one_light().is_dark());
        assert!(!DocumentTheme::formal().is_dark());

        let mut tweaked = DocumentTheme::one_dark();
        tweaked.type_scale.body.color = super::ThemeColor::new(0.0, 1.0, 0.5, 1.0);
        assert_ne!(tweaked, DocumentTheme::one_dark());
        assert!(
            tweaked.is_dark(),
            "changing a body color should not flip the shell to light"
        );
    }

    #[test]
    fn dark_and_light_differ_only_in_color() {
        let dark = DocumentTheme::one_dark();
        let light = DocumentTheme::one_light();
        assert_ne!(dark, light);
        assert!(dark.layout_metrics_eq(&light));
        assert!(light.layout_metrics_eq(&dark));
        assert!(dark.layout_metrics_eq(&dark));
    }

    #[test]
    fn recoloring_any_token_layer_leaves_the_geometry_alone() {
        let base = DocumentTheme::one_dark();
        let hot = super::ThemeColor::new(0.0, 1.0, 0.5, 1.0);

        let mut t = base;
        t.paint.canvas = hot;
        t.paint.caret = hot;
        assert!(base.layout_metrics_eq(&t), "canvas and caret colors");

        let mut t = base;
        t.type_scale.body.color = hot;
        t.type_scale.heading[2].color = hot;
        assert!(base.layout_metrics_eq(&t), "body and heading colors");

        let mut t = base;
        t.syntax.keyword = hot;
        t.syntax.string = hot;
        assert!(base.layout_metrics_eq(&t), "syntax highlight colors");

        let mut t = base;
        t.inline.link = hot;
        t.inline.inline_code_fill = hot;
        assert!(base.layout_metrics_eq(&t), "inline colors");

        let mut t = base;
        t.chrome.scrollbar_thumb = hot;
        assert!(base.layout_metrics_eq(&t), "scrollbar colors");

        let mut t = base;
        t.app.bar_bg = hot;
        t.app.text = hot;
        assert!(base.layout_metrics_eq(&t), "shell colors");

        let mut t = base;
        t.decoration.alert_warning = hot;
        t.decoration.well_lang = hot;
        assert!(base.layout_metrics_eq(&t), "component colors");
    }

    #[test]
    fn anything_that_moves_a_glyph_counts_as_a_geometry_change() {
        let base = DocumentTheme::one_dark();

        let mut t = base;
        t.type_scale.body.size_px += 1.0;
        assert!(!base.layout_metrics_eq(&t), "body font size");

        let mut t = base;
        t.type_scale.footnote.line_height_em += 0.1;
        assert!(!base.layout_metrics_eq(&t), "footnote line height");

        let mut t = base;
        t.type_scale.heading[0].letter_spacing_px += 0.1;
        assert!(!base.layout_metrics_eq(&t), "heading letter spacing");

        let mut t = base;
        t.type_scale.body.family = super::SYSTEM_SERIF;
        assert!(!base.layout_metrics_eq(&t), "body font family");

        let mut t = base;
        t.type_scale.quote.weight = FontWeight::BLACK;
        assert!(!base.layout_metrics_eq(&t), "quote weight");

        let mut t = base;
        t.inline.strong_weight = FontWeight::MEDIUM;
        assert!(!base.layout_metrics_eq(&t), "inline bold weight");

        let mut t = base;
        t.inline.emphasis_style = FontStyle::Normal;
        assert!(!base.layout_metrics_eq(&t), "inline emphasis slant");

        let mut t = base;
        t.edge_scale = 1.25;
        assert!(!base.layout_metrics_eq(&t), "layout density");

        let mut t = base;
        t.boxes.paragraph.margin.top += 1.0;
        assert!(!base.layout_metrics_eq(&t), "paragraph spacing");

        let mut t = base;
        t.flow.paragraph_lead_top += 1.0;
        assert!(!base.layout_metrics_eq(&t), "first paragraph top spacing");

        let mut t = base;
        t.decoration.code_max_height += 1.0;
        assert!(!base.layout_metrics_eq(&t), "code well height cap");

        let mut t = base;
        t.decoration.list_marker_gap += 1.0;
        assert!(!base.layout_metrics_eq(&t), "list marker gap");
    }

    #[test]
    fn the_appearance_controls_all_register_as_geometry() {
        use super::{Appearance, Density};
        let base = Appearance::default();
        for d in Density::ALL {
            let mut a = base;
            a.density = d;
            let same = a.density == base.density;
            assert_eq!(
                base.document_theme().layout_metrics_eq(&a.document_theme()),
                same,
                "density {d:?}"
            );
        }
        let bigger = base.with_body_size_px(20.0);
        assert!(
            !base
                .document_theme()
                .layout_metrics_eq(&bigger.document_theme())
        );
    }

    #[test]
    fn v2_editor_spacing_matches_box_scale() {
        let boxes = DocumentTheme::one_dark().boxes;
        assert_eq!(boxes.doc_root.padding.top, 32.0);
        assert_eq!(boxes.doc_root.padding.right, 48.0);
        assert_eq!(boxes.doc_root.padding.bottom, 48.0);
        assert_eq!(boxes.doc_root.padding.left, 48.0);
        assert_eq!(boxes.paragraph.margin.top, 20.0);
        assert_eq!(boxes.heading[0].margin.top, 0.0);
        assert_eq!(boxes.heading[1].margin.top, 48.0);
        assert_eq!(boxes.heading[2].margin.top, 32.0);
        assert_eq!(boxes.heading[3].margin.top, 24.0);
        assert_eq!(boxes.heading[4].margin.top, 20.0);
        assert_eq!(boxes.heading[5].margin.top, 20.0);
        assert_eq!(boxes.quote.margin.top, 32.0);
        assert_eq!(boxes.quote.padding.left, 20.0);
        assert_eq!(boxes.quote.border.left, 4.0);
        assert_eq!(boxes.list.margin.top, 20.0);
        assert_eq!(boxes.list.padding.left, 40.0);
        assert_eq!(boxes.list.gap, 8.0);
        assert_eq!(boxes.code.margin.top, 32.0);
        assert_eq!(boxes.code.padding.top, 24.0);
        assert_eq!(boxes.code.padding.right, 16.0);
        assert_eq!(boxes.code.padding.bottom, 12.0);
        assert_eq!(boxes.code.padding.left, 16.0);
        assert_eq!(boxes.table.margin.top, 32.0);
        assert_eq!(boxes.rule.margin.top, 24.0);
        assert_eq!(boxes.image.margin.top, 20.0);
        assert_eq!(boxes.mermaid.margin.top, 32.0);
        assert_eq!(boxes.mermaid.padding.top, 22.0);
        assert_eq!(boxes.mermaid.padding.right, 16.0);
        assert_eq!(boxes.mermaid.padding.bottom, 22.0);
        assert_eq!(boxes.mermaid.padding.left, 16.0);
        assert_eq!(boxes.math.margin.top, 24.0);
        assert_eq!(boxes.math.margin.bottom, 4.0);
        assert_eq!(boxes.math.padding.top, 20.0);
        assert_eq!(boxes.math.padding.right, 16.0);
        assert_eq!(boxes.math.padding.bottom, 20.0);
        assert_eq!(boxes.math.padding.left, 16.0);
        assert_eq!(DocumentTheme::one_dark().decoration.list_nested_top, 8.0);
        let flow = DocumentTheme::one_dark().flow;
        assert_eq!(flow.quote_paragraph_top, 0.0);
        assert_eq!(flow.paragraph_lead_top, 24.0);
        assert_eq!(flow.quote_paragraph_slot, TypeSlot::Quote);
        let dark = DocumentTheme::one_dark();
        assert_eq!(dark.type_scale.quote.weight, FontWeight::MEDIUM);
        assert_eq!(dark.type_scale.quote.style, FontStyle::Italic);
        assert_eq!(dark.type_scale.heading[0].letter_spacing_px, -0.3);
        assert!((dark.inline.link_underline.a - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn v2_nested_list_indents_another_gutter() {
        use md_core::block::BlockKind;
        use md_core::document::{editor_options, load_markdown};
        use md_layout::box_tree::{BoxOwner, BoxRole};
        use md_layout::compose::compose;

        let theme = DocumentTheme::one_dark();
        let doc = load_markdown("- a\n  - b\n", editor_options());
        let tree = compose(&doc, &theme.layout_theme());
        let mut paras: Vec<_> = tree
            .nodes()
            .values()
            .filter(|n| n.kind() == BlockKind::Paragraph && n.id().role == BoxRole::Frame)
            .collect();
        paras.sort_by_key(|n| match n.id().owner {
            BoxOwner::Block(b) => b,
            BoxOwner::DocStart => 0,
        });
        assert_eq!(paras.len(), 2);
        let content_x = |id| {
            let chain = tree.ancestor_chain(id);
            let mut x = 0.0;
            for anc in &chain[..chain.len().saturating_sub(1)] {
                let s = tree.style(*anc);
                x += s.padding.left + s.border.left;
            }
            x
        };
        let parent_x = content_x(paras[0].id());
        let nested_x = content_x(paras[1].id());
        assert_eq!(
            nested_x - parent_x,
            theme.boxes.list.padding.left,
            "parent {parent_x} nested {nested_x}"
        );
    }

    #[test]
    fn metadata_uses_code_type_and_box_roles_together() {
        let theme = DocumentTheme::one_dark();
        assert_eq!(
            theme.type_role(BlockKind::MetadataBlock),
            theme.type_scale.code,
            "metadata text must match its code box"
        );
        assert_eq!(
            theme.box_style(BlockKind::MetadataBlock),
            theme.box_style(BlockKind::CodeBlock),
            "metadata geometry must match its code box"
        );
    }
}

use super::GpuiShaper;
use super::artifact::{ShapeArtifact, ShapeMedia};
use super::cache::{ShapeCache, Stats};
use super::resolved::{ResolvedTypes, resolve_type};
use crate::gpui_theme::ThemeColorExt;
use gpui::App;
use md_core::Px;
use md_core::block::BlockKind;
use md_layout::box_tree::TypeSlot;
use md_theme::DocumentTheme;
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

impl GpuiShaper {
    pub fn new(
        window: &gpui::Window,
        cx: &App,
        theme: &DocumentTheme,
        scale: f64,
        cache: Rc<ShapeCache>,
        media: ShapeMedia,
    ) -> Self {
        let ShapeMedia {
            mermaid_fitted,
            math_metrics,
            math_gen,
            image_sizes,
            image_failed,
            image_gen,
            link_dests,
            link_raw,
            block_image_dest,
            block_code_lang,
        } = media;
        let roles = ResolvedTypes {
            body: resolve_type(cx, theme.type_role(BlockKind::Paragraph), scale),
            headings: [
                resolve_type(cx, theme.type_role(BlockKind::Heading(1)), scale),
                resolve_type(cx, theme.type_role(BlockKind::Heading(2)), scale),
                resolve_type(cx, theme.type_role(BlockKind::Heading(3)), scale),
                resolve_type(cx, theme.type_role(BlockKind::Heading(4)), scale),
                resolve_type(cx, theme.type_role(BlockKind::Heading(5)), scale),
                resolve_type(cx, theme.type_role(BlockKind::Heading(6)), scale),
            ],
            code: resolve_type(cx, theme.type_scale.code, scale),
            quote: resolve_type(cx, theme.type_scale.quote, scale),
            table: resolve_type(cx, theme.type_scale.table, scale),
            table_header: resolve_type(cx, theme.type_scale.table_header, scale),
            image: resolve_type(cx, theme.type_scale.image, scale),
            footnote: resolve_type(cx, theme.type_scale.footnote, scale),
            task_done: {
                let mut r = resolve_type(cx, theme.type_scale.task_done, scale);
                r.strikethrough = Some(theme.inline.task_strike.hsla());
                r
            },
        };
        GpuiShaper {
            text_system: window.text_system().clone(),
            roles,
            cache,
            stats: RefCell::new(Stats::default()),
            scale,
            inline: theme.inline,
            decoration: theme.decoration,
            mermaid_fitted,
            math_metrics,
            image_sizes,
            image_failed,
            link_dests,
            link_raw,
            block_image_dest,
            block_code_lang,
            syntax: theme.syntax,
            math_gen,
            image_gen,
        }
    }

    pub fn stats(&self) -> Stats {
        *self.stats.borrow()
    }

    pub fn env_fingerprint(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.scale as f32).to_bits().hash(&mut h);
        self.inline.fingerprint().hash(&mut h);
        self.decoration.fingerprint().hash(&mut h);
        format!("{:?}", self.roles).hash(&mut h);
        self.syntax.fingerprint().hash(&mut h);
        h.finish()
    }

    pub fn style_fingerprint(&self, kind: BlockKind) -> u64 {
        self.roles.for_kind(kind).style_fp
    }

    pub fn body_row_advance(&self) -> Px {
        self.roles.body.row_advance
    }

    pub fn row_advance_for(&self, kind: BlockKind) -> Px {
        self.roles.for_kind(kind).row_advance
    }

    pub fn caret_ink(
        &self,
        kind: BlockKind,
        slot: TypeSlot,
        art: &ShapeArtifact,
        row: u32,
    ) -> (Px, Px) {
        let role = self.roles.for_slot(kind, slot);
        let dy = art
            .bands
            .get(row as usize)
            .map(|b| b.text_dy)
            .unwrap_or(0.0);
        (dy + role.ink_top_in_line(), role.ink_height())
    }
}

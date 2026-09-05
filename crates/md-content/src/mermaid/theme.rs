use super::{MermaidKey, RasterSpec};
use md_theme::{DocumentTheme, ThemeColor};
use merman::svg::{
    HostTheme, HostThemeAppearance, Presentation, RenderError, RootBackgroundPostprocessor,
    SvgPipeline, SvgPostprocessContext, SvgPostprocessor, ThemeRole,
};
use std::borrow::Cow;

pub fn theme_fingerprint(theme: &DocumentTheme) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();

    5u8.hash(&mut h);
    hash_color(&mut h, theme.paint.canvas);
    hash_color(&mut h, theme.type_scale.body.color);
    hash_color(&mut h, theme.paint.table_border);
    hash_color(&mut h, theme.app.text_disabled);
    hash_color(&mut h, theme.app.ok);
    hash_color(&mut h, theme.app.warn);
    hash_color(&mut h, theme.app.syn_red);
    for color in series_colors(theme) {
        hash_color(&mut h, color);
    }
    hash_color(&mut h, theme.app.panel_bg);
    hash_color(&mut h, theme.app.active);
    theme.type_scale.body.family.hash(&mut h);
    (theme.decoration.mermaid_max_width as f32)
        .to_bits()
        .hash(&mut h);
    (theme.decoration.mermaid_max_height as f32)
        .to_bits()
        .hash(&mut h);
    theme.type_scale.body.size_px.to_bits().hash(&mut h);
    h.finish()
}

fn hash_color<H: std::hash::Hasher>(h: &mut H, c: ThemeColor) {
    use std::hash::Hash;
    c.h.to_bits().hash(h);
    c.s.to_bits().hash(h);
    c.l.to_bits().hash(h);
    c.a.to_bits().hash(h);
}

pub fn spec_from_theme(theme: &DocumentTheme, key: MermaidKey) -> RasterSpec {
    let canvas = theme.paint.canvas;
    let text = theme.type_scale.body.color.to_css_hex();
    let border = theme.paint.table_border.to_css_hex();
    RasterSpec {
        theme_fp: key.theme_fp,
        dark: canvas.l <= 0.5,
        canvas: canvas.to_css_hex(),

        surface: theme.app.panel_bg.to_css_hex(),
        cluster: theme.app.active.to_css_hex(),
        text,
        subtle: theme.app.text_disabled.to_css_hex(),
        border: border.clone(),

        line: border,
        ok: theme.app.ok.to_css_hex(),
        warn: theme.app.warn.to_css_hex(),
        error: theme.app.syn_red.to_css_hex(),
        series: series_colors(theme).map(|c| c.to_css_hex()),
        font_size: format!("{}px", theme.type_scale.body.size_px),
        font_family: mermaid_font_family(theme.type_scale.body.family),
        fit_w: key.width_q,
        fit_h: key.max_h,
        dpr: key.dpr(),
    }
}

fn mermaid_font_family(family: &str) -> String {
    let family = match family {
        ".SystemUIFont" => "system-ui",
        other => other,
    };
    if family
        .split(',')
        .any(|f| f.trim().eq_ignore_ascii_case("sans-serif"))
    {
        family.to_string()
    } else {
        format!("{family}, sans-serif")
    }
}

fn series_colors(theme: &DocumentTheme) -> [ThemeColor; 8] {
    let s = &theme.syntax;
    [
        s.symbol,
        s.number,
        s.macro_name,
        s.string,
        s.character,
        s.function,
        s.keyword,
        theme.app.accent,
    ]
}

pub(super) fn presentation(spec: &RasterSpec) -> Presentation {
    let appearance = if spec.dark {
        HostThemeAppearance::Dark
    } else {
        HostThemeAppearance::Light
    };
    let mut theme = HostTheme::new()
        .with_appearance(appearance)
        .try_with_font_size(spec.font_size.as_str())
        .and_then(|t| t.try_with_font_family(spec.font_family.as_str()))
        .and_then(|t| t.try_with_series_palette(spec.series.iter().cloned()))
        .expect("host theme values are validated CSS declarations");
    for (role, value) in roles(spec) {
        theme = theme
            .try_with_role(role, value)
            .expect("host theme values are validated CSS declarations");
    }
    Presentation::new().with_theme(theme)
}

fn roles(spec: &RasterSpec) -> Vec<(ThemeRole, &str)> {
    vec![
        (ThemeRole::Canvas, spec.canvas.as_str()),
        (ThemeRole::Surface, spec.surface.as_str()),
        (ThemeRole::SurfaceAlt, spec.surface.as_str()),
        (ThemeRole::Text, spec.text.as_str()),
        (ThemeRole::SubtleText, spec.subtle.as_str()),
        (ThemeRole::Border, spec.border.as_str()),
        (ThemeRole::Line, spec.line.as_str()),
        (ThemeRole::EdgeLabelBackground, spec.canvas.as_str()),
        (ThemeRole::ClusterBackground, spec.cluster.as_str()),
        (ThemeRole::ClusterBorder, spec.border.as_str()),
        (ThemeRole::NoteBackground, spec.surface.as_str()),
        (ThemeRole::NoteBorder, spec.border.as_str()),
        (ThemeRole::NoteText, spec.text.as_str()),
        (ThemeRole::ActorBackground, spec.surface.as_str()),
        (ThemeRole::ActorBorder, spec.border.as_str()),
        (ThemeRole::ActorText, spec.text.as_str()),
        (ThemeRole::Error, spec.error.as_str()),
        (ThemeRole::Warning, spec.warn.as_str()),
        (ThemeRole::Success, spec.ok.as_str()),
    ]
}

pub(super) fn pipeline() -> SvgPipeline {
    SvgPipeline::resvg_safe()
        .with_postprocessor(RootBackgroundPostprocessor::new("transparent"))
        .with_postprocessor(PadViewBox)
}

struct PadViewBox;

impl SvgPostprocessor for PadViewBox {
    fn name(&self) -> &'static str {
        "md-test-pad-viewbox"
    }

    fn process<'a>(
        &self,
        svg: Cow<'a, str>,
        _ctx: &SvgPostprocessContext<'_>,
    ) -> Result<Cow<'a, str>, RenderError> {
        Ok(Cow::Owned(super::svg::pad_svg_viewbox(
            &svg,
            super::VIEWBOX_PAD,
        )))
    }
}

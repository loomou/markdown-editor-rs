use super::{MermaidKey, RasterSpec};
use md_theme::{DocumentTheme, ThemeColor};
use merman::svg::{
    CanvasPaint, DiagramFamilyId, DiagramTheme, DiagramThemeCompiler, DiagramThemeSpec, FontStack,
    MermaidThemeCompatibility, OrdinalPalette, RenderError, RootBackgroundPostprocessor,
    SvgPipeline, SvgPostprocessContext, SvgPostprocessor, ThemeColorValue,
    ThemeCompileValidationError, ThemeRule, ThemeRuleSet, ThemeStylePatch, ThemeTarget,
    ThemeTextStyle, ThemeVariant, TypographySpec,
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

pub(super) fn compile(spec: &RasterSpec) -> Result<DiagramTheme, crate::Error> {
    let theme = build_spec(spec).map_err(theme_error)?;
    DiagramThemeCompiler::new()
        .compile(theme)
        .map_err(theme_error)
}

fn theme_error(error: impl std::fmt::Display) -> crate::Error {
    crate::Error::Mermaid(error.to_string().into())
}

fn build_spec(spec: &RasterSpec) -> Result<DiagramThemeSpec, ThemeCompileValidationError> {
    Ok(DiagramThemeSpec::new()
        .with_mermaid_compatibility(build_compat(spec)?)
        .with_typography(build_typography(spec)?)
        .with_styles(build_styles(spec)?))
}

fn build_compat(
    spec: &RasterSpec,
) -> Result<MermaidThemeCompatibility, ThemeCompileValidationError> {
    let canvas = spec.canvas.as_str();
    let surface = spec.surface.as_str();
    let text = spec.text.as_str();
    let subtle = spec.subtle.as_str();
    let border = spec.border.as_str();
    let line = spec.line.as_str();
    let mut compat = MermaidThemeCompatibility::default()
        .with_theme("base")?
        .with_dark_mode(spec.dark)?;
    for (key, value) in [
        ("fontFamily", spec.font_family.as_str()),
        ("fontSize", spec.font_size.as_str()),
        ("background", canvas),
        ("primaryColor", surface),
        ("mainBkg", surface),
        ("secondaryColor", surface),
        ("tertiaryColor", surface),
        ("primaryTextColor", text),
        ("nodeTextColor", text),
        ("textColor", text),
        ("titleColor", text),
        ("secondaryTextColor", subtle),
        ("tertiaryTextColor", subtle),
        ("primaryBorderColor", border),
        ("nodeBorder", border),
        ("lineColor", line),
        ("arrowheadColor", line),
        ("edgeLabelBackground", canvas),
        ("clusterBkg", spec.cluster.as_str()),
        ("clusterBorder", border),
        ("noteBkgColor", surface),
        ("noteBorderColor", border),
        ("noteTextColor", text),
        ("actorBkg", surface),
        ("actorBorder", border),
        ("actorTextColor", text),
        ("actorLineColor", border),
        ("signalColor", line),
        ("signalTextColor", text),
        ("labelTextColor", text),
        ("loopTextColor", text),
        ("labelBoxBkgColor", surface),
        ("labelBoxBorderColor", border),
        ("activationBkgColor", surface),
        ("activationBorderColor", border),
        ("classText", text),
        ("labelColor", text),
        ("transitionColor", line),
        ("transitionLabelColor", text),
        ("stateLabelColor", text),
        ("stateBkg", surface),
        ("stateBorder", border),
        ("specialStateColor", line),
        ("compositeBackground", canvas),
        ("attributeBackgroundColorOdd", surface),
        ("attributeBackgroundColorEven", surface),
        ("rowOdd", surface),
        ("rowEven", surface),
        ("requirementBackground", surface),
        ("requirementBorderColor", border),
        ("requirementTextColor", text),
        ("relationColor", line),
        ("relationLabelBackground", canvas),
        ("relationLabelColor", text),
        ("requirementEdgeLabelBackground", canvas),
        ("pieTitleTextColor", text),
        ("pieSectionTextColor", text),
        ("pieLegendTextColor", subtle),
        ("pieStrokeColor", border),
        ("pieOuterStrokeColor", border),
        ("commitLabelColor", text),
        ("commitLabelBackground", surface),
        ("commitLineColor", line),
        ("tagLabelColor", text),
        ("tagLabelBackground", surface),
        ("tagLabelBorder", border),
        ("quadrant1Fill", surface),
        ("quadrant2Fill", surface),
        ("quadrant3Fill", canvas),
        ("quadrant4Fill", surface),
        ("quadrant1TextFill", text),
        ("quadrant2TextFill", text),
        ("quadrant3TextFill", text),
        ("quadrant4TextFill", text),
        ("quadrantPointFill", line),
        ("quadrantPointTextFill", text),
        ("quadrantTitleFill", text),
        ("quadrantXAxisTextFill", subtle),
        ("quadrantYAxisTextFill", subtle),
        ("quadrantExternalBorderStrokeFill", border),
        ("quadrantInternalBorderStrokeFill", border),
        ("archEdgeColor", line),
        ("archEdgeArrowColor", line),
        ("archGroupBorderColor", border),
        ("emUiFill", surface),
        ("emUiStroke", border),
        ("emRelationStroke", line),
        ("emArrowhead", line),
        ("emSwimlaneBackgroundOdd", spec.cluster.as_str()),
        ("emSwimlaneBackgroundStroke", border),
        ("taskTextDarkColor", text),
        ("taskTextClickableColor", line),
        ("taskTextColor", text),
        ("taskTextOutsideColor", subtle),
        ("taskBkgColor", surface),
        ("taskBorderColor", border),
        ("activeTaskBkgColor", surface),
        ("activeTaskBorderColor", line),
        ("doneTaskBkgColor", surface),
        ("doneTaskBorderColor", spec.ok.as_str()),
        ("critBkgColor", surface),
        ("critBorderColor", spec.error.as_str()),
        ("excludeBkgColor", surface),
        ("gridColor", border),
        ("todayLineColor", spec.warn.as_str()),
        ("vertLineColor", spec.warn.as_str()),
        ("sectionBkgColor", spec.cluster.as_str()),
        ("sectionBkgColor2", surface),
        ("altSectionBkgColor", canvas),
        ("errorBkgColor", spec.error.as_str()),
        ("errorTextColor", text),
        ("faceColor", surface),
        ("border2", border),
    ] {
        compat = compat.with_variable(key, value)?;
    }
    for (index, color) in spec.series.iter().enumerate() {
        compat = compat
            .with_variable(format!("cScale{index}"), color.clone())?
            .with_variable(format!("cScalePeer{index}"), color.clone())?
            .with_variable(format!("cScaleLabel{index}"), text)?
            .with_variable(format!("cScaleInv{index}"), text)?
            .with_variable(format!("git{index}"), color.clone())?
            .with_variable(format!("gitBranchLabel{index}"), text)?
            .with_variable(format!("pie{}", index + 1), color.clone())?
            .with_variable(format!("venn{}", index + 1), color.clone())?
            .with_variable(format!("fillType{index}"), color.clone())?
            .with_variable(format!("actor{index}"), color.clone())?;
    }
    Ok(compat)
}

fn build_typography(spec: &RasterSpec) -> Result<TypographySpec, ThemeCompileValidationError> {
    let families = spec
        .font_family
        .split(',')
        .map(str::trim)
        .filter(|family| !family.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut style = ThemeTextStyle::default().with_font_stack(FontStack::new(families)?);
    if let Some(size) = font_size_px(spec.font_size.as_str()) {
        style = style.with_font_size_px(size)?;
    }
    Ok(TypographySpec::default().with_default(style))
}

fn font_size_px(value: &str) -> Option<f32> {
    let value = value.trim().strip_suffix("px")?.trim();
    let size = value.parse::<f32>().ok()?;
    (size.is_finite() && size > 0.0).then_some(size)
}

fn build_styles(spec: &RasterSpec) -> Result<ThemeRuleSet, ThemeCompileValidationError> {
    let canvas = spec.canvas.as_str();
    let surface = spec.surface.as_str();
    let text = spec.text.as_str();
    let subtle = spec.subtle.as_str();
    let border = spec.border.as_str();
    let line = spec.line.as_str();
    let cluster = spec.cluster.as_str();

    let mut set = ThemeRuleSet::default();
    for (target, fill, stroke) in [
        (ThemeTarget::Text, Some(text), None),
        (ThemeTarget::Node, None, Some(border)),
        (ThemeTarget::Edge, None, Some(line)),
        (ThemeTarget::Cluster, Some(cluster), Some(border)),
        (ThemeTarget::Actor, Some(surface), Some(border)),
        (ThemeTarget::Lifeline, Some(border), Some(border)),
        (ThemeTarget::Message, None, Some(line)),
        (ThemeTarget::State, Some(surface), Some(border)),
        (ThemeTarget::Transition, None, Some(line)),
        (ThemeTarget::TransitionMarker, Some(line), Some(line)),
        (ThemeTarget::TransitionLabelBackground, Some(canvas), None),
        (ThemeTarget::Composite, Some(canvas), Some(border)),
        (ThemeTarget::CompositeHeader, Some(surface), Some(border)),
        (ThemeTarget::Note, Some(surface), Some(border)),
        (ThemeTarget::EdgeLabelBackground, Some(canvas), None),
        (
            ThemeTarget::LoopLabelBackground,
            Some(surface),
            Some(border),
        ),
        (ThemeTarget::Entity, Some(surface), Some(border)),
    ] {
        set = rule(set, None, target, None, fill, stroke)?;
    }
    for (target, fill, stroke) in [
        (ThemeTarget::Title, Some(text), None),
        (ThemeTarget::ActorLabel, Some(text), None),
        (ThemeTarget::StateLabel, Some(text), None),
        (ThemeTarget::TransitionLabel, Some(text), None),
        (ThemeTarget::CompositeLabel, Some(text), None),
        (ThemeTarget::NoteLabel, Some(text), None),
        (ThemeTarget::SequenceNumberLabel, Some(subtle), None),
        (ThemeTarget::MessageLabel, Some(text), None),
        (ThemeTarget::LoopLabel, Some(text), None),
        (ThemeTarget::PacketByteLabel, Some(text), None),
        (ThemeTarget::PacketFieldLabel, Some(text), None),
        (ThemeTarget::Axis, Some(text), Some(line)),
        (ThemeTarget::Legend, Some(subtle), None),
    ] {
        set = rule(set, None, target, None, fill, stroke)?;
    }
    for family in [
        DiagramFamilyId::FLOWCHART,
        DiagramFamilyId::C4,
        DiagramFamilyId::SWIMLANE,
        DiagramFamilyId::CLASS,
        DiagramFamilyId::TREE_VIEW,
        DiagramFamilyId::BLOCK,
        DiagramFamilyId::AGENTFLOW,
        DiagramFamilyId::USECASE,
        DiagramFamilyId::VENN,
        DiagramFamilyId::ISHIKAWA,
        DiagramFamilyId::RAILROAD,
        DiagramFamilyId::EVENT_MODELING,
        DiagramFamilyId::PACKET,
        DiagramFamilyId::CYNEFIN,
        DiagramFamilyId::WARDLEY,
    ] {
        set = rule(
            set,
            Some(family),
            ThemeTarget::Node,
            None,
            Some(surface),
            None,
        )?;
    }
    for family in [
        DiagramFamilyId::ISHIKAWA,
        DiagramFamilyId::RAILROAD,
        DiagramFamilyId::EVENT_MODELING,
        DiagramFamilyId::CYNEFIN,
        DiagramFamilyId::WARDLEY,
        DiagramFamilyId::ARCHITECTURE,
    ] {
        set = rule(
            set,
            Some(family),
            ThemeTarget::Marker,
            None,
            Some(line),
            None,
        )?;
    }
    for family in [
        DiagramFamilyId::VENN,
        DiagramFamilyId::ISHIKAWA,
        DiagramFamilyId::RAILROAD,
        DiagramFamilyId::EVENT_MODELING,
        DiagramFamilyId::CYNEFIN,
        DiagramFamilyId::WARDLEY,
        DiagramFamilyId::TREEMAP,
    ] {
        set = rule(
            set,
            Some(family),
            ThemeTarget::NodeLabel,
            None,
            Some(text),
            None,
        )?;
    }
    set = rule(
        set,
        Some(DiagramFamilyId::USECASE),
        ThemeTarget::Marker,
        None,
        Some(line),
        Some(line),
    )?;
    set = rule(
        set,
        Some(DiagramFamilyId::EVENT_MODELING),
        ThemeTarget::ClusterLabel,
        None,
        Some(text),
        None,
    )?;
    for family in [DiagramFamilyId::CYNEFIN, DiagramFamilyId::WARDLEY] {
        set = rule(
            set,
            Some(family),
            ThemeTarget::EdgeLabel,
            None,
            Some(text),
            None,
        )?;
    }
    for (family, target, fill, stroke) in [
        (
            DiagramFamilyId::C4,
            ThemeTarget::NodeLabel,
            Some(text),
            None,
        ),
        (
            DiagramFamilyId::C4,
            ThemeTarget::EdgeLabel,
            Some(text),
            None,
        ),
        (
            DiagramFamilyId::C4,
            ThemeTarget::ClusterLabel,
            Some(text),
            None,
        ),
        (DiagramFamilyId::ER, ThemeTarget::Relation, None, Some(line)),
        (
            DiagramFamilyId::REQUIREMENT,
            ThemeTarget::Requirement,
            Some(surface),
            Some(border),
        ),
        (
            DiagramFamilyId::REQUIREMENT,
            ThemeTarget::Relation,
            None,
            Some(line),
        ),
        (
            DiagramFamilyId::PIE,
            ThemeTarget::PieSlice,
            None,
            Some(text),
        ),
        (
            DiagramFamilyId::QUADRANT_CHART,
            ThemeTarget::ChartSeries,
            Some(spec.series[0].as_str()),
            None,
        ),
    ] {
        set = rule(set, Some(family), target, None, fill, stroke)?;
    }
    set = rule(
        set,
        Some(DiagramFamilyId::STATE),
        ThemeTarget::SpecialState,
        Some(ThemeVariant::Start),
        Some(line),
        Some(line),
    )?;
    set = rule(
        set,
        Some(DiagramFamilyId::STATE),
        ThemeTarget::SpecialState,
        Some(ThemeVariant::End),
        Some(canvas),
        Some(line),
    )?;
    set = rule(
        set,
        None,
        ThemeTarget::SpecialState,
        Some(ThemeVariant::Special),
        Some(line),
        Some(line),
    )?;
    set = rule(
        set,
        None,
        ThemeTarget::SpecialStateInner,
        Some(ThemeVariant::End),
        Some(line),
        Some(line),
    )?;
    for (variant, fill) in [(ThemeVariant::Odd, surface), (ThemeVariant::Even, surface)] {
        set = rule(
            set,
            Some(DiagramFamilyId::ER),
            ThemeTarget::Table,
            Some(variant),
            Some(fill),
            None,
        )?;
    }
    for (variant, fill, stroke, label) in [
        (ThemeVariant::Default, surface, border, text),
        (ThemeVariant::Active, surface, line, text),
        (ThemeVariant::Success, surface, spec.ok.as_str(), text),
        (ThemeVariant::Error, surface, spec.error.as_str(), text),
    ] {
        set = rule(
            set,
            Some(DiagramFamilyId::GANTT),
            ThemeTarget::Task,
            Some(variant),
            Some(fill),
            Some(stroke),
        )?;
        set = rule(
            set,
            Some(DiagramFamilyId::GANTT),
            ThemeTarget::TaskLabel,
            Some(variant),
            Some(label),
            None,
        )?;
    }
    let palette = OrdinalPalette::new(
        spec.series
            .iter()
            .map(|color| ThemeColorValue::parse(color))
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    for target in [
        ThemeTarget::Node,
        ThemeTarget::PieSlice,
        ThemeTarget::ChartSeries,
        ThemeTarget::Task,
    ] {
        set = set.with_ordinal_palette(target, palette.clone());
    }
    Ok(set)
}

#[allow(
    clippy::too_many_arguments,
    reason = "one rule is described by its target, scope, variant, fill, and stroke"
)]
fn rule(
    set: ThemeRuleSet,
    family: Option<DiagramFamilyId>,
    target: ThemeTarget,
    variant: Option<ThemeVariant>,
    fill: Option<&str>,
    stroke: Option<&str>,
) -> Result<ThemeRuleSet, ThemeCompileValidationError> {
    let mut theme_rule = ThemeRule::new(target, style(fill, stroke)?);
    if let Some(family) = family {
        theme_rule = theme_rule.for_family(family);
    }
    if let Some(variant) = variant {
        theme_rule = theme_rule.with_variant(variant);
    }
    Ok(set.with_rule(theme_rule))
}

fn style(
    fill: Option<&str>,
    stroke: Option<&str>,
) -> Result<ThemeStylePatch, ThemeCompileValidationError> {
    let mut style = ThemeStylePatch::default();
    if let Some(fill) = fill {
        style = style.with_fill(CanvasPaint::solid(fill)?);
    }
    if let Some(stroke) = stroke {
        style = style.with_stroke(CanvasPaint::solid(stroke)?);
    }
    Ok(style)
}

pub(super) fn pipeline() -> SvgPipeline {
    SvgPipeline::resvg_safe()
        .with_postprocessor(RootBackgroundPostprocessor::new("transparent"))
        .with_postprocessor(PadViewBox)
}

struct PadViewBox;

impl SvgPostprocessor for PadViewBox {
    fn name(&self) -> &'static str {
        "markdown-editor-rs-pad-viewbox"
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

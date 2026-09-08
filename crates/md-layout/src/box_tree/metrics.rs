use md_core::Px;

#[derive(Clone, Copy, Debug)]
pub struct LeafMetrics {
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

pub(crate) fn estimated_text_width(em_width: Px, text: &str) -> Px {
    let ems: Px = text
        .chars()
        .map(|ch| if ch.is_ascii() { 0.5 } else { 1.0 })
        .sum();
    ems * em_width
}

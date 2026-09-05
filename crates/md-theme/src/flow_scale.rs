use md_core::Px;
use md_layout::box_tree::TypeSlot;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlowScale {
    pub paragraph_lead_top: Px,

    pub quote_paragraph_top: Px,

    pub quote_alert_lead: Px,

    pub quote_paragraph_slot: TypeSlot,

    pub list_item_lead_zero: bool,

    pub doc_lead_zero: bool,

    pub footnote_item_top: Px,
}

impl FlowScale {
    pub(crate) fn formal() -> Self {
        Self {
            paragraph_lead_top: 24.0,
            quote_paragraph_top: 0.0,
            quote_alert_lead: 28.0,
            quote_paragraph_slot: TypeSlot::Quote,
            list_item_lead_zero: true,
            doc_lead_zero: true,
            footnote_item_top: 12.0,
        }
    }
}

use md_core::Px;
use md_layout::spine::FlowItemId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollAnchor {
    pub item: FlowItemId,

    pub within: Px,
}

impl ScrollAnchor {
    pub fn top() -> Self {
        ScrollAnchor {
            item: FlowItemId::NONE,
            within: 0.0,
        }
    }
}

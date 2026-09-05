use md_core::Px;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub top: Px,
    pub right: Px,
    pub bottom: Px,
    pub left: Px,
}

impl Edges {
    pub const ZERO: Edges = Edges {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    pub fn all(v: Px) -> Self {
        Edges {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    pub fn vh(vertical: Px, horizontal: Px) -> Self {
        Edges {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    fn inline_sum(&self) -> Px {
        self.left + self.right
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxDisplay {
    FlowStack,

    IslandRow,

    MeasuredLeaf,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxLayoutStyle {
    pub display: BoxDisplay,
    pub margin: Edges,
    pub padding: Edges,
    pub border: Edges,

    pub gap: Px,
}

impl BoxLayoutStyle {
    pub fn top_border_padding(&self) -> Px {
        self.border.top + self.padding.top
    }

    pub fn bottom_border_padding(&self) -> Px {
        self.border.bottom + self.padding.bottom
    }

    pub fn inline_border_padding(&self) -> Px {
        self.border.inline_sum() + self.padding.inline_sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxLayoutEnvironment {
    pub viewport_width: Px,
}

const DEFAULT_VIEWPORT_WIDTH: Px = 800.0;
pub const DEFAULT_LINE_HEIGHT: Px = 20.0;

impl Default for BoxLayoutEnvironment {
    fn default() -> Self {
        BoxLayoutEnvironment {
            viewport_width: DEFAULT_VIEWPORT_WIDTH,
        }
    }
}

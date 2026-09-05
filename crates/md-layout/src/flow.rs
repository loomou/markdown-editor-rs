use md_core::Px;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeightState {
    Estimated(Px),
    Exact(Px),
}

impl HeightState {
    pub fn px(self) -> Px {
        match self {
            HeightState::Estimated(v) | HeightState::Exact(v) => v,
        }
    }

    pub fn is_exact(self) -> bool {
        matches!(self, HeightState::Exact(_))
    }
}

use md_core::Px;

pub struct SnapOperator {
    scale: f64,
}

impl SnapOperator {
    pub fn new(scale: f64) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        SnapOperator { scale }
    }

    pub fn snap(&self, logical_y: Px) -> Px {
        let logical_y = if logical_y.is_finite() {
            logical_y
        } else {
            0.0
        };
        (logical_y * self.scale).round() / self.scale
    }

    pub fn snap_range(&self, top: Px, bottom: Px) -> (Px, Px) {
        (self.snap(top), self.snap(bottom))
    }
}

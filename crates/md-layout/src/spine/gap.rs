use crate::box_tree::{BoxTree, LayoutBoxId};
use md_core::Px;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlowBoundary {
    Start,
    Child(LayoutBoxId),
    End,
}

pub(crate) fn gap_height(
    tree: &BoxTree,
    parent: LayoutBoxId,
    before: FlowBoundary,
    after: FlowBoundary,
) -> Px {
    let pstyle = *tree.style(parent);
    match (before, after) {
        (FlowBoundary::Start, FlowBoundary::End) => 0.0,
        (FlowBoundary::Start, FlowBoundary::Child(c)) => tree.flow_margins(c).0,
        (FlowBoundary::Child(c), FlowBoundary::End) => tree.flow_margins(c).1,
        (FlowBoundary::Child(a), FlowBoundary::Child(b)) => {
            tree.flow_margins(a).1 + pstyle.gap + tree.flow_margins(b).0
        }

        other => panic!("illegal gap boundary pair {other:?}"),
    }
}

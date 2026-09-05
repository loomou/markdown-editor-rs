use md_core::Px;
use md_layout::style::{BoxDisplay, BoxLayoutStyle, Edges};

fn leaf(margin: Edges, padding: Edges) -> BoxLayoutStyle {
    BoxLayoutStyle {
        display: BoxDisplay::MeasuredLeaf,
        margin,
        padding,
        border: Edges::ZERO,
        gap: 0.0,
    }
}

fn flow(margin: Edges, padding: Edges, border: Edges, gap: Px) -> BoxLayoutStyle {
    BoxLayoutStyle {
        display: BoxDisplay::FlowStack,
        margin,
        padding,
        border,
        gap,
    }
}

fn top(v: Px) -> Edges {
    Edges {
        top: v,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    }
}

fn left(v: Px) -> Edges {
    Edges {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: v,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxScale {
    pub doc_root: BoxLayoutStyle,
    pub doc_start: BoxLayoutStyle,
    pub paragraph: BoxLayoutStyle,
    pub heading: [BoxLayoutStyle; 6],
    pub code: BoxLayoutStyle,
    pub quote: BoxLayoutStyle,
    pub footnote: BoxLayoutStyle,
    pub list: BoxLayoutStyle,
    pub list_item: BoxLayoutStyle,
    pub table: BoxLayoutStyle,
    pub table_row: BoxLayoutStyle,
    pub table_cell: BoxLayoutStyle,
    pub rule: BoxLayoutStyle,
    pub image: BoxLayoutStyle,
    pub mermaid: BoxLayoutStyle,
    pub math: BoxLayoutStyle,
}

impl BoxScale {
    pub(crate) fn formal() -> Self {
        Self {
            doc_root: flow(
                Edges::ZERO,
                Edges {
                    top: 32.0,
                    right: 48.0,
                    bottom: 48.0,
                    left: 48.0,
                },
                Edges::ZERO,
                0.0,
            ),
            doc_start: leaf(Edges::ZERO, Edges::ZERO),
            paragraph: leaf(top(20.0), Edges::ZERO),
            heading: [
                leaf(Edges::ZERO, Edges::ZERO),
                leaf(top(48.0), Edges::ZERO),
                leaf(top(32.0), Edges::ZERO),
                leaf(top(24.0), Edges::ZERO),
                leaf(top(20.0), Edges::ZERO),
                leaf(top(20.0), Edges::ZERO),
            ],
            code: leaf(
                top(32.0),
                Edges {
                    top: 24.0,
                    right: 16.0,
                    bottom: 12.0,
                    left: 16.0,
                },
            ),
            quote: flow(top(32.0), left(20.0), left(4.0), 0.0),
            footnote: flow(top(32.0), left(20.0), left(2.0), 0.0),
            list: flow(top(20.0), left(40.0), Edges::ZERO, 8.0),
            list_item: flow(Edges::ZERO, Edges::ZERO, Edges::ZERO, 0.0),
            table: flow(top(32.0), Edges::all(0.0), Edges::ZERO, 0.0),
            table_row: BoxLayoutStyle {
                display: BoxDisplay::IslandRow,
                margin: Edges::ZERO,
                padding: Edges::ZERO,
                border: Edges::ZERO,
                gap: 0.0,
            },
            table_cell: leaf(Edges::ZERO, Edges::vh(8.0, 8.0)),
            rule: leaf(top(24.0), Edges::vh(8.0, 0.0)),
            image: leaf(top(20.0), Edges::ZERO),
            mermaid: leaf(top(32.0), Edges::vh(22.0, 16.0)),

            math: leaf(
                Edges {
                    top: 24.0,
                    right: 0.0,
                    bottom: 4.0,
                    left: 0.0,
                },
                Edges::vh(20.0, 16.0),
            ),
        }
    }
}

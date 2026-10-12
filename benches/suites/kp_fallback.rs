use gpui::BenchAppContext;
use md_bench::corpus::{Shape, generate, human};
use md_bench::gpui_support::{count_hot_path, empty_media, text_nodes, theme_with_mode};
use md_bench::harness::{doc, environment};
use md_content::shaper::{GpuiShaper, ShapeCache};
use md_layout::box_tree::{BoxTree, LayoutBoxId};
use md_layout::style::BoxLayoutEnvironment;
use md_theme::{DocumentTheme, LineBreakMode};
use std::fmt;

const SEED: u64 = 0x5eed_0f4b;
const SIZES: [usize; 4] = [16 << 10, 32 << 10, 64 << 10, 256 << 10];

const SHAPES: [Shape; 4] = [
    Shape::LongParagraph,
    Shape::LongParagraphZh,
    Shape::LongParagraphMixed,
    Shape::PlainZh,
];

struct KpCase {
    label: String,
    text: String,
    theme: DocumentTheme,
    env: BoxLayoutEnvironment,
    max_items: u32,
    expectation: Option<bool>,
}

impl fmt::Display for KpCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

fn expectation(shape: Shape, size: usize) -> Option<bool> {
    let largest = size == *SIZES.last().expect("SIZES is not empty");
    match shape {
        Shape::PlainZh => Some(false),
        Shape::LongParagraphZh if largest => Some(true),
        _ => None,
    }
}

fn cases() -> Vec<KpCase> {
    let mut out = Vec::with_capacity(SHAPES.len() * SIZES.len());
    for shape in SHAPES {
        for size in SIZES {
            let theme = theme_with_mode(LineBreakMode::Optimal);
            out.push(KpCase {
                label: format!("{}-{}", shape.name(), human(size)),
                text: generate(shape, size, SEED),
                max_items: theme.line_break.params.max_items,
                theme,
                env: environment(),
                expectation: expectation(shape, size),
            });
        }
    }
    out
}

fn shape_all(
    tree: &BoxTree,
    nodes: &[LayoutBoxId],
    shaper: &GpuiShaper,
    env: BoxLayoutEnvironment,
) {
    for id in nodes {
        let node = tree.get(*id);
        shaper.artifact(
            tree.text_of(node),
            tree.runs_of(node),
            tree.content_width(*id, env.viewport_width),
            node.kind(),
            node.shape_ident(),
        );
    }
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "linebreak/kp-limit", sample_size = 10)]
fn kp_limit(case: &KpCase, cx: &mut BenchAppContext<'_, '_>) {
    let doc = doc(&case.text);
    let (tree, nodes) = text_nodes(&doc, &case.theme);
    assert!(
        !nodes.is_empty(),
        "the kp-limit corpus must yield at least one text node"
    );
    let mut window = cx.add_empty_window();
    let shaper = window.update(|window, app| {
        GpuiShaper::new(
            window,
            app,
            &case.theme,
            1.0,
            ShapeCache::new(),
            empty_media(),
        )
    });
    let env = case.env;
    let snapshot = count_hot_path(&format!("linebreak/kp-limit {}", case.label), || {
        shape_all(&tree, &nodes, &shaper, env);
    });
    eprintln!(
        "[linebreak/kp-limit {}] bytes={} text-nodes={} max-items={} fallbacks={}",
        case.label,
        case.text.len(),
        nodes.len(),
        case.max_items,
        snapshot.kp_max_items
    );
    match case.expectation {
        Some(true) => assert!(
            snapshot.kp_max_items > 0,
            "{}: a paragraph past the item cap must record a fallback",
            case.label
        ),
        Some(false) => assert_eq!(
            snapshot.kp_max_items, 0,
            "{}: a document of short paragraphs must stay under the item cap",
            case.label
        ),
        None => {}
    }
    cx.bench_iter(move |_cx| {
        shape_all(&tree, &nodes, &shaper, env);
    });
}

gpui::bench_group!(benches, kp_limit);
gpui::bench_main!(benches);

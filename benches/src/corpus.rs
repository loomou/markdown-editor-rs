#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    PlainEn,
    PlainZh,
    MixedZhEn,
    LongParagraph,
    LongParagraphZh,
    LongParagraphMixed,
    NestedLists,
    Tables,
    CodeFences,
    Alerts,
    Quotes,
    Footnotes,
    FrontMatter,
    Math,
    Mermaid,
    Images,
    KitchenSink,
    AdversarialToken,
    AdversarialLine,
    AdversarialTable,
}

impl Shape {
    pub const ALL: [Shape; 20] = [
        Shape::PlainEn,
        Shape::PlainZh,
        Shape::MixedZhEn,
        Shape::LongParagraph,
        Shape::LongParagraphZh,
        Shape::LongParagraphMixed,
        Shape::NestedLists,
        Shape::Tables,
        Shape::CodeFences,
        Shape::Alerts,
        Shape::Quotes,
        Shape::Footnotes,
        Shape::FrontMatter,
        Shape::Math,
        Shape::Mermaid,
        Shape::Images,
        Shape::KitchenSink,
        Shape::AdversarialToken,
        Shape::AdversarialLine,
        Shape::AdversarialTable,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Shape::PlainEn => "plain-en",
            Shape::PlainZh => "plain-zh",
            Shape::MixedZhEn => "mixed-zh-en",
            Shape::LongParagraph => "long-paragraph",
            Shape::LongParagraphZh => "long-paragraph-zh",
            Shape::LongParagraphMixed => "long-paragraph-mixed",
            Shape::NestedLists => "nested-lists",
            Shape::Tables => "tables",
            Shape::CodeFences => "code-fences",
            Shape::Alerts => "alerts",
            Shape::Quotes => "quotes",
            Shape::Footnotes => "footnotes",
            Shape::FrontMatter => "front-matter",
            Shape::Math => "math",
            Shape::Mermaid => "mermaid",
            Shape::Images => "images",
            Shape::KitchenSink => "kitchen-sink",
            Shape::AdversarialToken => "adversarial-token",
            Shape::AdversarialLine => "adversarial-line",
            Shape::AdversarialTable => "adversarial-table",
        }
    }

    fn salt(self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in self.name().bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

pub const SIZES: [usize; 4] = [1 << 10, 16 << 10, 256 << 10, 2 << 20];
pub const HUGE: usize = 16 << 20;

pub const BLOCK_COUNTS: [usize; 7] = [10, 100, 1_000, 10_000, 25_000, 50_000, 100_000];
pub const RENDER_FRAME_BLOCK_COUNTS: [usize; 3] = [10, 100, 1_000];
pub const BLOCK_SEED: u64 = 0x5EED_0A11;

pub fn sizes() -> Vec<usize> {
    let mut out = SIZES.to_vec();
    if std::env::var_os("MD_BENCH_HUGE").is_some() {
        out.push(HUGE);
    }
    out
}

pub fn human(bytes: usize) -> String {
    if bytes >= 1 << 20 {
        format!("{}m", bytes >> 20)
    } else if bytes >= 1 << 10 {
        format!("{}k", bytes >> 10)
    } else {
        format!("{bytes}")
    }
}

pub fn human_blocks(blocks: usize) -> String {
    if blocks >= 1_000 {
        format!("{}k", blocks / 1_000)
    } else {
        blocks.to_string()
    }
}

pub fn fingerprint(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() % bound as u64) as usize
    }

    pub fn range(&mut self, low: usize, high: usize) -> usize {
        if high <= low {
            return low;
        }
        low + self.below(high - low)
    }

    pub fn chance(&mut self, one_in: u64) -> bool {
        if one_in <= 1 {
            return true;
        }
        self.next_u64().is_multiple_of(one_in)
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const EN_WORDS: [&str; 64] = [
    "editor",
    "layout",
    "document",
    "paragraph",
    "render",
    "viewport",
    "caret",
    "line",
    "break",
    "wrap",
    "measure",
    "width",
    "height",
    "cache",
    "incremental",
    "shaping",
    "glyph",
    "font",
    "baseline",
    "margin",
    "padding",
    "indent",
    "quote",
    "list",
    "table",
    "cell",
    "header",
    "row",
    "column",
    "fence",
    "code",
    "span",
    "inline",
    "block",
    "tree",
    "arena",
    "node",
    "offset",
    "boundary",
    "grapheme",
    "cluster",
    "unicode",
    "segment",
    "algorithm",
    "dynamic",
    "programming",
    "optimal",
    "greedy",
    "ratio",
    "cost",
    "penalty",
    "badness",
    "scroll",
    "anchor",
    "snapshot",
    "frame",
    "paint",
    "scene",
    "clip",
    "shadow",
    "theme",
    "density",
    "outline",
    "status",
];

const ZH_WORDS: [&str; 48] = [
    "编辑器",
    "文档",
    "排版",
    "布局",
    "段落",
    "换行",
    "断行",
    "测量",
    "宽度",
    "高度",
    "缓存",
    "增量",
    "字形",
    "字体",
    "基线",
    "边距",
    "缩进",
    "引用",
    "列表",
    "表格",
    "单元格",
    "表头",
    "行",
    "列",
    "代码块",
    "行内",
    "块级",
    "语法树",
    "节点",
    "偏移",
    "字素",
    "断点",
    "算法",
    "最优",
    "贪心",
    "代价",
    "惩罚",
    "滚动",
    "锚点",
    "快照",
    "视口",
    "绘制",
    "场景",
    "裁剪",
    "阴影",
    "主题",
    "密度",
    "大纲",
];

const LANGS: [&str; 14] = [
    "rust",
    "python",
    "javascript",
    "typescript",
    "json",
    "toml",
    "bash",
    "go",
    "c",
    "cpp",
    "html",
    "css",
    "yaml",
    "markdown",
];

const ALERT_KINDS: [&str; 5] = ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];

const CODE_LINES: [&str; 12] = [
    "let value = compute(input);",
    "if value > threshold { emit(value); }",
    "return Ok(Default::default());",
    "for item in items.iter().take(limit) {}",
    "match kind { Kind::A => 1, _ => 2 }",
    "self.cache.insert(key, entry);",
    "fn measure(text: &str) -> f64 { 0.0 }",
    "total += weight * length as f64;",
    "assert_eq!(left, right, \"mismatch\");",
    "let Some(node) = arena.get(id) else { return };",
    "state = state.wrapping_add(step);",
    "push_str(&format!(\"{}:{}\", key, value));",
];

const MATH_INLINE: [&str; 6] = [
    "a^2 + b^2 = c^2",
    "\\frac{\\pi}{2}",
    "O(n \\log n)",
    "\\sum_{i=0}^{n} x_i",
    "\\alpha \\le \\beta",
    "\\int_0^1 f(x)\\,dx",
];

const MATH_DISPLAY: [&str; 4] = [
    "\\begin{aligned} a &= b + c \\\\ d &= e - f \\end{aligned}",
    "\\sum_{k=1}^{n} \\frac{1}{k^2} = \\frac{\\pi^2}{6}",
    "\\mathbf{M} = \\begin{pmatrix} 1 & 0 \\\\ 0 & 1 \\end{pmatrix}",
    "\\lim_{n \\to \\infty} \\left(1 + \\frac{1}{n}\\right)^n = e",
];

pub fn generate(shape: Shape, target_bytes: usize, seed: u64) -> String {
    let target = target_bytes.max(1);
    let mut rng = Rng::new(seed ^ shape.salt());
    match shape {
        Shape::PlainEn => grow(target, &mut rng, emit_paragraph),
        Shape::PlainZh => grow(target, &mut rng, emit_cjk_paragraph),
        Shape::MixedZhEn => grow(target, &mut rng, emit_mixed_paragraph),
        Shape::LongParagraph => long_paragraph(target, &mut rng),
        Shape::LongParagraphZh => long_paragraph_zh(target, &mut rng),
        Shape::LongParagraphMixed => long_paragraph_mixed(target, &mut rng),
        Shape::NestedLists => grow(target, &mut rng, emit_nested_list),
        Shape::Tables => grow(target, &mut rng, emit_table),
        Shape::CodeFences => grow(target, &mut rng, emit_code_fence),
        Shape::Alerts => grow(target, &mut rng, emit_alert),
        Shape::Quotes => grow(target, &mut rng, emit_quote),
        Shape::Footnotes => grow(target, &mut rng, emit_footnote),
        Shape::FrontMatter => front_matter(target, &mut rng),
        Shape::Math => grow(target, &mut rng, emit_math),
        Shape::Mermaid => grow(target, &mut rng, emit_mermaid),
        Shape::Images => grow(target, &mut rng, emit_image),
        Shape::KitchenSink => grow(target, &mut rng, emit_any),
        Shape::AdversarialToken => adversarial_token(target),
        Shape::AdversarialLine => adversarial_line(target, &mut rng),
        Shape::AdversarialTable => adversarial_table(target, &mut rng),
    }
}

pub fn generate_blocks(blocks: usize, seed: u64) -> String {
    let mut rng = Rng::new(seed ^ 0xB10C_5EED);
    let mut out = String::with_capacity(blocks.saturating_mul(96).saturating_add(256));
    for index in 0..blocks {
        match index % 12 {
            0 => emit_heading(&mut rng, &mut out),
            1 => emit_paragraph(&mut rng, &mut out),
            2 => emit_deep_list(&mut rng, &mut out),
            3 => emit_table(&mut rng, &mut out),
            4 => emit_code_fence(&mut rng, &mut out),
            5 => emit_deep_quote(&mut rng, &mut out),
            6 => emit_alert(&mut rng, &mut out),
            7 => emit_cjk_paragraph(&mut rng, &mut out),
            8 => emit_math(&mut rng, &mut out),
            9 => emit_mermaid(&mut rng, &mut out),
            10 => emit_image(&mut rng, &mut out),
            _ => emit_footnote(&mut rng, &mut out),
        }
    }
    out
}

fn grow(target: usize, rng: &mut Rng, mut emit: impl FnMut(&mut Rng, &mut String)) -> String {
    let mut out = String::with_capacity(target.saturating_add(256));
    let mut guard = 0usize;
    while out.len() < target {
        emit(rng, &mut out);
        guard += 1;
        if guard >= 200_000 {
            break;
        }
    }
    out
}

fn latin_run(rng: &mut Rng, out: &mut String, words: usize) {
    for index in 0..words {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(rng.pick(&EN_WORDS));
    }
}

fn cjk_run(rng: &mut Rng, out: &mut String, words: usize) {
    for index in 0..words {
        if index > 0 && index % 12 == 0 {
            out.push('。');
        }
        out.push_str(rng.pick(&ZH_WORDS));
    }
}

fn latin_prose(rng: &mut Rng, out: &mut String, words: usize) {
    let mut done = 0usize;
    while done < words {
        let run = rng.range(3, 12).min(words - done);
        if done > 0 {
            out.push(' ');
        }
        match rng.below(9) {
            0 => {
                out.push_str("**");
                latin_run(rng, out, run);
                out.push_str("**");
            }
            1 => {
                out.push('`');
                out.push_str(rng.pick(&EN_WORDS));
                out.push('`');
            }
            2 => {
                out.push('[');
                latin_run(rng, out, run);
                out.push_str("](https://example.com/spec)");
            }
            3 => {
                out.push('*');
                latin_run(rng, out, run);
                out.push('*');
            }
            _ => latin_run(rng, out, run),
        }
        done += run;
    }
}

fn emit_paragraph(rng: &mut Rng, out: &mut String) {
    let words = rng.range(18, 70);
    latin_prose(rng, out, words);
    out.push_str("\n\n");
}

fn emit_cjk_paragraph(rng: &mut Rng, out: &mut String) {
    let words = rng.range(20, 80);
    cjk_run(rng, out, words);
    out.push_str("\n\n");
}

fn emit_mixed_paragraph(rng: &mut Rng, out: &mut String) {
    let rounds = rng.range(2, 6);
    for round in 0..rounds {
        if round > 0 {
            out.push(' ');
        }
        if rng.chance(2) {
            let words = rng.range(4, 14);
            latin_run(rng, out, words);
        } else {
            let words = rng.range(6, 20);
            cjk_run(rng, out, words);
        }
    }
    out.push_str("\n\n");
}

fn long_paragraph(target: usize, rng: &mut Rng) -> String {
    let mut out = String::with_capacity(target.saturating_add(8));
    while out.len() < target {
        if !out.is_empty() {
            out.push(' ');
        }
        let words = rng.range(8, 24);
        latin_run(rng, &mut out, words);
    }
    out.push('\n');
    out
}

fn long_paragraph_zh(target: usize, rng: &mut Rng) -> String {
    let mut out = String::with_capacity(target.saturating_add(8));
    while out.len() < target {
        let words = rng.range(8, 24);
        cjk_run(rng, &mut out, words);
    }
    out.push('\n');
    out
}

fn long_paragraph_mixed(target: usize, rng: &mut Rng) -> String {
    let mut out = String::with_capacity(target.saturating_add(8));
    while out.len() < target {
        if !out.is_empty() {
            out.push(' ');
        }
        if rng.chance(2) {
            let words = rng.range(4, 14);
            latin_run(rng, &mut out, words);
        } else {
            let words = rng.range(6, 20);
            cjk_run(rng, &mut out, words);
        }
    }
    out.push('\n');
    out
}

fn emit_list_item(rng: &mut Rng, out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    match rng.below(4) {
        0 => out.push_str("1. "),
        1 => out.push_str("- [ ] "),
        2 => out.push_str("- [x] "),
        _ => out.push_str("- "),
    }
    let words = rng.range(3, 11);
    latin_run(rng, out, words);
    out.push('\n');
    if depth < 4 && rng.chance(2) {
        let kids = rng.range(1, 4);
        for _ in 0..kids {
            emit_list_item(rng, out, depth + 1);
        }
    }
}

fn emit_nested_list(rng: &mut Rng, out: &mut String) {
    let top = rng.range(2, 6);
    for _ in 0..top {
        emit_list_item(rng, out, 0);
    }
    out.push('\n');
}

fn emit_heading(rng: &mut Rng, out: &mut String) {
    let level = rng.range(1, 5);
    for _ in 0..level {
        out.push('#');
    }
    out.push(' ');
    let words = rng.range(2, 7);
    latin_run(rng, out, words);
    out.push_str("\n\n");
}

fn deep_list_item(rng: &mut Rng, out: &mut String, depth: usize, max_depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    match rng.below(4) {
        0 => out.push_str("1. "),
        1 => out.push_str("- [ ] "),
        2 => out.push_str("- [x] "),
        _ => out.push_str("- "),
    }
    let words = rng.range(3, 10);
    latin_run(rng, out, words);
    out.push('\n');
    if depth < max_depth && rng.chance(2) {
        let kids = rng.range(1, 3);
        for _ in 0..kids {
            deep_list_item(rng, out, depth + 1, max_depth);
        }
    }
}

fn emit_deep_list(rng: &mut Rng, out: &mut String) {
    let top = rng.range(1, 3);
    for _ in 0..top {
        deep_list_item(rng, out, 0, 7);
    }
    out.push('\n');
}

fn emit_deep_quote(rng: &mut Rng, out: &mut String) {
    let depth = rng.range(2, 6);
    let mut prefix = String::new();
    for _ in 0..depth {
        prefix.push_str("> ");
    }
    let lines = rng.range(2, 6);
    for _ in 0..lines {
        out.push_str(&prefix);
        let words = rng.range(4, 12);
        latin_run(rng, out, words);
        out.push('\n');
    }
    out.push('\n');
}

fn emit_table_sized(rng: &mut Rng, out: &mut String, rows: usize, cols: usize) {
    out.push('|');
    for col in 0..cols {
        out.push_str(" h");
        out.push_str(&col.to_string());
        out.push_str(" |");
    }
    out.push('\n');
    out.push('|');
    for _ in 0..cols {
        match rng.below(4) {
            0 => out.push_str(":--|"),
            1 => out.push_str("--:|"),
            2 => out.push_str(":-:|"),
            _ => out.push_str("---|"),
        }
    }
    out.push('\n');
    for row in 0..rows {
        out.push('|');
        for _ in 0..cols {
            out.push(' ');
            out.push_str(rng.pick(&EN_WORDS));
            out.push_str(&row.to_string());
            out.push_str(" |");
        }
        out.push('\n');
    }
}

fn emit_table(rng: &mut Rng, out: &mut String) {
    let rows = rng.range(2, 20);
    let cols = rng.range(2, 9);
    emit_table_sized(rng, out, rows, cols);
    out.push('\n');
}

fn emit_code_fence(rng: &mut Rng, out: &mut String) {
    out.push_str("```");
    out.push_str(rng.pick(&LANGS));
    out.push('\n');
    let lines = rng.range(3, 16);
    for _ in 0..lines {
        out.push_str(rng.pick(&CODE_LINES));
        out.push('\n');
    }
    out.push_str("```\n\n");
}

fn emit_alert(rng: &mut Rng, out: &mut String) {
    out.push_str("> [!");
    out.push_str(rng.pick(&ALERT_KINDS));
    out.push_str("]\n");
    if !rng.chance(6) {
        let lines = rng.range(1, 5);
        for _ in 0..lines {
            out.push_str("> ");
            let words = rng.range(4, 16);
            latin_run(rng, out, words);
            out.push('\n');
        }
    }
    out.push('\n');
}

fn emit_quote(rng: &mut Rng, out: &mut String) {
    let depth = rng.range(1, 4);
    let mut prefix = String::new();
    for _ in 0..depth {
        prefix.push_str("> ");
    }
    let lines = rng.range(2, 8);
    for _ in 0..lines {
        out.push_str(&prefix);
        let words = rng.range(4, 14);
        latin_run(rng, out, words);
        out.push('\n');
    }
    out.push('\n');
}

fn emit_footnote(rng: &mut Rng, out: &mut String) {
    let id = rng.range(1, 10_000);
    let label = id.to_string();
    out.push_str("See note[^");
    out.push_str(&label);
    out.push_str("] for detail.\n\n[^");
    out.push_str(&label);
    out.push_str("]: ");
    let words = rng.range(6, 20);
    latin_run(rng, out, words);
    out.push_str("\n\n");
}

fn front_matter(target: usize, rng: &mut Rng) -> String {
    let mut out = String::from(
        "---\ntitle: bench corpus\nauthor: bench\ntags:\n  - bench\n  - markdown\n---\n\n",
    );
    let rest = target.saturating_sub(out.len());
    out.push_str(&grow(rest, rng, emit_paragraph));
    out
}

fn emit_math(rng: &mut Rng, out: &mut String) {
    out.push_str("Inline $");
    out.push_str(rng.pick(&MATH_INLINE));
    out.push_str("$ follows a display block:\n\n$$\n");
    out.push_str(rng.pick(&MATH_DISPLAY));
    out.push_str("\n$$\n\n");
}

fn emit_mermaid(rng: &mut Rng, out: &mut String) {
    out.push_str("```mermaid\ngraph TD;\n");
    let nodes = rng.range(3, 12);
    for index in 0..nodes {
        out.push_str("    N");
        out.push_str(&index.to_string());
        out.push_str("-->N");
        out.push_str(&(index + 1).to_string());
        out.push_str(";\n");
    }
    out.push_str("```\n\n");
}

fn emit_image(rng: &mut Rng, out: &mut String) {
    match rng.below(3) {
        0 => out.push_str("![diagram](assets/diagram.png)\n\n"),
        1 => out.push_str("A paragraph with ![inline](assets/icon.png) in the middle.\n\n"),
        _ => out.push_str("<img src=\"assets/photo.png\" alt=\"photo\" width=\"320\">\n\n"),
    }
}

fn emit_any(rng: &mut Rng, out: &mut String) {
    match rng.below(12) {
        0 => emit_nested_list(rng, out),
        1 => emit_table(rng, out),
        2 => emit_code_fence(rng, out),
        3 => emit_alert(rng, out),
        4 => emit_quote(rng, out),
        5 => emit_footnote(rng, out),
        6 => emit_math(rng, out),
        7 => emit_mermaid(rng, out),
        8 => emit_image(rng, out),
        9 => emit_cjk_paragraph(rng, out),
        10 => emit_mixed_paragraph(rng, out),
        _ => emit_paragraph(rng, out),
    }
}

fn adversarial_token(target: usize) -> String {
    let mut out = String::with_capacity(target.saturating_add(1));
    for _ in 0..target {
        out.push('a');
    }
    out.push('\n');
    out
}

fn adversarial_line(target: usize, rng: &mut Rng) -> String {
    let mut out = String::with_capacity(target.saturating_add(2));
    while out.len() < target {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(rng.pick(&EN_WORDS));
    }
    out.push('\n');
    out
}

fn adversarial_table(target: usize, rng: &mut Rng) -> String {
    let cols = 6;
    let rows = (target / 48).max(1);
    let mut out = String::with_capacity(target.saturating_add(512));
    emit_table_sized(rng, &mut out, rows, cols);
    out
}

pub fn table(rows: usize, cols: usize, seed: u64) -> String {
    let mut rng = Rng::new(seed ^ 0x7ab1_e5eed);
    let mut out = String::new();
    emit_table_sized(&mut rng, &mut out, rows.max(1), cols.max(1));
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaStyle {
    Photo,
    Graphic,
}

#[derive(Clone, Copy, Debug)]
pub struct MediaImage {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub style: MediaStyle,
}

pub const MEDIA_IMAGES: [MediaImage; 12] = [
    MediaImage {
        name: "img01",
        width: 320,
        height: 240,
        style: MediaStyle::Graphic,
    },
    MediaImage {
        name: "img02",
        width: 480,
        height: 360,
        style: MediaStyle::Graphic,
    },
    MediaImage {
        name: "img03",
        width: 640,
        height: 480,
        style: MediaStyle::Photo,
    },
    MediaImage {
        name: "img04",
        width: 800,
        height: 600,
        style: MediaStyle::Graphic,
    },
    MediaImage {
        name: "img05",
        width: 1024,
        height: 768,
        style: MediaStyle::Photo,
    },
    MediaImage {
        name: "img06",
        width: 1280,
        height: 720,
        style: MediaStyle::Graphic,
    },
    MediaImage {
        name: "img07",
        width: 1600,
        height: 1200,
        style: MediaStyle::Photo,
    },
    MediaImage {
        name: "img08",
        width: 1920,
        height: 1080,
        style: MediaStyle::Graphic,
    },
    MediaImage {
        name: "img09",
        width: 1000,
        height: 1000,
        style: MediaStyle::Photo,
    },
    MediaImage {
        name: "img10",
        width: 600,
        height: 1400,
        style: MediaStyle::Graphic,
    },
    MediaImage {
        name: "img11",
        width: 1400,
        height: 600,
        style: MediaStyle::Photo,
    },
    MediaImage {
        name: "img12",
        width: 512,
        height: 512,
        style: MediaStyle::Photo,
    },
];

pub const GIANT_IMAGE: MediaImage = MediaImage {
    name: "giant",
    width: 4096,
    height: 4096,
    style: MediaStyle::Graphic,
};

/// Four distinct oversized sources on one page. They decode to 16 MiB each and
/// each resize workspace is larger still, so a page holding all four is the
/// case that tells whether concurrent decodes are bounded: one at a time keeps
/// the peak near a single job, four at a time multiplies it.
pub const MEDIA_LARGE: [MediaImage; 4] = [
    MediaImage {
        name: "large01",
        width: 2048,
        height: 2048,
        style: MediaStyle::Graphic,
    },
    MediaImage {
        name: "large02",
        width: 2048,
        height: 2048,
        style: MediaStyle::Graphic,
    },
    MediaImage {
        name: "large03",
        width: 2048,
        height: 2048,
        style: MediaStyle::Graphic,
    },
    MediaImage {
        name: "large04",
        width: 2048,
        height: 2048,
        style: MediaStyle::Graphic,
    },
];

pub const MEDIA_SEED: u64 = 0x5EED_11ED;

pub fn media_image_specs() -> Vec<MediaImage> {
    MEDIA_IMAGES.to_vec()
}

fn blend(from: [u8; 4], to: [u8; 4], t: f32) -> [u8; 4] {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    [
        mix(from[0], to[0]),
        mix(from[1], to[1]),
        mix(from[2], to[2]),
        255,
    ]
}

fn fill_rect(img: &mut image::RgbaImage, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
    let (iw, ih) = img.dimensions();
    for py in y..y.saturating_add(h).min(ih) {
        for px in x..x.saturating_add(w).min(iw) {
            img.put_pixel(px, py, image::Rgba(color));
        }
    }
}

fn paint_gradient(img: &mut image::RgbaImage, rng: &mut Rng) {
    let (w, h) = img.dimensions();
    let top = [
        rng.below(200) as u8,
        rng.below(200) as u8,
        rng.below(200) as u8,
        255,
    ];
    let bottom = [
        rng.below(200) as u8,
        rng.below(200) as u8,
        rng.below(200) as u8,
        255,
    ];
    for y in 0..h {
        let t = y as f32 / (h.max(2) - 1) as f32;
        let color = blend(top, bottom, t);
        for x in 0..w {
            img.put_pixel(x, y, image::Rgba(color));
        }
    }
}

fn paint_graphic(img: &mut image::RgbaImage, rng: &mut Rng) {
    let (w, h) = img.dimensions();
    paint_gradient(img, rng);
    let blocks = rng.range(4, 10);
    for _ in 0..blocks {
        let bw = rng.range(20, (w as usize / 2).max(21)) as u32;
        let bh = rng.range(16, (h as usize / 2).max(17)) as u32;
        let x = rng.range(0, w.saturating_sub(bw).max(1) as usize) as u32;
        let y = rng.range(0, h.saturating_sub(bh).max(1) as usize) as u32;
        let color = [
            rng.below(256) as u8,
            rng.below(256) as u8,
            rng.below(256) as u8,
            255,
        ];
        fill_rect(img, x, y, bw, bh, color);
    }
    let lines = rng.range(2, 6);
    for _ in 0..lines {
        let y = rng.range(0, h.max(2) as usize) as u32;
        let thickness = rng.range(2, 6) as u32;
        let color = [
            rng.below(256) as u8,
            rng.below(256) as u8,
            rng.below(256) as u8,
            255,
        ];
        fill_rect(img, 0, y, w, thickness, color);
    }
}

fn paint_photo(img: &mut image::RgbaImage, rng: &mut Rng) {
    let (w, h) = img.dimensions();
    paint_gradient(img, rng);
    let noise: [i16; 256] = {
        let mut table = [0i16; 256];
        for slot in table.iter_mut() {
            *slot = rng.below(49) as i16 - 24;
        }
        table
    };
    for y in 0..h {
        for x in 0..w {
            let n = noise[(x as usize * 7 + y as usize * 13) % 256];
            let pixel = img.get_pixel_mut(x, y);
            for channel in 0..3 {
                let value = i16::from(pixel.0[channel]) + n;
                pixel.0[channel] = value.clamp(0, 255) as u8;
            }
        }
    }
}

pub fn media_pixels(spec: &MediaImage, seed: u64) -> image::RgbaImage {
    media_pixels_sized(spec.width, spec.height, spec.style, seed)
}

pub fn media_pixels_sized(
    width: u32,
    height: u32,
    style: MediaStyle,
    seed: u64,
) -> image::RgbaImage {
    let salt = u64::from(width) ^ (u64::from(height) << 32);
    let mut rng = Rng::new(seed ^ salt);
    let mut img = image::RgbaImage::new(width, height);
    match style {
        MediaStyle::Graphic => paint_graphic(&mut img, &mut rng),
        MediaStyle::Photo => paint_photo(&mut img, &mut rng),
    }
    img
}

pub fn encode_png(img: &image::RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("the media corpus image must encode as png");
    out
}

pub const MEDIA_MERMAID: [&str; 12] = [
    "flowchart TD\n  A[Start] --> B{Parse}\n  B -->|ok| C[Layout]\n  B -->|err| D[Report]\n  C --> E[Paint]\n  E --> F[Done]\n  D --> F",
    "flowchart LR\n  A[Read] --> B[Shape]\n  A --> C[Split]\n  B --> D[Merge]\n  C --> D\n  D --> E[Emit]",
    "sequenceDiagram\n  participant U as User\n  participant E as Editor\n  participant C as Cache\n  U->>E: keystroke\n  E->>C: lookup\n  C-->>E: hit\n  E-->>U: frame\n  U->>E: scroll\n  E->>C: prefetch\n  C-->>E: miss\n  E-->>U: frame",
    "classDiagram\n  class Doc { +blocks +text }\n  class Block { +kind +span }\n  class Cache { +get() +put() }\n  Doc --> Block\n  Doc --> Cache\n  Block --> Cache",
    "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Loading: open\n  Loading --> Ready: decoded\n  Loading --> Failed: error\n  Ready --> Idle: close\n  Failed --> Idle: dismiss",
    "erDiagram\n  DOC ||--o{ BLOCK : has\n  BLOCK ||--o{ SPAN : has\n  DOC ||--o{ SOURCE : owns\n  SOURCE ||--o{ PIXEL : decodes",
    "gantt\n  title Release\n  dateFormat YYYY-MM-DD\n  section Core\n  Parse :a1, 2026-01-01, 10d\n  Layout :a2, after a1, 14d\n  section Media\n  Decode :b1, 2026-01-05, 8d\n  Raster :b2, after b1, 6d",
    "pie title Cache bytes\n  \"display\" : 48\n  \"source\" : 12\n  \"mermaid\" : 30\n  \"math\" : 4",
    "gitGraph\n  commit id: \"a1\"\n  commit id: \"b2\"\n  branch fix\n  commit id: \"c3\"\n  checkout main\n  commit id: \"d4\"\n  merge fix",
    "journey\n  title Scroll pass\n  section Enter\n    Layout: 5: Editor\n    Shape: 3: Editor\n  section Exit\n    Evict: 2: Cache\n    Drop: 4: Cache",
    "mindmap\n  root((media))\n    images\n      source\n      display\n      gpu\n    mermaid\n      source\n      raster\n    math\n      metrics",
    "timeline\n  title Media work\n  2026-10 : decode\n  2026-11 : raster\n  2026-12 : budget",
];

/// A second set of diagrams, one of each syntax kind the set above already
/// proves renders. The raster cache is keyed on the source hash, so repeating a
/// diagram does not add an entry: a document that has to hold more diagrams
/// than the slice allows needs more sources, not more copies.
pub const MEDIA_MERMAID_EXTRA: [&str; 12] = [
    "flowchart TD\n  A[Open] --> B[Parse]\n  B --> C[Compose]\n  C --> D[Layout]\n  D --> E[Snapshot]\n  E --> F[Paint]\n  F --> G{More?}\n  G -->|yes| D\n  G -->|no| H[Idle]",
    "flowchart LR\n  A[Input] --> B{Kind}\n  B -->|text| C[Shape]\n  B -->|cell| D[Grid]\n  B -->|media| E[Load]\n  C --> F[Scene]\n  D --> F\n  E --> F",
    "sequenceDiagram\n  participant P as Paint\n  participant U as Usage\n  participant Q as Queue\n  P->>U: note frame\n  U->>Q: release?\n  Q-->>U: pending\n  P->>U: commit\n  U->>Q: sweep\n  Q-->>P: dropped\n  U-->>P: kept",
    "classDiagram\n  class Budget { +job +resident +split() }\n  class Permit { +bytes +lease() }\n  class Lease { +id +release() }\n  Budget --> Permit\n  Permit --> Lease",
    "stateDiagram-v2\n  [*] --> Warm\n  Warm --> Thawing: visible\n  Thawing --> Ready: inflated\n  Thawing --> Warm: cancelled\n  Ready --> Warm: left screen\n  Ready --> [*]: evicted",
    "erDiagram\n  WINDOW ||--o{ FRAME : paints\n  FRAME ||--o{ USAGE : notes\n  USAGE ||--o{ ALLOC : holds\n  ALLOC ||--o| LEASE : charges",
    "gantt\n  title Migration\n  dateFormat YYYY-MM-DD\n  section Read\n  Survey :r1, 2026-02-01, 6d\n  Prototype :r2, after r1, 9d\n  section Write\n  Port :w1, after r2, 12d\n  Verify :w2, after w1, 5d",
    "pie title Job budget\n  \"source\" : 22\n  \"display\" : 41\n  \"zoom\" : 9\n  \"mermaid\" : 18\n  \"math\" : 6",
    "gitGraph\n  commit id: \"e1\"\n  branch warm\n  commit id: \"f2\"\n  commit id: \"g3\"\n  checkout main\n  commit id: \"h4\"\n  merge warm\n  commit id: \"i5\"",
    "journey\n  title Open a document\n  section Read\n    Resolve: 4: App\n    Decode: 2: Cache\n  section Show\n    Raster: 3: App\n    Paint: 5: App",
    "mindmap\n  root((budget))\n    jobs\n      source\n      display\n      thaw\n    resident\n      hot\n      warm\n      leases",
    "timeline\n  title Cache states\n  Hot : pixels\n  Warm : bytes\n  Thawing : decode\n  Ready : pixels",
];

pub fn media_mermaid_long_doc() -> String {
    let mut all: Vec<&str> = MEDIA_MERMAID.to_vec();
    all.extend(MEDIA_MERMAID_EXTRA);
    media_mermaid_doc(&all)
}

pub fn media_images_doc(specs: &[MediaImage], per_image_text: bool) -> String {
    let mut out = String::with_capacity(specs.len() * 256);
    for (index, spec) in specs.iter().enumerate() {
        out.push_str("## Figure ");
        out.push_str(&(index + 1).to_string());
        out.push_str("\n\n");
        if per_image_text {
            out.push_str(
                "A paragraph of body text ahead of the figure so the block does not sit flush \
                 against the viewport edge while scrolling.\n\n",
            );
        }
        out.push_str("![");
        out.push_str(spec.name);
        out.push_str("](");
        out.push_str(spec.name);
        out.push_str(".png)\n\n");
        if per_image_text {
            out.push_str(
                "A paragraph of body text after the figure, long enough that the next heading \
                 lands below the fold.\n\n",
            );
        }
    }
    out
}

pub fn media_mermaid_doc(sources: &[&str]) -> String {
    let mut out = String::with_capacity(sources.len() * 512);
    for (index, source) in sources.iter().enumerate() {
        out.push_str("## Diagram ");
        out.push_str(&(index + 1).to_string());
        out.push_str("\n\nA short line of prose before the diagram.\n\n```mermaid\n");
        out.push_str(source);
        out.push_str("\n```\n\nA short line of prose after the diagram.\n\n");
    }
    out
}

pub fn media_mermaid_repeat_doc(sources: &[&str], repeat: usize) -> String {
    let mut out = String::with_capacity(sources.len() * repeat * 512);
    for round in 0..repeat {
        for (index, source) in sources.iter().enumerate() {
            out.push_str("## Round ");
            out.push_str(&round.to_string());
            out.push_str(" Diagram ");
            out.push_str(&(index + 1).to_string());
            out.push_str("\n\nA short line of prose before the diagram.\n\n```mermaid\n");
            out.push_str(source);
            out.push_str("\n```\n\nA short line of prose after the diagram.\n\n");
        }
    }
    out
}

pub fn media_text_doc(count: usize) -> String {
    let mut out = String::with_capacity(count * 512);
    for index in 0..count {
        out.push_str("## Figure ");
        out.push_str(&(index + 1).to_string());
        out.push_str("\n\n");
        out.push_str(
            "A paragraph of body text ahead of the figure so the block does not sit flush \
             against the viewport edge while scrolling.\n\n",
        );
        out.push_str(
            "A paragraph of body text after the figure, long enough that the next heading \
             lands below the fold.\n\n",
        );
    }
    out
}

/// Geometry of every gallery source. The twelve curated shapes exist to compare
/// geometries; the gallery exists to grow the number of sources, so it holds the
/// geometry fixed and varies only the pixels.
pub const GALLERY_WIDTH: u32 = 240;
pub const GALLERY_HEIGHT: u32 = 180;

/// Name, geometry, style and seed of the `index`-th gallery source.
///
/// The seed is per index on purpose. Distinct file names alone are enough for
/// the caches, which key on the path, but identical bytes would let any
/// content-addressed layer collapse the files and a document of twelve hundred
/// pictures would quietly measure one picture twelve hundred times. The style
/// is graphic rather than photographic so the corpus stays small enough to
/// regenerate; the photo sources in `MEDIA_IMAGES` cover the codec difference.
pub fn media_gallery_entry(index: usize) -> (String, u32, u32, MediaStyle, u64) {
    (
        format!("gallery{index:04}"),
        GALLERY_WIDTH,
        GALLERY_HEIGHT,
        MediaStyle::Graphic,
        MEDIA_SEED ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
    )
}

pub fn media_gallery_doc(count: usize) -> String {
    let mut out = String::with_capacity(count * 384);
    for index in 0..count {
        let (name, _, _, _, _) = media_gallery_entry(index);
        out.push_str("## Figure ");
        out.push_str(&(index + 1).to_string());
        out.push_str("\n\nA paragraph of body text ahead of the figure so the block does not sit flush against the viewport edge.\n\n![");
        out.push_str(&name);
        out.push_str("](");
        out.push_str(&name);
        out.push_str(".png)\n\nA paragraph of body text after the figure, long enough that the next heading lands below the fold.\n\n");
    }
    out
}

/// One round per screen: an image, a formula and a diagram interleaved with
/// prose. The three caches are hot at the same time, which is the only way to
/// see whether a per-cache slice that looks fine on its own adds up to
/// something a window should not hold.
pub fn media_mixed_doc(rounds: usize) -> String {
    let mut out = String::with_capacity(rounds * 1024);
    for index in 0..rounds {
        let spec = MEDIA_IMAGES[index % MEDIA_IMAGES.len()];
        out.push_str("## Round ");
        out.push_str(&(index + 1).to_string());
        out.push_str("\n\nA paragraph of body text ahead of the figure so the block does not sit flush against the viewport edge.\n\n![");
        out.push_str(spec.name);
        out.push_str("](");
        out.push_str(spec.name);
        out.push_str(".png)\n\n");
        out.push_str("Inline $");
        out.push_str(MATH_INLINE[index % MATH_INLINE.len()]);
        out.push_str("$ follows a display block:\n\n$$\n");
        out.push_str(MATH_DISPLAY[index % MATH_DISPLAY.len()]);
        out.push_str("\n$$\n\n```mermaid\n");
        out.push_str(MEDIA_MERMAID[index % MEDIA_MERMAID.len()]);
        out.push_str("\n```\n\nA short line of prose after the diagram.\n\n");
    }
    out
}

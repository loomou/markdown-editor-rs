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

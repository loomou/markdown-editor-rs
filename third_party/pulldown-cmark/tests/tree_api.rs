//! tree API 与事件流的一致性(feature `tree`,见仓库 docs
//! `tree-api-03-fork-impl.md` §6)。
//!
//! 核心保证:`Parsed` 走树得到的结构、span 与 `Parser` 事件流逐一对齐
//! ——这是 `resolve_all_inlines` / `is_container_body` 正确性的唯一防线。
//! 夹具:上游全部 spec 文件(CommonMark / GFM / specs)× 三组 options,
//! 外加边界清单与混合生成器。

#![cfg(feature = "tree")]

use pulldown_cmark::{Event, NodeKind, Options, Parsed, Parser, Tag, TagEnd};
use std::fmt::Write;
use std::path::PathBuf;

// ── 夹具 ────────────────────────────────────────────────

fn spec_files() -> Vec<(String, String)> {
    let mut paths = Vec::new();
    // 测试进程的 cwd 不保证是 crate 根,用编译期路径拼。
    // CommonMark 下两份(spec + smart_punct)、GitHub 下三份、specs 下
    // 15 份(specs/definition_lists.txt 等即使 md-test 侧不开该 option,
    // 树侧也必须覆盖它的变体)。
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for dir in ["third_party/CommonMark", "third_party/GitHub", "specs"] {
        let entries = std::fs::read_dir(base.join(dir))
            .unwrap_or_else(|e| panic!("missing spec dir {dir}: {e}"));
        for e in entries.flatten() {
            if e.path().extension().is_some_and(|x| x == "txt") {
                paths.push(e.path());
            }
        }
    }
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, text)
        })
        .collect()
}

fn option_sets() -> Vec<(&'static str, Options)> {
    let mut editor = Options::empty();
    editor.insert(Options::ENABLE_TABLES);
    editor.insert(Options::ENABLE_FOOTNOTES);
    editor.insert(Options::ENABLE_STRIKETHROUGH);
    editor.insert(Options::ENABLE_TASKLISTS);
    editor.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    editor.insert(Options::ENABLE_SUPERSCRIPT);
    editor.insert(Options::ENABLE_SUBSCRIPT);
    editor.insert(Options::ENABLE_MATH);
    editor.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    editor.insert(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);
    vec![
        ("all", Options::all()),
        ("empty", Options::empty()),
        ("editor", editor),
    ]
}

/// md-test `docs/tree-api-03-fork-impl.md` §7 边界清单。
const BOUNDARY: &[&str] = &[
    "- one\n- two\n",
    "- one\n\n- two\n",
    "> > a\n> > b\n",
    "> - a `x\n>     y` z\n",
    "| a | b |\n| --- | --- |\n| c \\| d | e |\n",
    "<div>\n\ntext\n",
    "[a]: url\n\n[a]\n",
    "text[^a]\n\n[^a]: n\n",
    "$$x$$\n",
    "$x$\n",
    "**a\nb**\n",
    "a \\*not em\\* b\n",
    "a &amp; b\n",
    "Title\n=====\n",
    "",
    "\n\n\n",
];

fn deep_quote(depth: usize) -> String {
    format!("{}a\n", "> ".repeat(depth))
}

/// 混合构造生成器(精简版 md-test `unique_mixed`):每轮 14 种构造
/// 各一段,把全部块级与行内路径搅在一起。
fn mixed(n: usize) -> String {
    let mut out = String::new();
    for i in 0..n {
        let body = match i % 14 {
            0 => format!("# Head {i} *em* `c` {{ #h{i} .x k=v }}\n\n"),
            1 => format!("para {i} *em* **b** ~~s~~ `c` [l](https://x/{i}) ![a](i.png)\n\n"),
            2 => format!("> > deep {i}\n> >\n> > - list `c`\n> > - $x_{i}$\n\n"),
            3 => format!("```rust\nfn f{i}() {{}}\n```\n\n"),
            4 => format!("$$\\int_{i}$$\n\n"),
            5 => format!("- item {i} *em*\n- second $q_{i}$\n\n"),
            6 => format!("1. ordered {i}\n2. second ~~z~~\n\n"),
            7 => format!("- [ ] task {i}\n- [x] done\n\n"),
            8 => format!("| *A{i}* | $B$ |\n| --- | --- |\n| `C` | d |\n\n"),
            9 => format!("<div>html-{i}</div>\n\n"),
            10 => format!("Term {i}\n: Def {i:x}\n\n"),
            11 => format!("> [!NOTE]\n> alert {i} *em*\n\n"),
            12 => format!("+++\nid: {i}\n+++\n\n"),
            _ => "***\n\n".to_string(),
        };
        out.push_str(&body);
        out.push_str(&format!("[^f{i}]: note {i}\n"));
    }
    out
}

// ── 事件侧形状串 ────────────────────────────────────────

fn tag_name(tag: &Tag<'_>) -> String {
    match tag {
        Tag::Paragraph => "Paragraph".into(),
        Tag::Heading { level, .. } => format!("Heading({level:?})"),
        Tag::BlockQuote(_) => "BlockQuote".into(),
        Tag::CodeBlock(_) => "CodeBlock".into(),
        Tag::List(_) => "List".into(),
        Tag::Item => "Item".into(),
        Tag::FootnoteDefinition(_) => "FootnoteDefinition".into(),
        Tag::DefinitionList => "DefinitionList".into(),
        Tag::DefinitionListTitle => "DefinitionListTitle".into(),
        Tag::DefinitionListDefinition => "DefinitionListDefinition".into(),
        Tag::Table(_) => "Table".into(),
        Tag::TableHead => "TableHead".into(),
        Tag::TableRow => "TableRow".into(),
        Tag::TableCell => "TableCell".into(),
        Tag::Emphasis => "Emphasis".into(),
        Tag::Strong => "Strong".into(),
        Tag::Strikethrough => "Strikethrough".into(),
        Tag::Superscript => "Superscript".into(),
        Tag::Subscript => "Subscript".into(),
        Tag::Link { .. } => "Link".into(),
        Tag::Image { .. } => "Image".into(),
        Tag::HtmlBlock => "HtmlBlock".into(),
        Tag::MetadataBlock(_) => "MetadataBlock".into(),
    }
}

fn event_name(event: &Event<'_>) -> String {
    match event {
        Event::Start(tag) => format!("Start({})", tag_name(tag)),
        Event::End(end) => format!("End({})", end_name(end)),
        Event::Text(_) => "Text".into(),
        Event::Code(_) => "Code".into(),
        Event::InlineMath(_) => "InlineMath".into(),
        Event::DisplayMath(_) => "DisplayMath".into(),
        Event::Html(_) => "Html".into(),
        Event::InlineHtml(_) => "InlineHtml".into(),
        Event::FootnoteReference(_) => "FootnoteReference".into(),
        Event::TaskListMarker(_) => "TaskListMarker".into(),
        Event::SoftBreak => "SoftBreak".into(),
        Event::HardBreak => "HardBreak".into(),
        Event::Rule => "Rule".into(),
    }
}

fn end_name(end: &TagEnd) -> String {
    match end {
        TagEnd::Heading(level) => format!("Heading({level:?})"),
        TagEnd::List(_) => "List".into(),
        TagEnd::BlockQuote(_) => "BlockQuote".into(),
        TagEnd::MetadataBlock(_) => "MetadataBlock".into(),
        _ => format!("{end:?}"),
    }
}

fn shape_via_events(src: &str, opts: Options) -> String {
    let mut out = String::new();
    for (event, range) in Parser::new_ext(src, opts).into_offset_iter() {
        let _ = writeln!(out, "{} {}..{}", event_name(&event), range.start, range.end);
    }
    out
}

// ── 树侧形状串 ──────────────────────────────────────────

fn shape_via_tree(src: &str, opts: Options) -> String {
    let parsed = Parsed::new(src, opts);
    let mut out = String::new();
    walk_shape(parsed.root(), &mut out);
    out
}

fn kind_name(kind: &NodeKind<'_, '_>) -> String {
    match kind {
        NodeKind::Root => "Root".into(),
        NodeKind::BlockQuote(_) => "BlockQuote".into(),
        NodeKind::List(_) => "List".into(),
        NodeKind::ListItem(_) => "Item".into(),
        NodeKind::FootnoteDefinition(_) => "FootnoteDefinition".into(),
        NodeKind::Table(_) => "Table".into(),
        NodeKind::TableHead => "TableHead".into(),
        NodeKind::TableRow => "TableRow".into(),
        NodeKind::TableCell => "TableCell".into(),
        NodeKind::DefinitionList { .. } => "DefinitionList".into(),
        NodeKind::DefinitionListTitle => "DefinitionListTitle".into(),
        NodeKind::DefinitionListDefinition { .. } => "DefinitionListDefinition".into(),
        NodeKind::Paragraph => "Paragraph".into(),
        NodeKind::TightParagraph => "TightParagraph".into(),
        NodeKind::Heading(info) => format!("Heading({:?})", info.level),
        NodeKind::CodeBlock(_) => "CodeBlock".into(),
        NodeKind::HtmlBlock => "HtmlBlock".into(),
        NodeKind::MetadataBlock(_) => "MetadataBlock".into(),
        NodeKind::Rule => "Rule".into(),
        NodeKind::Emphasis => "Emphasis".into(),
        NodeKind::Strong => "Strong".into(),
        NodeKind::Strikethrough => "Strikethrough".into(),
        NodeKind::Superscript => "Superscript".into(),
        NodeKind::Subscript => "Subscript".into(),
        NodeKind::Link(_) => "Link".into(),
        NodeKind::Image(_) => "Image".into(),
        NodeKind::Text { .. } => "Text".into(),
        NodeKind::TextOwned => "Text".into(),
        NodeKind::SynthesizedChar(_) => "Text".into(),
        NodeKind::Code(_) => "Code".into(),
        NodeKind::Math { display: false, .. } => "InlineMath".into(),
        NodeKind::Math { display: true, .. } => "DisplayMath".into(),
        NodeKind::InlineHtml => "InlineHtml".into(),
        NodeKind::Html => "Html".into(),
        NodeKind::FootnoteReference(_) => "FootnoteReference".into(),
        NodeKind::TaskMarker { .. } => "TaskListMarker".into(),
        NodeKind::SoftBreak => "SoftBreak".into(),
        NodeKind::HardBreak { .. } => "HardBreak".into(),
    }
}

fn is_tree_container(kind: &NodeKind<'_, '_>) -> bool {
    matches!(
        kind,
        NodeKind::BlockQuote(_)
            | NodeKind::List(_)
            | NodeKind::ListItem(_)
            | NodeKind::FootnoteDefinition(_)
            | NodeKind::Table(_)
            | NodeKind::TableHead
            | NodeKind::TableRow
            | NodeKind::TableCell
            | NodeKind::DefinitionList { .. }
            | NodeKind::DefinitionListTitle
            | NodeKind::DefinitionListDefinition { .. }
            | NodeKind::Paragraph
            | NodeKind::Heading(_)
            | NodeKind::CodeBlock(_)
            | NodeKind::HtmlBlock
            | NodeKind::MetadataBlock(_)
            | NodeKind::Emphasis
            | NodeKind::Strong
            | NodeKind::Strikethrough
            | NodeKind::Superscript
            | NodeKind::Subscript
            | NodeKind::Link(_)
            | NodeKind::Image(_)
    )
}

fn walk_shape(node: pulldown_cmark::NodeRef<'_, '_>, out: &mut String) {
    let kind = node.kind();
    match &kind {
        // 事件层隐形:只下钻,不产行。
        NodeKind::Root | NodeKind::TightParagraph => {
            for child in node.children() {
                walk_shape(child, out);
            }
        }
        k if is_tree_container(k) => {
            let span = node.span();
            let _ = writeln!(out, "Start({}) {}..{}", kind_name(k), span.start, span.end);
            for child in node.children() {
                walk_shape(child, out);
            }
            let _ = writeln!(out, "End({}) {}..{}", kind_name(k), span.start, span.end);
        }
        k => {
            let span = node.span();
            let _ = writeln!(out, "{} {}..{}", kind_name(k), span.start, span.end);
        }
    }
}

// ── 测试 ────────────────────────────────────────────────

fn assert_shapes_match(src: &str, opts: Options, ctx: &str) {
    let events = shape_via_events(src, opts);
    let tree = shape_via_tree(src, opts);
    if events != tree {
        // 首个差异行打出来,别把整份形状串糊进断言。
        let mut detail = String::new();
        for (i, (a, b)) in events.lines().zip(tree.lines()).enumerate() {
            if a != b {
                let _ = writeln!(detail, "line {}: events={a:?} tree={b:?}", i + 1);
                break;
            }
        }
        if detail.is_empty() {
            let _ = writeln!(
                detail,
                "lengths: events={} tree={}",
                events.lines().count(),
                tree.lines().count()
            );
        }
        panic!("tree shape diverges from event stream ({ctx}):\n{detail}");
    }
}

#[test]
fn tree_matches_event_stream_specs() {
    for (name, src) in spec_files() {
        for (opt_name, opts) in option_sets() {
            assert_shapes_match(&src, opts, &format!("{name} / {opt_name}"));
        }
    }
}

#[test]
fn tree_matches_event_stream_boundary() {
    for (i, src) in BOUNDARY.iter().enumerate() {
        for (opt_name, opts) in option_sets() {
            assert_shapes_match(src, opts, &format!("boundary-{i:02} / {opt_name}"));
        }
    }
    assert_shapes_match(&deep_quote(50), Options::all(), "deep-quote-50");
    assert_shapes_match(&deep_quote(1000), Options::all(), "deep-quote-1000");
}

#[test]
fn tree_matches_event_stream_mixed() {
    let src = mixed(200);
    for (opt_name, opts) in option_sets() {
        assert_shapes_match(&src, opts, &format!("mixed-200 / {opt_name}"));
    }
}

/// 同一个 `Parsed` 遍历两遍结果相同 —— 没有漏掉的 `take_*`,
/// 也是 md-test 删逐叶重解析的正确性依据。
#[test]
fn parsed_is_reusable() {
    let src = mixed(60);
    for (_opt_name, opts) in option_sets() {
        let parsed = Parsed::new(&src, opts);
        let a = dump_tree(&parsed);
        let b = dump_tree(&parsed);
        assert_eq!(a, b, "second traversal of the same Parsed differs");
        assert!(parsed.node_count() > 1);
    }
}

/// 遍历全树 `kind()` 不 panic(`Maybe*` 残留会在这里炸出来)。
#[test]
fn no_unresolved_inlines() {
    for (_name, src) in spec_files() {
        for (_opt_name, opts) in option_sets() {
            // dump_tree 对每个节点调 kind()
            let parsed = Parsed::new(&src, opts);
            let _ = dump_tree(&parsed);
        }
    }
}

fn dump_tree(parsed: &Parsed<'_>) -> String {
    fn walk(node: pulldown_cmark::NodeRef<'_, '_>, depth: usize, out: &mut String) {
        let kind = node.kind();
        let span = node.span();
        let _ = writeln!(
            out,
            "{}{:?} {}..{} text={:?}",
            "  ".repeat(depth),
            kind,
            span.start,
            span.end,
            node.text()
        );
        for child in node.children() {
            walk(child, depth + 1, out);
        }
    }
    let mut out = String::new();
    walk(parsed.root(), 0, &mut out);
    out
}

/// 列表松紧双判据一致:`ListInfo.tight` 与「项内容是否包 Paragraph」。
#[test]
fn list_tightness_matches_children() {
    let src = mixed(60);
    let parsed = Parsed::new(&src, Options::all());
    fn walk(node: pulldown_cmark::NodeRef<'_, '_>) {
        if let NodeKind::List(info) = node.kind() {
            for item in node.children() {
                let wraps = item
                    .children()
                    .any(|c| matches!(c.kind(), NodeKind::Paragraph | NodeKind::TightParagraph));
                let child_is_para = item
                    .children()
                    .find(|c| {
                        !matches!(
                            c.kind(),
                            NodeKind::TaskMarker { .. } | NodeKind::Text { .. }
                        )
                    })
                    .is_some_and(|c| matches!(c.kind(), NodeKind::Paragraph));
                // 紧列表项内容直接是文本(TightParagraph 包着);松列表项
                // 内容包 Paragraph。两判据必须同向。
                if info.tight {
                    assert!(!child_is_para, "tight list item wraps Paragraph");
                    assert!(
                        wraps,
                        "tight list item has no TightParagraph wrapper; setext escape?"
                    );
                } else {
                    assert!(child_is_para, "loose list item lacks Paragraph wrapper");
                }
            }
        }
        for child in node.children() {
            walk(child);
        }
    }
    walk(parsed.root());
}

/// `inline_only` / `is_single_paragraph` 的出口判定(`tree-api-03` §1 方案 A
/// 与 `tree-api-07` §3 的边界清单)。
#[test]
fn inline_only_single_paragraph_exit() {
    let opts = Options::all();
    let yes = [
        "hello *a*",
        "hello `code`",
        "a\nb",
        "",
        "hello [a][x]\n\n[x]: url\n",
        "[x]: url\n",
        "text [^fn]\n",
    ];
    for src in yes {
        let parsed = Parsed::inline_only(src, opts);
        assert!(
            parsed.is_single_paragraph(),
            "should be single paragraph: {src:?}"
        );
    }
    let no = [
        "# heading",
        "- item",
        "1. item",
        "> quote",
        "```\ncode\n```\n",
        "a\n\nb",
        "| a | b |\n| - | - |\n",
        "***\n",
    ];
    for src in no {
        let parsed = Parsed::inline_only(src, opts);
        assert!(
            !parsed.is_single_paragraph(),
            "should NOT be single paragraph: {src:?}"
        );
    }
    // 单段落内的行内树照常解析(岛投影的输入)。
    let parsed = Parsed::inline_only("plain *em* `c` ![alt](u.png)", opts);
    let para = parsed.root().first_child().expect("paragraph");
    let kinds: Vec<bool> = para
        .children()
        .map(|c| {
            matches!(
                c.kind(),
                NodeKind::Emphasis | NodeKind::Code(_) | NodeKind::Image(_)
            )
        })
        .collect();
    assert!(kinds.iter().filter(|k| **k).count() >= 3, "{kinds:?}");
}

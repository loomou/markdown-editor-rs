//! Consistency of the tree API with the event stream (feature `tree`; see
//! the repo docs `tree-api-03-fork-impl.md` §6).
//!
//! Core guarantee: the structure and spans `Parsed` yields from the tree
//! line up one to one with the `Parser` event stream — this is the only
//! line of defense for the correctness of `resolve_all_inlines` /
//! `is_container_body`. Fixtures: every upstream spec file (CommonMark /
//! GFM / specs) x three option sets, plus a boundary list and a mixed
//! generator.

#![cfg(feature = "tree")]

use pulldown_cmark::{Event, NodeKind, Options, Parsed, Parser, Tag, TagEnd};
use std::fmt::Write;
use std::path::PathBuf;

// ── fixtures ────────────────────────────────────────────

fn spec_files() -> Vec<(String, String)> {
    let mut paths = Vec::new();
    // The test process cwd is not guaranteed to be the crate root, so
    // build the path at compile time. Two files under CommonMark (spec +
    // smart_punct), three under GitHub, fifteen under specs
    // (specs/definition_lists.txt and friends: even when the md-test side
    // leaves that option off, the tree side must still cover its
    // variants).
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

/// The boundary list from md-test `docs/tree-api-03-fork-impl.md` §7.
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

/// Mixed-construct generator (a trimmed md-test `unique_mixed`): one
/// stretch of each of 14 constructs per round, stirring every block and
/// inline path together.
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

// ── event-side shape strings ─────────────────────────────

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

// ── tree-side shape strings ──────────────────────────────

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
        // Invisible on the event layer: descend only, emit no line.
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

// ── tests ────────────────────────────────────────────────

fn assert_shapes_match(src: &str, opts: Options, ctx: &str) {
    let events = shape_via_events(src, opts);
    let tree = shape_via_tree(src, opts);
    if events != tree {
        // Print the first differing line instead of pasting the whole
        // shape string into the assertion.
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

/// Walking the same `Parsed` twice yields the same result — no `take_*`
/// was missed, and it is the correctness basis for md-test dropping
/// per-leaf re-parsing.
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

/// `kind()` over the whole tree must not panic (leftover `Maybe*` bodies
/// blow up here).
#[test]
fn no_unresolved_inlines() {
    for (_name, src) in spec_files() {
        for (_opt_name, opts) in option_sets() {
            // dump_tree calls kind() on every node
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

/// The two list-tightness criteria agree: `ListInfo.tight` and "does the
/// item content wrap a Paragraph".
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
                // A tight list item's content is text directly (wrapped
                // in TightParagraph); a loose item wraps a Paragraph. Both
                // criteria must point the same way.
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

/// Exit checks for `inline_only` / `is_single_paragraph` (the `tree-api-03`
/// §1 plan A and the `tree-api-07` §3 boundary list).
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
    // The inline tree inside a single paragraph still parses (input for
    // island projection).
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

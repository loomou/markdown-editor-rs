use super::support::{dummy_layout, estimator, loaded};
use crate::incremental::engine::IncrementalEngine;
use md_core::block::{BlockKind, NodeExtra};
use md_core::doc::Doc;
use md_core::document::{Caret, Command, FocusBias, NodeId, Sel, TableOp};
use md_layout::box_tree::{BoxChildren, LayoutBoxId, TypeSlot};
use md_layout::flow::HeightState;
use md_layout::spine::FlowItemKind;
use md_layout::style::{BoxLayoutEnvironment, BoxLayoutStyle};
use std::fmt::Write;

const INITIAL_MARKDOWN: &str = "# heading\n\nalpha **bold** beta\n\n- first item\n- second item\n\n> quoted text\n\n| a | b |\n| --- | --- |\n| c | d |\n\n```rust\nfn main() {}\n```\n\ntail\n";

#[derive(Clone, Copy, Debug)]
enum EditOp {
    Insert(usize),
    DeleteBackward,
    DeleteForward,
    Break,
    SoftBreak,
    Indent,
    Outdent,
    ToggleTask,
    WrapBullet,
    WrapOrdered,
    Undo,
    Redo,
    Move,

    Paste(usize),

    PasteFragment(usize),
    Table(usize),
}

const INSERTS: &[&str] = &["x", " ", "€", "🙂", "**", "`", "_", "\nword"];

const PASTES: &[&str] = &[
    "plain",
    "line one\nline two",
    "para one\n\npara two",
    "**bold** `code`",
    "# frag heading\n\ntext",
    "- a\n- b",
    "| x | y |\n| --- | --- |\n| 1 | 2 |",
    "> quoted\n\n```rust\nfn q() {}\n```",
];

const TABLE_OP_KINDS: usize = 14;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn pick(&mut self, len: usize) -> usize {
        debug_assert!(len > 0);
        (self.next() as usize) % len
    }

    fn one_in(&mut self, denominator: u64) -> bool {
        self.next().is_multiple_of(denominator)
    }
}

#[derive(Debug, PartialEq)]
enum ChildrenSig {
    Vertical(Vec<LayoutBoxId>),
    Island(Vec<LayoutBoxId>),
    None,
}

#[derive(Debug, PartialEq)]
struct NodeSig {
    id: LayoutBoxId,
    kind: BlockKind,
    parent: Option<LayoutBoxId>,
    children: ChildrenSig,
    text: String,
    runs: String,
    extra: NodeExtra,
    revision: u64,
    generation: u32,
    edit_source: bool,
    type_slot: TypeSlot,
    style: BoxLayoutStyle,
}

#[derive(Debug, PartialEq)]
enum FlowKindSig {
    Open(LayoutBoxId),
    Gap,
    Content(LayoutBoxId),
    Collapsed(LayoutBoxId),
    Close(LayoutBoxId),
}

fn node_signature(engine: &IncrementalEngine) -> Vec<NodeSig> {
    engine
        .tree
        .nodes()
        .into_iter()
        .map(|(id, node)| NodeSig {
            id: *id,
            kind: node.kind(),
            parent: node.parent(),
            children: match node.children() {
                BoxChildren::Vertical(ids) => ChildrenSig::Vertical(ids.clone()),
                BoxChildren::Island(ids) => ChildrenSig::Island(ids.clone()),
                BoxChildren::None => ChildrenSig::None,
            },
            text: engine.tree.text_of(node).to_string(),
            runs: format!("{:?}", engine.tree.runs_of(node)),
            extra: node.extra(),
            revision: node.content_revision(),
            generation: node.content_generation(),
            edit_source: node.edit_source(),
            type_slot: node.type_slot(),
            style: *engine.tree.style_of(node),
        })
        .collect()
}

fn spine_signature(engine: &IncrementalEngine) -> Vec<(FlowKindSig, HeightState)> {
    (0..engine.spine.len())
        .map(|pos| {
            let item = engine.spine.item_at(pos);
            let kind = match item.kind {
                FlowItemKind::ContainerOpen { box_id } => FlowKindSig::Open(box_id),
                FlowItemKind::Gap => FlowKindSig::Gap,
                FlowItemKind::Content { box_id } => FlowKindSig::Content(box_id),
                FlowItemKind::Collapsed { box_id } => FlowKindSig::Collapsed(box_id),
                FlowItemKind::ContainerClose { box_id } => FlowKindSig::Close(box_id),
            };
            (kind, item.height)
        })
        .collect()
}

fn text_signature(doc: &md_core::document::Document) -> String {
    let mut out = String::new();
    for id in doc.preorder() {
        let node = doc.arena.get(id).expect("live node");
        if !node.kind.is_text_leaf() {
            continue;
        }

        if node.kind == BlockKind::TableCell && cell_beyond_header_width(doc, id) {
            continue;
        }

        let raw = doc.block_source(id).replace("<br>", "\n");

        let lines: Vec<String> = raw
            .split('\n')
            .map(|line| line.replace("\\|", "|"))
            .collect();

        let truncate_to: Vec<Option<usize>> = table_block_widths(&lines);
        for (index, line) in lines.iter().enumerate() {
            let line = if node.kind == BlockKind::TableCell {
                line.trim()
            } else {
                strip_block_prefixes(line)
            };

            let columns: Vec<&str> = if line.contains('|') {
                let mut columns = split_columns(line);
                if let Some(width) = truncate_to[index] {
                    columns.truncate(width);
                }
                columns
            } else {
                vec![line]
            };
            if !columns.is_empty() && columns.iter().all(|c| is_table_separator_column(c)) {
                continue;
            }
            for column in columns {
                let column = strip_block_prefixes(column);
                if column.is_empty() {
                    continue;
                }

                if matches!(column, "-" | "*" | "+" | "#" | ">" | "[ ]" | "[x]" | "[X]") {
                    continue;
                }

                if column
                    .chars()
                    .all(|c| matches!(c, '*' | '_' | '`' | '~' | '[' | ']' | ' ' | '\t'))
                {
                    continue;
                }

                if column.len() >= 3
                    && column.chars().all(|c| matches!(c, '-' | '*' | '_' | ' '))
                    && column.chars().filter(|c| !c.is_whitespace()).count() >= 3
                {
                    continue;
                }

                if is_fence_marker_line(column) {
                    continue;
                }
                let _ = writeln!(out, "{column}");
            }
        }
    }
    out
}

fn is_table_separator_column(column: &str) -> bool {
    !column.is_empty()
        && column.chars().all(|c| c == '-' || c == ':')
        && column.chars().any(|c| c == '-')
}

fn strip_edge_pipes(line: &str) -> &str {
    let mut s = line.trim();
    s = s.strip_prefix('|').unwrap_or(s);
    s = s.strip_suffix('|').unwrap_or(s);
    s
}

fn split_columns(line: &str) -> Vec<&str> {
    strip_edge_pipes(line)
        .split('|')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect()
}

fn table_block_widths(lines: &[String]) -> Vec<Option<usize>> {
    let columns_of = |line: &str| split_columns(line).len();
    let has_column_bar = |line: &str| strip_edge_pipes(line).contains('|');
    let mut out = vec![None; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if !has_column_bar(&lines[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i + 1;
        while end < lines.len() && has_column_bar(&lines[end]) {
            end += 1;
        }
        for k in start..end {
            if k > start
                && is_separator_line(&lines[k])
                && columns_of(&lines[k - 1]) == columns_of(&lines[k])
            {
                let width = columns_of(&lines[k]);
                for slot in &mut out[k + 1..end] {
                    *slot = Some(width);
                }
                break;
            }
        }
        i = end;
    }
    out
}

fn is_separator_line(line: &str) -> bool {
    let columns = split_columns(line);
    !columns.is_empty() && columns.iter().all(|c| is_table_separator_column(c))
}

fn strip_block_prefixes(line: &str) -> &str {
    let mut s = line.trim();
    loop {
        let stripped = if let Some(rest) = s
            .strip_prefix("[ ] ")
            .or_else(|| s.strip_prefix("[x] "))
            .or_else(|| s.strip_prefix("[X] "))
            .or_else(|| s.strip_prefix('>'))
        {
            rest
        } else if matches!(s.as_bytes().first(), Some(b'-' | b'*' | b'+'))
            && matches!(s.as_bytes().get(1), Some(b' ' | b'\t' | 0))
        {
            &s[2..]
        } else if s.starts_with('#') {
            let hashes = s.bytes().take_while(|b| *b == b'#').count();
            &s[hashes..]
        } else {
            let digits = s.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && matches!(s.as_bytes().get(digits), Some(b'.' | b')')) {
                &s[digits + 1..]
            } else {
                s
            }
        };
        let next = stripped.trim_start();
        if next == s {
            return s;
        }
        s = next;
    }
}

fn cell_beyond_header_width(doc: &md_core::document::Document, id: NodeId) -> bool {
    let Some(row) = doc.arena.get(id).and_then(|node| node.parent) else {
        return false;
    };
    if !matches!(
        doc.arena.get(row).map(|node| node.kind),
        Some(BlockKind::TableRow)
    ) {
        return false;
    }
    let Some(table) = doc.arena.get(row).and_then(|node| node.parent) else {
        return false;
    };
    let header_cols = doc
        .arena
        .children(table)
        .next()
        .map(|first_row| doc.arena.children(first_row).count())
        .unwrap_or(usize::MAX);
    doc.arena
        .children(row)
        .take_while(|&cell| cell != id)
        .count()
        >= header_cols
}

fn is_fence_marker_line(line: &str) -> bool {
    let s = line.trim();
    let marker = match s.as_bytes().first() {
        Some(b @ (b'`' | b'~')) => *b,
        _ => return false,
    };
    let fence_len = s.bytes().take_while(|&b| b == marker).count();
    fence_len >= 3 && !s[fence_len..].contains(marker as char)
}

fn assert_save_reload_matches(doc: &Doc, context: &str) {
    let md = doc.document.to_markdown();
    let reloaded = loaded(&md);
    if std::env::var_os("MD_TEST_RANDOM_DUMP_TREE").is_some() {
        eprintln!("=== DUMP TREE (context={context}) ===");
        for id in doc.document.preorder() {
            let node = doc.document.arena.get(id).expect("live node");
            eprintln!(
                "id={id:?} kind={:?} parent={:?} source={:?} display={:?}",
                node.kind,
                node.parent,
                doc.document.block_source(id),
                doc.document.display(id),
            );
        }
        eprintln!("=== END DUMP ===");
    }
    assert_eq!(
        text_signature(&doc.document),
        text_signature(&reloaded),
        "save-reload diverged\n{context}\nmarkdown={md:?}"
    );
}

fn random_caret(doc: &Doc, rng: &mut Rng) -> Caret {
    let leaves: Vec<_> = doc
        .text_leaves()
        .into_iter()
        .filter(|id| !doc.kind(*id).is_some_and(BlockKind::supports_block_edit))
        .collect();
    let block = leaves[rng.pick(leaves.len())];
    let text = doc.caret_text(block).expect("live text leaf");
    let mut boundaries: Vec<usize> = text.char_indices().map(|(offset, _)| offset).collect();
    boundaries.push(text.len());
    Caret {
        block,
        offset: boundaries[rng.pick(boundaries.len())],
    }
}

fn random_selection(doc: &Doc, rng: &mut Rng) -> Sel {
    let anchor = random_caret(doc, rng);
    if rng.one_in(4) {
        Sel {
            anchor,
            head: random_caret(doc, rng),
        }
    } else {
        Sel::collapsed(anchor)
    }
}

fn retarget_selection(doc: &mut Doc, sel: Sel) -> Sel {
    if sel.anchor == sel.head {
        return Sel::collapsed(doc.retarget_focus(sel.head));
    }
    let (anchor, head) = doc.retarget_focus_range(sel.anchor, sel.head, FocusBias::Neutral);
    Sel { anchor, head }
}

fn random_op(rng: &mut Rng) -> EditOp {
    match rng.pick(26) {
        0..=5 => EditOp::Insert(rng.pick(INSERTS.len())),
        6 => EditOp::DeleteBackward,
        7 => EditOp::DeleteForward,
        8 => EditOp::Break,
        9 => EditOp::SoftBreak,
        10 => EditOp::Indent,
        11 => EditOp::Outdent,
        12 => EditOp::ToggleTask,
        13 => EditOp::WrapBullet,
        14 => EditOp::WrapOrdered,
        15 => EditOp::Undo,
        16 => EditOp::Redo,
        17 => EditOp::Paste(rng.pick(PASTES.len())),
        18 => EditOp::PasteFragment(rng.pick(PASTES.len())),
        19..=21 => EditOp::Table(rng.pick(TABLE_OP_KINDS)),
        _ => EditOp::Move,
    }
}

fn random_edit_op(rng: &mut Rng) -> EditOp {
    match rng.pick(24) {
        0..=5 => EditOp::Insert(rng.pick(INSERTS.len())),
        6 => EditOp::DeleteBackward,
        7 => EditOp::DeleteForward,
        8 => EditOp::Break,
        9 => EditOp::SoftBreak,
        10 => EditOp::Indent,
        11 => EditOp::Outdent,
        12 => EditOp::ToggleTask,
        13 => EditOp::WrapBullet,
        14 => EditOp::WrapOrdered,
        15 => EditOp::Paste(rng.pick(PASTES.len())),
        16 => EditOp::PasteFragment(rng.pick(PASTES.len())),
        17..=19 => EditOp::Table(rng.pick(TABLE_OP_KINDS)),
        _ => EditOp::Move,
    }
}

fn table_op(kind: usize, rng: &mut Rng) -> TableOp {
    match kind {
        0 => TableOp::Insert {
            rows: 2 + rng.pick(3),
            cols: 1 + rng.pick(3),
        },
        1 => TableOp::InsertRowAbove,
        2 => TableOp::InsertRowBelow,
        3 => TableOp::MoveRowUp,
        4 => TableOp::MoveRowDown,
        5 => TableOp::InsertColumnLeft,
        6 => TableOp::InsertColumnRight,
        7 => TableOp::MoveColumnLeft,
        8 => TableOp::MoveColumnRight,
        9 => TableOp::MoveRowTo { index: rng.pick(8) },
        10 => TableOp::MoveColumnTo { index: rng.pick(8) },
        11 => TableOp::DeleteRow,
        12 => TableOp::DeleteColumn,
        13 => TableOp::Resize {
            rows: 1 + rng.pick(5),
            cols: 1 + rng.pick(4),
        },
        _ => unreachable!(),
    }
}

fn caret_inside_a_table(doc: &Doc, rng: &mut Rng) -> Option<Caret> {
    let cells: Vec<_> = doc
        .text_leaves()
        .into_iter()
        .filter(|&block| doc.kind(block) == Some(BlockKind::TableCell))
        .collect();
    if cells.is_empty() {
        return None;
    }
    let cell = cells[rng.pick(cells.len())];
    let text = doc.caret_text(cell).expect("live cell");
    let mut boundaries: Vec<usize> = text.char_indices().map(|(offset, _)| offset).collect();
    boundaries.push(text.len());
    Some(Caret {
        block: cell,
        offset: boundaries[rng.pick(boundaries.len())],
    })
}

fn apply_op(doc: &mut Doc, current: Sel, op: EditOp, rng: &mut Rng) -> Sel {
    match op {
        EditOp::Undo => {
            let restored = doc.undo().unwrap_or(current);
            retarget_selection(doc, restored)
        }
        EditOp::Redo => {
            let restored = doc.redo().unwrap_or(current);
            retarget_selection(doc, restored)
        }
        EditOp::Move => {
            let moved = random_selection(doc, rng);
            retarget_selection(doc, moved)
        }
        EditOp::Table(kind) => {
            let op = table_op(kind, rng);

            let base = caret_inside_a_table(doc, rng).unwrap_or(current.head);
            let caret = doc.apply(Sel::collapsed(base), Command::Table(op));
            retarget_selection(doc, Sel::collapsed(caret))
        }
        other => {
            let command = match other {
                EditOp::Insert(index) => Command::Insert {
                    text: INSERTS[index].to_string(),
                },
                EditOp::DeleteBackward => Command::DeleteBackward,
                EditOp::DeleteForward => Command::DeleteForward,
                EditOp::Break => Command::Break,
                EditOp::SoftBreak => Command::SoftBreak,
                EditOp::Indent => Command::Indent,
                EditOp::Outdent => Command::Outdent,
                EditOp::ToggleTask => Command::ToggleTask,
                EditOp::WrapBullet => Command::WrapList {
                    ordered: false,
                    task: None,
                },
                EditOp::WrapOrdered => Command::WrapList {
                    ordered: true,
                    task: None,
                },
                EditOp::Paste(index) => Command::Paste {
                    text: PASTES[index].to_string(),
                    intent: md_core::document::PasteIntent::PlainText,
                },
                EditOp::PasteFragment(index) => Command::Paste {
                    text: PASTES[index].to_string(),
                    intent: md_core::document::PasteIntent::IndependentFragment,
                },
                EditOp::Undo | EditOp::Redo | EditOp::Move | EditOp::Table(_) => unreachable!(),
            };
            let caret = doc.apply(current, command);
            retarget_selection(doc, Sel::collapsed(caret))
        }
    }
}

fn env_count(name: &str, default: usize, max: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(1, max)
}

fn env_start(name: &str) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
}

fn assert_document_tree_links(doc: &md_core::document::Document, context: &str) {
    let live = doc.arena.live_count();
    let mut stack = vec![doc.root];
    let mut seen = std::collections::HashSet::new();
    while let Some(parent) = stack.pop() {
        assert!(
            seen.insert(parent),
            "node visited twice: {parent:?}\n{context}"
        );
        let children: Vec<_> = doc.arena.children(parent).take(live + 1).collect();
        assert!(
            children.len() <= live,
            "sibling chain did not terminate under {parent:?}: {children:?}\n{context}"
        );
        let mut siblings = std::collections::HashSet::new();
        for child in children {
            assert!(
                siblings.insert(child),
                "duplicate sibling {child:?} under {parent:?}\n{context}"
            );
            let node = doc
                .arena
                .get(child)
                .unwrap_or_else(|| panic!("dead child {child:?} under {parent:?}\n{context}"));
            assert_eq!(node.parent, Some(parent), "{context}");
            stack.push(child);
        }
    }
}

fn assert_heading_sources_stay_valid(doc: &Doc, context: &str) {
    for id in doc.document.preorder() {
        let Some(node) = doc.document.arena.get(id) else {
            continue;
        };
        if !matches!(node.kind, BlockKind::Heading(_)) {
            continue;
        }
        let source = doc.document.block_source(id);
        if doc.document.display(id) == source {
            continue;
        }
        let first = source.lines().next().unwrap_or("");
        let hashes = first.chars().take_while(|c| *c == '#').count();
        let rest = &first[hashes..];

        assert!(
            rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t'),
            "heading source is not valid ATX: {source:?} display={:?}\n{context}",
            doc.document.display(id)
        );
    }
}

fn assert_tables_stay_rectangular(doc: &Doc, context: &str) {
    for id in doc.document.preorder() {
        let Some(node) = doc.document.arena.get(id) else {
            continue;
        };
        if node.kind != BlockKind::Table {
            continue;
        }
        let rows: Vec<NodeId> = doc.document.arena.children(id).collect();
        assert!(!rows.is_empty(), "table has no rows\n{context}");
        let width = doc.document.arena.children(rows[0]).count();
        for row in &rows {
            assert_eq!(
                doc.document.arena.get(*row).map(|n| n.kind),
                Some(BlockKind::TableRow),
                "table child is not a TableRow\n{context}"
            );
            let cells: Vec<NodeId> = doc.document.arena.children(*row).collect();
            assert_eq!(
                cells.len(),
                width,
                "table rows are ragged: {width} vs {}\n{context}",
                cells.len()
            );
            for cell in &cells {
                assert_eq!(
                    doc.document.arena.get(*cell).map(|n| n.kind),
                    Some(BlockKind::TableCell),
                    "table row child is not a TableCell\n{context}"
                );
            }
        }
    }
}

fn assert_step_alloc_within_cap(doc: &Doc, prev_slots: &mut usize, context: &str) {
    let slots = doc.document.arena.live_count() + doc.document.arena.tombstone_count();
    let delta = slots - *prev_slots;
    assert!(
        delta <= 32,
        "{context}: one edit allocated {delta} arena slots — far past the \
         natural peak (19, big pastes); an edit path is leaking nodes in bulk"
    );
    *prev_slots = slots;
}

fn assert_growth_stays_bounded(doc: &Doc, base: usize, productive: usize, context: &str) {
    let slots = doc.document.arena.live_count() + doc.document.arena.tombstone_count();
    assert!(
        slots <= base + productive + productive / 2 + 24,
        "{context}: arena slots {slots} exceed the bounded-growth envelope \
         (base {base} + 1.5 x {productive} + 24) — an edit path is leaking nodes"
    );
}

#[test]
fn random_edit_sequences_match_a_cold_engine() {
    let case_start = env_start("MD_TEST_RANDOM_CASE_START");

    let cases = env_count("MD_TEST_RANDOM_CASES", 16, 4096);
    let steps = env_count("MD_TEST_RANDOM_STEPS", 256, 1024);
    let trace_progress = std::env::var_os("MD_TEST_RANDOM_TRACE").is_some();
    let env = BoxLayoutEnvironment::default();

    let mut table_ops_attempted = 0usize;
    let mut table_ops_booked = 0usize;

    for case in case_start..case_start.saturating_add(cases) {
        let mut attempted = 0usize;
        let mut booked = 0usize;
        let failure = run_case(
            &env,
            case,
            steps,
            trace_progress,
            &mut attempted,
            &mut booked,
        );
        table_ops_attempted += attempted;
        table_ops_booked += booked;
        let Some(original) = failure else {
            continue;
        };

        let failed_step = failure_step(&original).unwrap_or(steps - 1);
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let mut lo = 1usize;
        let mut hi = failed_step + 1;
        let mut minimal_failure = original.clone();
        while lo < hi {
            let mid = (lo + hi) / 2;
            let mut a = 0usize;
            let mut b = 0usize;
            let replay = run_case(&env, case, mid, false, &mut a, &mut b);
            match replay {
                Some(msg) => {
                    minimal_failure = msg;
                    hi = mid;
                }
                None => lo = mid + 1,
            }
        }
        std::panic::set_hook(quiet);
        panic!(
            "case {case} shrunk: reproduces with {lo} steps \
             (full run reached step {failed_step} of {steps})\n\
             --- minimal replay failure ---\n{minimal_failure}\n\
             --- original full-dose failure ---\n{original}"
        );
    }

    assert!(
        table_ops_attempted < 9 || table_ops_booked > 0,
        "table ops were attempted {table_ops_attempted} times but none took effect"
    );
}

fn run_case(
    env: &BoxLayoutEnvironment,
    case: usize,
    steps: usize,
    trace_progress: bool,
    table_ops_attempted: &mut usize,
    table_ops_booked: &mut usize,
) -> Option<String> {
    let seed = 0x6d64_7465_7374_0001u64.wrapping_add(case as u64 * 0x9e37_79b9);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut rng = Rng::new(seed);
        let mut doc = Doc::new(loaded(INITIAL_MARKDOWN));
        let initial = Sel::collapsed(random_caret(&doc, &mut rng));
        let mut current = retarget_selection(&mut doc, initial);
        let _ = doc.take_changes();
        let growth_base = doc.document.arena.live_count() + doc.document.arena.tombstone_count();
        let mut prev_slots = growth_base;

        let mut productive_steps = 0usize;
        let mut hot = IncrementalEngine::new(&doc.document, *env, estimator(), dummy_layout());
        let mut trace = Vec::new();

        for step in 0..steps {
            let op = random_op(&mut rng);
            trace.push(op);
            if trace_progress {
                eprintln!("case={case} seed={seed:#x} step={step} op={op:?} phase=before");
            }
            current = apply_op(&mut doc, current, op, &mut rng);
            if trace_progress {
                eprintln!("case={case} step={step} phase=applied");
            }
            let changes = doc.take_changes();
            if !changes.changes.is_empty() {
                productive_steps += 1;
            }
            if trace_progress {
                eprintln!("case={case} step={step} phase=changes {changes:?}");
            }
            let link_context = format!(
                "seed={seed:#x} step={step} op={op:?}\nchanges={:?}",
                changes.changes
            );
            assert_document_tree_links(&doc.document, &link_context);
            assert_heading_sources_stay_valid(&doc, &link_context);
            assert_tables_stay_rectangular(&doc, &link_context);
            assert_step_alloc_within_cap(&doc, &mut prev_slots, &link_context);
            assert_growth_stays_bounded(&doc, growth_base, productive_steps, &link_context);
            if matches!(op, EditOp::Table(_)) {
                *table_ops_attempted += 1;
                if !changes.changes.is_empty() {
                    *table_ops_booked += 1;
                }
            }
            hot.apply_changes(&doc.document, &changes);
            hot.sync_block_edit(&doc.document);
            if trace_progress {
                eprintln!("case={case} step={step} phase=hot");
            }

            let cold = IncrementalEngine::new(&doc.document, *env, estimator(), dummy_layout());
            if trace_progress {
                eprintln!("case={case} step={step} phase=cold");
            }
            let context = format!(
                "seed={seed:#x} step={step} op={op:?}\nchanges={:?}\ntrace={trace:?}\nmarkdown={:?}",
                changes.changes,
                doc.document.to_markdown()
            );
            assert_eq!(node_signature(&hot), node_signature(&cold), "{context}");
            assert_eq!(spine_signature(&hot), spine_signature(&cold), "{context}");
            assert_eq!(hot.total_height(), cold.total_height(), "{context}");

            if (step + 1).is_multiple_of(32) || step + 1 == steps {
                assert_save_reload_matches(
                    &doc,
                    &format!("seed={seed:#x} case={case} step={step} op={op:?}"),
                );
            }
        }
    }));
    match result {
        Ok(()) => None,
        Err(payload) => Some(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "panic without a message".to_string()),
        ),
    }
}

fn failure_step(msg: &str) -> Option<usize> {
    let idx = msg.find("step=")?;
    let digits: String = msg[idx + 5..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[test]
fn random_edit_sequences_undo_to_the_bottom_restore_the_source() {
    let cases = env_count("MD_TEST_RANDOM_CASES", 8, 4096);
    let trace_progress = std::env::var_os("MD_TEST_RANDOM_TRACE").is_some();
    const STEPS: usize = 32;

    for case in 0..cases {
        let seed = 0x6d64_7465_7374_0002u64.wrapping_add(case as u64 * 0x9e37_79b9);
        let mut rng = Rng::new(seed);
        let mut doc = Doc::new(loaded(INITIAL_MARKDOWN));
        let initial = Sel::collapsed(random_caret(&doc, &mut rng));
        let mut current = retarget_selection(&mut doc, initial);
        let _ = doc.take_changes();
        let growth_base = doc.document.arena.live_count() + doc.document.arena.tombstone_count();
        let mut prev_slots = growth_base;
        let mut productive_steps = 0usize;

        for step in 0..STEPS {
            let op = random_edit_op(&mut rng);
            let probe_step_undo = std::env::var_os("MD_TEST_RANDOM_STEP_UNDO").is_some();
            let revision_before = doc.document.revision();
            current = apply_op(&mut doc, current, op, &mut rng);
            if doc.document.revision() != revision_before {
                productive_steps += 1;
            }
            assert_tables_stay_rectangular(
                &doc,
                &format!("seed={seed:#x} case={case} step={step} op={op:?}"),
            );
            assert_step_alloc_within_cap(
                &doc,
                &mut prev_slots,
                &format!("seed={seed:#x} case={case} step={step} op={op:?}"),
            );
            assert_growth_stays_bounded(
                &doc,
                growth_base,
                productive_steps,
                &format!("seed={seed:#x} case={case} step={step} op={op:?}"),
            );
            if trace_progress {
                eprintln!(
                    "undo-case={case} step={step} op={op:?} md={:?}",
                    doc.document.to_markdown()
                );
            }
            if probe_step_undo {
                let after_md = doc.document.to_markdown();
                let mut budget = 256;
                while doc.undo().is_some() {
                    budget -= 1;
                    assert!(budget > 0, "undo never ran dry at step {step}");
                }
                assert_eq!(
                    doc.document.to_markdown(),
                    INITIAL_MARKDOWN,
                    "step {step} op={op:?} undo-to-bottom diverged"
                );
                let mut budget = 256;
                while doc.redo().is_some() {
                    budget -= 1;
                    assert!(budget > 0, "redo never ran dry at step {step}");
                }
                assert_eq!(
                    doc.document.to_markdown(),
                    after_md,
                    "step {step} op={op:?} undo+redo round-trip diverged"
                );
                current = retarget_selection(&mut doc, current);
            }
        }
        let _ = doc.take_changes();
        let edited = doc.document.to_markdown();
        assert_save_reload_matches(&doc, &format!("seed={seed:#x} case={case} pre-undo"));

        let mut undo_budget = STEPS * 4;
        while doc.undo().is_some() {
            undo_budget -= 1;
            assert!(
                undo_budget > 0,
                "undo never ran dry: seed={seed:#x} case={case}"
            );
        }
        assert_eq!(
            doc.document.to_markdown(),
            INITIAL_MARKDOWN,
            "undo-to-bottom lost the source: seed={seed:#x} case={case}"
        );

        let mut redo_budget = STEPS * 4;
        while doc.redo().is_some() {
            redo_budget -= 1;
            assert!(
                redo_budget > 0,
                "redo never ran dry: seed={seed:#x} case={case}"
            );
        }
        assert_eq!(
            doc.document.to_markdown(),
            edited,
            "redo-to-bottom diverged from the edited state: seed={seed:#x} case={case}"
        );
    }
}

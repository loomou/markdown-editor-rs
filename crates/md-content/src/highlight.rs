use md_theme::SyntaxRole;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const MAX_BYTES: usize = 256 * 1024;
const MAX_CACHE: usize = 256;
const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;

const CAPTURES: &[&str] = &[
    "comment",
    "keyword",
    "keyword.operator",
    "keyword.directive",
    "string",
    "string.special.symbol",
    "string.special.url",
    "string.special",
    "string.regexp",
    "string.escape",
    "character",
    "escape",
    "number",
    "constant.builtin",
    "constant",
    "boolean",
    "function.macro",
    "function.builtin",
    "function.method",
    "function",
    "constructor",
    "type.builtin",
    "type",
    "module",
    "property",
    "variable.member",
    "variable.parameter",
    "variable.builtin",
    "attribute",
    "tag.attribute",
    "tag.delimiter",
    "tag",
    "operator",
    "punctuation.special",
    "punctuation",
    "label",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub role: SyntaxRole,
}

thread_local! {
    static HIGHLIGHTER: RefCell<Highlighter> = RefCell::new(Highlighter::new());
    static CONFIGS: RefCell<HashMap<&'static str, HighlightConfiguration>> =
        RefCell::new(HashMap::new());
    static CACHE: RefCell<SpanCache> = RefCell::new(SpanCache::new());
}

struct SpanCache {
    map: HashMap<(u64, u64), Rc<[Span]>>,
    lru: VecDeque<(u64, u64)>,
    bytes: usize,
}

impl SpanCache {
    fn new() -> Self {
        SpanCache {
            map: HashMap::new(),
            lru: VecDeque::new(),
            bytes: 0,
        }
    }

    fn get(&mut self, key: (u64, u64)) -> Option<Rc<[Span]>> {
        if let Some(spans) = self.map.get(&key) {
            self.lru.retain(|k| *k != key);
            self.lru.push_back(key);
            Some(Rc::clone(spans))
        } else {
            None
        }
    }

    fn insert(&mut self, key: (u64, u64), spans: Rc<[Span]>) {
        let incoming = spans.len().saturating_mul(std::mem::size_of::<Span>());
        if incoming > MAX_CACHE_BYTES {
            return;
        }
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.map.entry(key) {
            let previous = e.insert(spans);
            self.bytes = self
                .bytes
                .saturating_sub(previous.len().saturating_mul(std::mem::size_of::<Span>()));
            self.bytes = self.bytes.saturating_add(incoming);
            self.lru.retain(|k| *k != key);
            self.lru.push_back(key);
            return;
        }
        while self.map.len() >= MAX_CACHE || self.bytes.saturating_add(incoming) > MAX_CACHE_BYTES {
            if let Some(old) = self.lru.pop_front() {
                if let Some(previous) = self.map.remove(&old) {
                    self.bytes = self
                        .bytes
                        .saturating_sub(previous.len().saturating_mul(std::mem::size_of::<Span>()));
                }
            } else {
                break;
            }
        }
        self.map.insert(key, spans);
        self.bytes = self.bytes.saturating_add(incoming);
        self.lru.push_back(key);
    }
}

pub fn spans(lang: &str, text: &str) -> Rc<[Span]> {
    if text.is_empty() || text.len() > MAX_BYTES {
        return Rc::new([]);
    }
    let Some(canon) = canonical(lang) else {
        return Rc::new([]);
    };
    let key = (hash_str(canon), hash_str(text));
    if let Some(hit) = CACHE.with(|c| c.borrow_mut().get(key)) {
        return hit;
    }
    let Some(out) = highlight(canon, text) else {
        return Rc::new([]);
    };
    let shared: Rc<[Span]> = out.into();
    CACHE.with(|c| c.borrow_mut().insert(key, Rc::clone(&shared)));
    shared
}

fn canonical(lang: &str) -> Option<&'static str> {
    match lang {
        "rust" | "rs" => Some("rust"),
        "python" | "py" => Some("python"),
        "javascript" | "js" | "jsx" => Some("javascript"),
        "typescript" | "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "json" => Some("json"),
        "toml" => Some("toml"),
        "bash" | "sh" | "zsh" | "shell" => Some("bash"),
        "go" | "golang" => Some("go"),
        "c" => Some("c"),
        "cpp" | "c++" | "cxx" | "cc" => Some("cpp"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "yaml" | "yml" => Some("yaml"),
        _ => None,
    }
}

fn hash_str(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn highlight(canon: &'static str, text: &str) -> Option<Vec<Span>> {
    CONFIGS.with(|slot| {
        let mut configs = slot.borrow_mut();
        if let std::collections::hash_map::Entry::Vacant(e) = configs.entry(canon) {
            e.insert(config_for(canon)?);
        }
        HIGHLIGHTER.with(|hl| {
            let mut highlighter = hl.borrow_mut();
            let cfg = configs.get(canon)?;
            let events = highlighter
                .highlight(cfg, text.as_bytes(), None, |_| None)
                .ok()?;
            Some(events_to_spans(text, events))
        })
    })
}

fn config_for(canon: &str) -> Option<HighlightConfiguration> {
    let (language, name, query) = match canon {
        "rust" => (
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
        ),
        "python" => (
            tree_sitter_python::LANGUAGE.into(),
            "python",
            tree_sitter_python::HIGHLIGHTS_QUERY,
        ),
        "javascript" => (
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
        ),
        "typescript" => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        ),
        "tsx" => (
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "tsx",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        ),
        "json" => (
            tree_sitter_json::LANGUAGE.into(),
            "json",
            tree_sitter_json::HIGHLIGHTS_QUERY,
        ),
        "toml" => (
            tree_sitter_toml_ng::LANGUAGE.into(),
            "toml",
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
        ),
        "bash" => (
            tree_sitter_bash::LANGUAGE.into(),
            "bash",
            tree_sitter_bash::HIGHLIGHT_QUERY,
        ),
        "go" => (
            tree_sitter_go::LANGUAGE.into(),
            "go",
            tree_sitter_go::HIGHLIGHTS_QUERY,
        ),
        "c" => (
            tree_sitter_c::LANGUAGE.into(),
            "c",
            tree_sitter_c::HIGHLIGHT_QUERY,
        ),
        "cpp" => (
            tree_sitter_cpp::LANGUAGE.into(),
            "cpp",
            tree_sitter_cpp::HIGHLIGHT_QUERY,
        ),
        "html" => (
            tree_sitter_html::LANGUAGE.into(),
            "html",
            tree_sitter_html::HIGHLIGHTS_QUERY,
        ),
        "css" => (
            tree_sitter_css::LANGUAGE.into(),
            "css",
            tree_sitter_css::HIGHLIGHTS_QUERY,
        ),
        "yaml" => (
            tree_sitter_yaml::LANGUAGE.into(),
            "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
        ),
        _ => return None,
    };
    let mut cfg = HighlightConfiguration::new(language, name, query, "", "").ok()?;
    cfg.configure(CAPTURES);
    Some(cfg)
}

fn role_for(index: usize) -> SyntaxRole {
    match CAPTURES.get(index).copied() {
        Some("comment") => SyntaxRole::Comment,
        Some("keyword" | "keyword.operator" | "type.builtin") => SyntaxRole::Keyword,
        Some("keyword.directive") => SyntaxRole::Special,
        Some("string") => SyntaxRole::String,
        Some("string.special.symbol") => SyntaxRole::Symbol,
        Some("string.special.url") => SyntaxRole::Function,
        Some(
            "string.special" | "string.regexp" | "string.escape" | "escape" | "punctuation.special",
        ) => SyntaxRole::Special,
        Some("character" | "tag.delimiter") => SyntaxRole::Character,
        Some(
            "number" | "constant" | "constant.builtin" | "boolean" | "attribute"
            | "function.builtin",
        ) => SyntaxRole::Number,
        Some("function" | "function.method" | "tag") => SyntaxRole::Function,
        Some("function.macro") => SyntaxRole::Macro,
        Some("constructor" | "type" | "module" | "tag.attribute") => SyntaxRole::TypeName,
        Some("property" | "variable.member") => SyntaxRole::Property,
        Some("variable.parameter") => SyntaxRole::Parameter,
        Some("variable.builtin") => SyntaxRole::Builtin,
        Some("operator") => SyntaxRole::Operator,
        Some("punctuation") => SyntaxRole::Punctuation,
        Some("label") => SyntaxRole::Label,
        _ => SyntaxRole::Default,
    }
}

fn events_to_spans(
    text: &str,
    events: impl Iterator<Item = Result<HighlightEvent, tree_sitter_highlight::Error>>,
) -> Vec<Span> {
    let mut stack = Vec::new();
    let mut spans = Vec::new();
    let mut at = 0usize;
    for event in events.flatten() {
        match event {
            HighlightEvent::HighlightStart(h) => stack.push(role_for(h.0)),
            HighlightEvent::HighlightEnd => {
                stack.pop();
            }
            HighlightEvent::Source { start, end } => {
                let start = snap_start(text, start).max(at);
                let end = snap_end(text, end);
                if start >= end {
                    continue;
                }
                if start > at {
                    push_span(&mut spans, at, start, SyntaxRole::Default);
                }
                let role = stack.last().copied().unwrap_or(SyntaxRole::Default);
                push_span(&mut spans, start, end, role);
                at = at.max(end);
            }
        }
    }
    if at < text.len() {
        push_span(&mut spans, at, text.len(), SyntaxRole::Default);
    }
    spans
}

fn push_span(spans: &mut Vec<Span>, start: usize, end: usize, role: SyntaxRole) {
    if start >= end {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.end == start
        && last.role == role
    {
        last.end = end;
        return;
    }
    spans.push(Span { start, end, role });
}

fn snap_start(text: &str, i: usize) -> usize {
    let i = i.min(text.len());
    if text.is_char_boundary(i) {
        i
    } else {
        (0..i)
            .rev()
            .find(|&j| text.is_char_boundary(j))
            .unwrap_or(0)
    }
}

fn snap_end(text: &str, i: usize) -> usize {
    let i = i.min(text.len());
    if text.is_char_boundary(i) {
        i
    } else {
        (i..=text.len())
            .find(|&j| text.is_char_boundary(j))
            .unwrap_or(text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CAPTURES, Span, events_to_spans, push_span, role_for, snap_end, snap_start, spans,
    };
    use md_theme::SyntaxRole;

    #[test]
    fn rust_highlights_keyword_and_string() {
        let src = "fn main() { let x = \"hi\"; }\n";
        let got = spans("rust", src);
        assert!(got.iter().any(|s| s.role == SyntaxRole::Keyword));
        assert!(got.iter().any(|s| s.role == SyntaxRole::String));
        assert_eq!(got.first().map(|s| s.start), Some(0));
        assert_eq!(got.last().map(|s| s.end), Some(src.len()));
    }

    #[test]
    fn unknown_lang_is_empty() {
        assert!(spans("not-a-lang", "fn main() {}").is_empty());
        assert!(spans("", "fn main() {}").is_empty());
    }

    #[test]
    fn adjacent_same_role_merges() {
        let mut spans = Vec::new();
        push_span(&mut spans, 0, 2, SyntaxRole::Keyword);
        push_span(&mut spans, 2, 5, SyntaxRole::Keyword);
        push_span(&mut spans, 5, 6, SyntaxRole::Default);
        assert_eq!(spans.len(), 2);
        assert_eq!(
            spans[0],
            Span {
                start: 0,
                end: 5,
                role: SyntaxRole::Keyword
            }
        );
    }

    #[test]
    fn snap_does_not_split_multibyte() {
        let text = "a😀b";
        let mid = 2;
        assert!(!text.is_char_boundary(mid));
        let s = snap_start(text, mid);
        let e = snap_end(text, mid);
        assert!(text.is_char_boundary(s));
        assert!(text.is_char_boundary(e));
    }

    #[test]
    fn events_are_normalized_to_contiguous_non_overlapping_spans() {
        use tree_sitter_highlight::HighlightEvent;

        let text = "élan";
        let events = [
            Ok(HighlightEvent::Source { start: 0, end: 1 }),
            Ok(HighlightEvent::Source {
                start: 1,
                end: text.len(),
            }),
        ];
        let got = events_to_spans(text, events.into_iter());
        assert_eq!(got.first().map(|span| span.start), Some(0));
        assert_eq!(got.last().map(|span| span.end), Some(text.len()));
        for pair in got.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        assert_eq!(
            got.iter().map(|span| span.end - span.start).sum::<usize>(),
            text.len()
        );
    }

    #[test]
    fn alias_rs_is_rust() {
        let a = spans("rs", "fn x() {}");
        let b = spans("rust", "fn x() {}");
        assert!(!a.is_empty());
        assert_eq!(a, b);
    }

    fn role_at(spans: &[Span], src: &str, needle: &str) -> SyntaxRole {
        let i = src.find(needle).expect(needle);
        spans
            .iter()
            .find(|s| s.start <= i && i < s.end)
            .map(|s| s.role)
            .unwrap_or(SyntaxRole::Default)
    }

    #[test]
    fn rust_roles_follow_catppuccin_captures() {
        let src = "fn f<'a>(x: &'a i32) { let _ = self.n; foo!(\"\\n\"); }\n";
        let got = spans("rust", src);
        assert_eq!(role_at(&got, src, "fn"), SyntaxRole::Keyword);
        assert_eq!(role_at(&got, src, "a>"), SyntaxRole::Label);
        assert_eq!(role_at(&got, src, "x:"), SyntaxRole::Parameter);
        assert_eq!(role_at(&got, src, "i32"), SyntaxRole::Keyword);
        assert_eq!(role_at(&got, src, "self"), SyntaxRole::Builtin);
        assert_eq!(role_at(&got, src, "n;"), SyntaxRole::Property);
        assert_eq!(role_at(&got, src, "foo"), SyntaxRole::Macro);
        assert_eq!(role_at(&got, src, "\\n"), SyntaxRole::Special);
        assert_eq!(role_at(&got, src, "&'"), SyntaxRole::Operator);
    }

    #[test]
    fn every_configured_capture_has_an_explicit_role() {
        for (index, capture) in CAPTURES.iter().enumerate() {
            assert_ne!(
                role_for(index),
                SyntaxRole::Default,
                "capture {capture:?} has no explicit syntax role"
            );
        }
    }
}

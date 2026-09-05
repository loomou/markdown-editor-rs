use crate::document::{editor_options, floor_char_boundary, load_markdown};

fn assert_three_hops(text: &str) {
    let doc = load_markdown(text, editor_options());
    let md = doc.to_markdown();
    let _ = load_markdown(&md, editor_options());
}

#[test]
fn malformed_corpus_never_panics() {
    let mut corpus: Vec<String> = vec![
        String::new(),
        "\n\n\n".into(),
        "---\n".into(),
        "| a | b |\n| --- |".into(),
        "| a |\n| --- | --- | --- |".into(),
        "| a | b |\n| --- | --- |\n| c |".into(),
        "| | | | |".into(),
        "```\ncode".into(),
        "~~~\ncode\n```".into(),
        "```rust\n```".into(),
        "> a\n> > b\n> > > c".into(),
        "a\r\nb\r\n".into(),
        "a\rb".into(),
        "a\0b".into(),
        "\u{feff}# heading\n".into(),
        "[^".into(),
        "![alt](".into(),
        "](".into(),
        "$$".into(),
        "$$x".into(),
        "- \n- \n- \n".into(),
        "[ ] [x] - * + 1. 2) #".into(),
    ];

    corpus.push(format!("{}text", "> ".repeat(1000)));

    let mut deep_list = String::new();
    for i in 0..500 {
        deep_list.push_str(&" ".repeat(i));
        deep_list.push_str("- item\n");
    }
    corpus.push(deep_list);

    corpus.push(format!("{}\n", "x".repeat(1 << 20)));

    for text in &corpus {
        assert_three_hops(text);
    }
}

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
}

const CORRUPTIONS: &[&str] = &[
    "\r", "\n", "\r\n", "\t", "\0", " ", ">", "|", "`", "*", "#", "-", "_", "[", "]", "(", ")",
    "!", "$", "~", ":", "€", "🙂", "\u{200d}", "\u{feff}",
];

fn env_count(name: &str, default: usize, max: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(1, max)
}

#[test]
fn randomly_corrupted_sources_never_panic() {
    let cases = env_count("MD_TEST_MALFORMED_CASES", 256, 4096);
    let seeds = [
        "# heading\n\nalpha **bold** beta\n\n- first item\n- second item\n",
        "| a | b |\n| --- | --- |\n| c | d |\n",
        "> quoted\n\n```rust\nfn main() {}\n```\n",
        "text with `code` and $x$ and ![img](url)\n",
    ];
    let mut rng = Rng::new(0x6d64_7465_7374_0003u64);
    for case in 0..cases {
        let mut text = seeds[rng.pick(seeds.len())].to_string();
        for _ in 0..1 + rng.pick(4) {
            corrupt(&mut text, &mut rng);
        }
        assert_three_hops(&text);

        let _ = case;
    }
}

fn corrupt(text: &mut String, rng: &mut Rng) {
    if text.is_empty() {
        text.push_str(CORRUPTIONS[rng.pick(CORRUPTIONS.len())]);
        return;
    }
    match rng.pick(4) {
        0 => {
            let (start, end) = random_span(text, rng);
            text.replace_range(start..end, "");
        }
        1 => {
            let (start, end) = random_span(text, rng);
            let piece = CORRUPTIONS[rng.pick(CORRUPTIONS.len())];
            text.replace_range(start..end, piece);
        }
        2 => {
            let at = floor_char_boundary(text, rng.pick(text.len()));
            let piece = CORRUPTIONS[rng.pick(CORRUPTIONS.len())];
            text.insert_str(at, piece);
        }
        _ => {
            let at = floor_char_boundary(text, rng.pick(text.len()));
            text.truncate(at);
        }
    }
}

fn random_span(text: &str, rng: &mut Rng) -> (usize, usize) {
    let start = floor_char_boundary(text, rng.pick(text.len()));
    let end = (start + 1 + rng.pick(8)).min(text.len());

    let end = crate::document::prev_char_boundary(text, end).max(start);
    (start, end)
}

#[test]
fn ten_thousand_nested_quotes_survive_all_three_hops() {
    let text = format!("{}text\n", "> ".repeat(10_000));
    assert_three_hops(&text);
}

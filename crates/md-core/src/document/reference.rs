use super::change::DocChange;
use super::load;
use pulldown_cmark::{Options, Parser};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Definition {
    pub(crate) label: String,
    pub(crate) dest: String,
    pub(crate) title: String,
}

pub(crate) fn fold_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut pending_space = false;
    for ch in label.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            for folded in ch.to_lowercase() {
                out.push(folded);
            }
        }
    }
    out
}

pub(crate) fn parse_definition(line: &str) -> Option<Definition> {
    let source = normalize_line(line)?;
    let mut opts = Options::empty();
    opts.remove(Options::ENABLE_DEFINITION_LIST);
    let parser = Parser::new_ext(&source, opts);
    for (label, def) in parser.reference_definitions().iter() {
        let Some(raw) = source.get(def.span.clone()) else {
            continue;
        };
        if !raw.trim().is_empty() {
            return Some(Definition {
                label: fold_label(label),
                dest: def.dest.to_string(),
                title: def.title.as_deref().map(str::to_string).unwrap_or_default(),
            });
        }
    }
    None
}

fn normalize_line(line: &str) -> Option<String> {
    let stripped = line.strip_prefix('\u{feff}').unwrap_or(line);
    let folded = load::fold_cr(stripped);
    Some(format!("{folded}\n"))
}

pub(crate) fn merged_definition_change(host: &[String], incoming: &[String]) -> Option<DocChange> {
    let mut labels: Vec<String> = host
        .iter()
        .filter_map(|line| parse_definition(line).map(|d| d.label))
        .collect();
    let mut merged: Vec<String> = Vec::new();
    for line in incoming {
        let Some(def) = parse_definition(line) else {
            continue;
        };
        if labels.contains(&def.label) {
            continue;
        }
        labels.push(def.label);
        merged.push(line.clone());
    }
    if merged.is_empty() {
        return None;
    }
    let mut next = host.to_vec();
    next.append(&mut merged);
    Some(DocChange::ReferenceDefsChanged {
        old: Arc::new(host.to_vec()),
        new: Arc::new(next),
    })
}

pub(crate) fn used_definitions(defs: &[String], wanted: &[(String, String)]) -> Vec<String> {
    defs.iter()
        .filter(|line| {
            parse_definition(line).is_some_and(|d| {
                wanted
                    .iter()
                    .any(|(dest, title)| d.dest == *dest && d.title == *title)
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Definition, fold_label, parse_definition};

    fn def(label: &str, dest: &str, title: &str) -> Definition {
        Definition {
            label: label.to_string(),
            dest: dest.to_string(),
            title: title.to_string(),
        }
    }

    #[test]
    fn parses_the_three_title_shapes_and_none() {
        assert_eq!(
            parse_definition("[docs]: https://ex.test"),
            Some(def("docs", "https://ex.test", ""))
        );
        assert_eq!(
            parse_definition("[docs]: https://ex.test \"the title\""),
            Some(def("docs", "https://ex.test", "the title"))
        );
        assert_eq!(
            parse_definition("[docs]: https://ex.test 't'"),
            Some(def("docs", "https://ex.test", "t"))
        );
        assert_eq!(
            parse_definition("[docs]: https://ex.test (t)"),
            Some(def("docs", "https://ex.test", "t"))
        );
        assert_eq!(
            parse_definition("  [docs]: <https://ex.test>"),
            Some(def("docs", "https://ex.test", ""))
        );
    }

    #[test]
    fn non_definitions_are_rejected() {
        assert!(parse_definition("plain text").is_none());
        assert!(parse_definition("[docs] no colon").is_none());
        assert!(parse_definition("[docs]:").is_none());
    }

    #[test]
    fn labels_fold_case_and_whitespace() {
        assert_eq!(fold_label("  A  b  "), "a b");
        assert_eq!(
            parse_definition("[Foo   BAR]: https://ex.test").map(|d| d.label),
            Some("foo bar".to_string())
        );
    }

    #[test]
    fn parses_multiline_definitions() {
        assert_eq!(
            parse_definition("[r]:\n target"),
            Some(def("r", "target", ""))
        );
        assert_eq!(
            parse_definition("[r]: target\n \"the title\""),
            Some(def("r", "target", "the title"))
        );
    }
}

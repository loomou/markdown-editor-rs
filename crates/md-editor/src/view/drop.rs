use super::EditorView;
use gpui::{Context, Window};
use md_content::images;
use md_core::block::BlockKind;
use md_core::document::{Command, PasteIntent};
use std::path::Path;

impl EditorView {
    pub(super) fn drop_images(
        &mut self,
        paths: &[std::path::PathBuf],
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.focus);
        let source = self.state.doc.source_path.clone();
        let mut snippets = Vec::new();
        for path in paths {
            if !images::is_image_path(path) {
                continue;
            }
            let dest = images::markdown_dest(path, source.as_deref());
            if dest.is_empty() {
                continue;
            }
            snippets.push(image_syntax(&image_alt(path), &dest));
        }
        if snippets.is_empty() {
            return;
        }
        let text = snippets.join("\n\n");
        let intent = match self.paste_host() {
            Some(
                BlockKind::CodeBlock
                | BlockKind::Mermaid
                | BlockKind::Math
                | BlockKind::TableCell
                | BlockKind::Image,
            ) => PasteIntent::PlainText,
            _ => PasteIntent::IndependentFragment,
        };
        self.apply_cmd(Command::Paste { text, intent });
        self.note_edit(cx);
        cx.notify();
    }
}

fn image_alt(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image")
        .replace(['[', ']'], "")
}

fn image_syntax(alt: &str, dest: &str) -> String {
    let dest = escape_angle(dest);
    if dest
        .bytes()
        .any(|b| matches!(b, b' ' | b'\t' | b'(' | b')'))
    {
        format!("![{alt}](<{dest}>)")
    } else {
        format!("![{alt}]({dest})")
    }
}

fn escape_angle(dest: &str) -> std::borrow::Cow<'_, str> {
    if !dest.bytes().any(|b| b == b'<' || b == b'>') {
        return std::borrow::Cow::Borrowed(dest);
    }
    std::borrow::Cow::Owned(dest.replace('<', "%3C").replace('>', "%3E"))
}

#[cfg(test)]
mod syntax_tests {
    use super::image_syntax;

    #[test]
    fn plain_dest_stays_bare() {
        assert_eq!(image_syntax("cat", "a.png"), "![cat](a.png)");
    }

    #[test]
    fn spaces_go_in_angle_brackets() {
        assert_eq!(image_syntax("cat", "my pic.png"), "![cat](<my pic.png>)");
    }

    #[test]
    fn angle_characters_are_percent_escaped() {
        assert_eq!(image_syntax("cat", "a<b.png"), "![cat](a%3Cb.png)");
        assert_eq!(image_syntax("cat", "x>.png"), "![cat](x%3E.png)");
    }

    #[test]
    fn angles_and_spaces_escape_then_bracket() {
        assert_eq!(
            image_syntax("cat", "my >pic.png"),
            "![cat](<my %3Epic.png>)"
        );
    }
}

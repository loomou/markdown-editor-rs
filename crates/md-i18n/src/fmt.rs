use crate::{Lang, current};

pub fn body_size_hint(px: f32) -> String {
    body_size_hint_in(current(), px)
}

pub fn body_size_hint_in(lang: Lang, px: f32) -> String {
    let n = px.round() as i32;
    match lang {
        Lang::ZhCn => format!("当前 {n}px，标题与代码按比例跟随"),
        Lang::En => format!("Now {n}px; headings and code scale with it"),
    }
}

pub fn color_count_hint(total: usize) -> String {
    color_count_hint_in(current(), total)
}

pub fn color_count_hint_in(lang: Lang, total: usize) -> String {
    match lang {
        Lang::ZhCn => format!("界面与正文共 {total} 项颜色，明暗主题独立配置"),
        Lang::En => {
            format!("{total} colors across the app and the document, per light/dark theme")
        }
    }
}

pub fn overrides_count(n: usize) -> String {
    overrides_count_in(current(), n)
}

pub fn overrides_count_in(lang: Lang, n: usize) -> String {
    match lang {
        Lang::ZhCn => format!("这套配色改过 {n} 项"),
        Lang::En if n == 1 => "1 color changed in this theme".to_owned(),
        Lang::En => format!("{n} colors changed in this theme"),
    }
}

pub fn save_changes_question() -> (&'static str, &'static str) {
    save_changes_question_in(current())
}

pub fn save_changes_question_in(lang: Lang) -> (&'static str, &'static str) {
    match lang {
        Lang::ZhCn => ("是否保存对 ", " 的更改？"),
        Lang::En => ("Save changes to ", "?"),
    }
}

pub fn group_count(total: usize, changed: usize) -> String {
    group_count_in(current(), total, changed)
}

pub fn group_count_in(lang: Lang, total: usize, changed: usize) -> String {
    match (lang, changed) {
        (Lang::ZhCn, 0) => format!("{total} 项"),
        (Lang::ZhCn, _) => format!("{total} 项 · 改过 {changed}"),
        (Lang::En, 0) => format!("{total} colors"),
        (Lang::En, _) => format!("{total} colors · {changed} changed"),
    }
}

pub fn table_size(rows: usize, cols: usize) -> String {
    table_size_in(current(), rows, cols)
}

pub fn table_size_in(lang: Lang, rows: usize, cols: usize) -> String {
    match lang {
        Lang::ZhCn => format!("{rows} 行 × {cols} 列"),
        Lang::En => format!("{rows} × {cols}"),
    }
}

pub fn status_line(line: usize) -> String {
    status_line_in(current(), line)
}

pub fn status_line_in(lang: Lang, line: usize) -> String {
    match lang {
        Lang::ZhCn => format!("第 {line} 行"),
        Lang::En => format!("Ln {line}"),
    }
}

pub fn status_column(column: usize) -> String {
    status_column_in(current(), column)
}

pub fn status_column_in(lang: Lang, column: usize) -> String {
    match lang {
        Lang::ZhCn => format!("第 {column} 列"),
        Lang::En => format!("Col {column}"),
    }
}

pub fn status_words(words: usize) -> String {
    status_words_in(current(), words)
}

pub fn status_words_in(lang: Lang, words: usize) -> String {
    match lang {
        Lang::ZhCn => format!("{words} 词"),
        Lang::En if words == 1 => "1 word".to_owned(),
        Lang::En => format!("{words} words"),
    }
}

pub fn shortcut_overrides_count(n: usize) -> String {
    shortcut_overrides_count_in(current(), n)
}

pub fn shortcut_overrides_count_in(lang: Lang, n: usize) -> String {
    match lang {
        Lang::ZhCn => format!("改过 {n} 条键位"),
        Lang::En if n == 1 => "1 shortcut changed".to_owned(),
        Lang::En => format!("{n} shortcuts changed"),
    }
}

pub fn shortcut_taken_from(other: crate::Key) -> String {
    shortcut_taken_from_in(current(), other)
}

pub fn shortcut_taken_from_in(lang: Lang, other: crate::Key) -> String {
    let name = crate::t_in(lang, other);
    match lang {
        Lang::ZhCn => format!("原来是「{name}」的键，已取消"),
        Lang::En => format!("Taken from “{name}”, which is now unset"),
    }
}

pub fn shortcut_reserved(chord: &str) -> String {
    shortcut_reserved_in(current(), chord)
}

pub fn shortcut_reserved_in(lang: Lang, chord: &str) -> String {
    match lang {
        Lang::ZhCn => format!("{chord} 是编辑区固定用的，改不了"),
        Lang::En => format!("{chord} is fixed in the editor and cannot be rebound"),
    }
}

pub fn status_chars(chars: usize) -> String {
    status_chars_in(current(), chars)
}

pub fn status_chars_in(lang: Lang, chars: usize) -> String {
    match lang {
        Lang::ZhCn => format!("{chars} 字"),
        Lang::En if chars == 1 => "1 char".to_owned(),
        Lang::En => format!("{chars} chars"),
    }
}

pub fn about_tagline(version: &str) -> String {
    about_tagline_in(current(), version)
}

pub fn about_tagline_in(lang: Lang, version: &str) -> String {
    match lang {
        Lang::ZhCn => format!(
            "{}v{version}",
            crate::t_in(lang, crate::Key::SetAboutTagline)
        ),
        Lang::En => format!(
            "{}v{version}",
            crate::t_in(lang, crate::Key::SetAboutTagline)
        ),
    }
}

pub fn col_width(col: usize, width: i32) -> String {
    col_width_in(current(), col, width)
}

pub fn col_width_in(lang: Lang, col: usize, width: i32) -> String {
    match lang {
        Lang::ZhCn => format!("列 {col} → {width}"),
        Lang::En => format!("col {col} → {width}"),
    }
}

pub fn image_fallback_label() -> String {
    image_fallback_label_in(current())
}

pub fn image_fallback_label_in(lang: Lang) -> String {
    match lang {
        Lang::ZhCn => "▣ 图片".to_owned(),
        Lang::En => "▣ image".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        about_tagline_in, body_size_hint_in, col_width_in, color_count_hint_in, group_count_in,
        image_fallback_label_in, overrides_count_in, save_changes_question_in,
        shortcut_overrides_count_in, shortcut_reserved_in, shortcut_taken_from_in, status_chars_in,
        status_column_in, status_line_in, status_words_in, table_size_in,
    };
    use crate::Key;
    use crate::Lang;

    #[test]
    fn every_number_reaches_the_sentence_in_every_language() {
        for lang in Lang::ALL {
            let cases = [
                ("body_size_hint", body_size_hint_in(lang, 17.0), "17"),
                ("color_count_hint", color_count_hint_in(lang, 85), "85"),
                ("overrides_count", overrides_count_in(lang, 7), "7"),
                ("group_count total", group_count_in(lang, 22, 3), "22"),
                ("group_count changed", group_count_in(lang, 22, 3), "3"),
                ("group_count untouched", group_count_in(lang, 22, 0), "22"),
                ("table_size rows", table_size_in(lang, 3, 4), "3"),
                ("table_size cols", table_size_in(lang, 3, 4), "4"),
                ("status_line", status_line_in(lang, 12), "12"),
                ("status_column", status_column_in(lang, 34), "34"),
                ("status_words", status_words_in(lang, 56), "56"),
                ("status_chars", status_chars_in(lang, 78), "78"),
                (
                    "shortcut_overrides_count",
                    shortcut_overrides_count_in(lang, 9),
                    "9",
                ),
                (
                    "shortcut_reserved",
                    shortcut_reserved_in(lang, "ctrl-right"),
                    "ctrl-right",
                ),
            ];
            for (name, got, want) in cases {
                assert!(
                    got.contains(want),
                    "{name} 在 {} 下丢了参数：`{got}` 里没有 {want}",
                    lang.key()
                );
                assert!(
                    !got.contains('{'),
                    "{name} 在 {} 下漏了个没填的占位符：`{got}`",
                    lang.key()
                );
            }

            let (head, tail) = save_changes_question_in(lang);
            let whole = format!("{head}题.md{tail}");
            assert!(
                !whole.contains('{'),
                "save_changes_question 在 {} 下有个没填的占位符：`{whole}`",
                lang.key()
            );
            assert!(
                !head.is_empty() && !tail.is_empty(),
                "save_changes_question 在 {} 下有一截是空的：`{head}` / `{tail}`",
                lang.key()
            );
        }
    }

    #[test]
    fn the_sentences_are_actually_translated() {
        let pairs = [
            (
                "body_size_hint",
                body_size_hint_in(Lang::ZhCn, 16.0),
                body_size_hint_in(Lang::En, 16.0),
            ),
            (
                "color_count_hint",
                color_count_hint_in(Lang::ZhCn, 85),
                color_count_hint_in(Lang::En, 85),
            ),
            (
                "overrides_count",
                overrides_count_in(Lang::ZhCn, 3),
                overrides_count_in(Lang::En, 3),
            ),
            (
                "save_changes_question",
                save_changes_question_in(Lang::ZhCn).0.to_owned(),
                save_changes_question_in(Lang::En).0.to_owned(),
            ),
            (
                "group_count",
                group_count_in(Lang::ZhCn, 22, 3),
                group_count_in(Lang::En, 22, 3),
            ),
            (
                "group_count untouched",
                group_count_in(Lang::ZhCn, 22, 0),
                group_count_in(Lang::En, 22, 0),
            ),
            (
                "table_size",
                table_size_in(Lang::ZhCn, 3, 4),
                table_size_in(Lang::En, 3, 4),
            ),
            (
                "status_line",
                status_line_in(Lang::ZhCn, 1),
                status_line_in(Lang::En, 1),
            ),
            (
                "status_column",
                status_column_in(Lang::ZhCn, 1),
                status_column_in(Lang::En, 1),
            ),
            (
                "status_words",
                status_words_in(Lang::ZhCn, 2),
                status_words_in(Lang::En, 2),
            ),
            (
                "status_chars",
                status_chars_in(Lang::ZhCn, 2),
                status_chars_in(Lang::En, 2),
            ),
            (
                "shortcut_overrides_count",
                shortcut_overrides_count_in(Lang::ZhCn, 3),
                shortcut_overrides_count_in(Lang::En, 3),
            ),
            (
                "shortcut_taken_from",
                shortcut_taken_from_in(Lang::ZhCn, Key::Save),
                shortcut_taken_from_in(Lang::En, Key::Save),
            ),
            (
                "shortcut_reserved",
                shortcut_reserved_in(Lang::ZhCn, "ctrl-right"),
                shortcut_reserved_in(Lang::En, "ctrl-right"),
            ),
        ];
        for (name, zh, en) in pairs {
            assert_ne!(zh, en, "{name} 中英文一样，像是没翻");
        }
    }

    #[test]
    fn the_stolen_command_is_named_in_the_same_language() {
        for lang in Lang::ALL {
            let got = shortcut_taken_from_in(lang, Key::Save);
            let want = crate::t_in(lang, Key::Save);
            assert!(
                got.contains(want),
                "{} 下这一句里没有那条命令的名字（`{want}`）：`{got}`",
                lang.key()
            );

            let other = if lang == Lang::ZhCn {
                Lang::En
            } else {
                Lang::ZhCn
            };
            let stray = crate::t_in(other, Key::Save);
            assert!(
                !got.contains(stray),
                "{} 下这一句混进了另一种语言的名字（`{stray}`）：`{got}`",
                lang.key()
            );
        }
    }

    #[test]
    fn the_font_hint_shows_a_whole_number() {
        for lang in Lang::ALL {
            let s = body_size_hint_in(lang, 15.7);
            assert!(s.contains("16"), "该收成 16：`{s}`");
            assert!(!s.contains('.'), "别带小数点：`{s}`");
        }
    }

    #[test]
    fn english_counts_agree_with_their_number() {
        assert_eq!(status_words_in(Lang::En, 1), "1 word");
        assert_eq!(status_words_in(Lang::En, 2), "2 words");
        assert_eq!(status_words_in(Lang::En, 0), "0 words");
        assert_eq!(status_chars_in(Lang::En, 1), "1 char");
        assert_eq!(status_chars_in(Lang::En, 2), "2 chars");
        assert_eq!(
            shortcut_overrides_count_in(Lang::En, 1),
            "1 shortcut changed"
        );
        assert_eq!(
            shortcut_overrides_count_in(Lang::En, 2),
            "2 shortcuts changed"
        );
        assert_eq!(
            overrides_count_in(Lang::En, 1),
            "1 color changed in this theme"
        );
        assert_eq!(
            overrides_count_in(Lang::En, 2),
            "2 colors changed in this theme"
        );
        assert_eq!(status_words_in(Lang::ZhCn, 1), "1 词");
        assert_eq!(status_words_in(Lang::ZhCn, 2), "2 词");
    }

    #[test]
    fn the_about_tagline_carries_the_version_in_both_languages() {
        for lang in Lang::ALL {
            let got = about_tagline_in(lang, "0.2.0");
            assert!(
                got.contains("v0.2.0"),
                "{} 下版本串没拼进去：`{got}`",
                lang.key()
            );
            assert!(got.ends_with("v0.2.0"), "版本该在句尾：`{got}`");
            let other = if lang == Lang::ZhCn {
                Lang::En
            } else {
                Lang::ZhCn
            };
            let stray = crate::t_in(other, Key::SetAboutTagline);
            assert!(
                !got.contains(stray),
                "{} 下混进了另一种语言的前缀（`{stray}`）：`{got}`",
                lang.key()
            );
        }
    }

    #[test]
    fn col_readouts_and_image_fallback_follow_the_language() {
        for lang in Lang::ALL {
            let col = col_width_in(lang, 2, 120);
            assert!(col.contains("2") && col.contains("120"), "`{col}`");
            let fallback = image_fallback_label_in(lang);
            assert!(fallback.contains('▣'), "`{fallback}`");
        }
        assert_eq!(col_width_in(Lang::ZhCn, 2, 120), "列 2 → 120");
        assert_eq!(col_width_in(Lang::En, 2, 120), "col 2 → 120");
        assert_eq!(image_fallback_label_in(Lang::ZhCn), "▣ 图片");
        assert_eq!(image_fallback_label_in(Lang::En), "▣ image");
    }
}

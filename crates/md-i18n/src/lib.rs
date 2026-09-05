pub mod fmt;
mod key;
mod lang;

pub use key::Key;
pub use lang::{Lang, current, set_current};

pub fn t(key: Key) -> &'static str {
    match current() {
        Lang::ZhCn => key.zh_cn(),
        Lang::En => key.en(),
    }
}

pub fn t_in(lang: Lang, key: Key) -> &'static str {
    match lang {
        Lang::ZhCn => key.zh_cn(),
        Lang::En => key.en(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, Lang, t_in};

    #[test]
    fn every_key_says_something_in_every_language() {
        for lang in Lang::ALL {
            for key in Key::ALL {
                let text = t_in(lang, *key);
                assert!(
                    !text.trim().is_empty(),
                    "{} 在 {} 下是空的",
                    key.debug_name(),
                    lang.key()
                );
            }
        }
    }

    #[test]
    fn the_two_languages_are_not_the_same_table() {
        let same: Vec<&str> = Key::ALL
            .iter()
            .filter(|k| t_in(Lang::ZhCn, **k) == t_in(Lang::En, **k))
            .map(|k| k.debug_name())
            .collect();
        assert!(
            same.len() * 4 < Key::COUNT,
            "有 {} / {} 条两种语言一模一样，像是照抄的：{same:?}",
            same.len(),
            Key::COUNT
        );
    }

    #[test]
    fn no_two_keys_carry_the_same_english_label() {
        let mut seen: Vec<(&str, &str)> = Key::ALL
            .iter()
            .map(|k| (t_in(Lang::En, *k), k.debug_name()))
            .collect();
        seen.sort_unstable();
        for pair in seen.windows(2) {
            assert_ne!(
                pair[0].0, pair[1].0,
                "{} 和 {} 是同一句英文",
                pair[0].1, pair[1].1
            );
        }
    }
}

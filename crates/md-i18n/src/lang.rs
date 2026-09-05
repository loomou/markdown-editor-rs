use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    ZhCn,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::ZhCn, Lang::En];

    pub fn key(self) -> &'static str {
        match self {
            Lang::ZhCn => "zh-CN",
            Lang::En => "en",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "zh-CN" => Some(Lang::ZhCn),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    pub fn from_bcp47(tag: &str) -> Self {
        let primary = tag
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if primary == "zh" {
            Lang::ZhCn
        } else {
            Lang::En
        }
    }

    pub fn from_system() -> Self {
        sys_locale::get_locale()
            .map(|tag| Self::from_bcp47(&tag))
            .unwrap_or(Lang::En)
    }
}

static CURRENT: OnceLock<Lang> = OnceLock::new();

pub fn current() -> Lang {
    *CURRENT.get_or_init(Lang::from_system)
}

pub fn set_current(lang: Lang) -> bool {
    CURRENT.set(lang).is_ok()
}

#[cfg(test)]
mod tests {
    use super::Lang;

    #[test]
    fn every_language_round_trips_through_its_key() {
        for lang in Lang::ALL {
            assert_eq!(Lang::from_key(lang.key()), Some(lang), "{}", lang.key());
        }
        assert_eq!(Lang::from_key("klingon"), None);
    }

    #[test]
    fn chinese_is_recognized_however_the_system_spells_it() {
        for tag in [
            "zh",
            "zh-CN",
            "zh-Hans-CN",
            "zh_CN.UTF-8",
            "ZH-TW",
            "zh-Hant",
        ] {
            assert_eq!(Lang::from_bcp47(tag), Lang::ZhCn, "{tag}");
        }
        for tag in ["en", "en-US", "de-DE", "ja-JP", "", "zhh", "x-zh"] {
            assert_eq!(Lang::from_bcp47(tag), Lang::En, "{tag}");
        }
    }
}

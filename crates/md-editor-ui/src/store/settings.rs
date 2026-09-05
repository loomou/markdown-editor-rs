use crate::keymap::{Chord, Cmd, Keymap};
use crate::platform::fs_atomic::write_bytes_atomic;
use md_i18n::Lang;
use md_theme::{Appearance, BodyFamily, ColorSlot, Density, ThemeColor, ThemeVariant};
use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const VERSION: u64 = 1;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    pub appearance: Appearance,
    pub autosave: bool,
    pub remote_images: bool,
    pub language: LanguageChoice,
    pub keymap: Keymap,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LanguageChoice {
    #[default]
    System,
    Fixed(Lang),
}

impl LanguageChoice {
    pub const ALL: [LanguageChoice; 3] =
        [Self::System, Self::Fixed(Lang::ZhCn), Self::Fixed(Lang::En)];

    pub fn label(self) -> md_i18n::Key {
        match self {
            Self::System => md_i18n::Key::SetLanguageSystem,
            Self::Fixed(Lang::ZhCn) => md_i18n::Key::SetLanguageZh,
            Self::Fixed(Lang::En) => md_i18n::Key::SetLanguageEn,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Fixed(lang) => lang.key(),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "system" => Some(Self::System),
            other => Lang::from_key(other).map(Self::Fixed),
        }
    }

    pub fn resolve(self) -> Lang {
        self.resolve_with(Lang::from_system)
    }

    pub fn resolve_with(self, system: impl FnOnce() -> Lang) -> Lang {
        match self {
            Self::System => system(),
            Self::Fixed(lang) => lang,
        }
    }
}

pub fn init_language() {
    let choice = SettingsStore::discover()
        .and_then(|s| s.load())
        .map(|s| s.language)
        .unwrap_or_default();
    md_i18n::set_current(choice.resolve());
}

impl Settings {
    pub fn encode(&self) -> String {
        let a = &self.appearance;
        let mut root = Map::new();
        root.insert("version".into(), VERSION.into());
        root.insert("language".into(), self.language.key().into());
        root.insert("theme".into(), a.variant.key().into());
        root.insert("body_family".into(), a.body_family.key().into());
        root.insert("body_size".into(), (a.body_size_px.round() as i64).into());
        root.insert("density".into(), a.density.key().into());
        root.insert("autosave".into(), self.autosave.into());
        root.insert("remote_images".into(), self.remote_images.into());
        for variant in ThemeVariant::ALL {
            let overrides = &a.colors[variant.index()];
            if overrides.is_empty() {
                continue;
            }
            let mut section = Map::new();
            for (slot, color) in overrides.iter() {
                insert_path(&mut section, slot.key(), color.to_css_hex().into());
            }
            root.insert(variant.key().into(), Value::Object(section));
        }

        let mut keys = Map::new();
        for (cmd, chord) in self.keymap.overrides() {
            let v = match chord {
                Some(c) => Value::String(c.unparse()),
                None => Value::Null,
            };
            keys.insert(cmd.key().into(), v);
        }
        if !keys.is_empty() {
            root.insert("keys".into(), Value::Object(keys));
        }
        let mut text = serde_json::to_string_pretty(&Value::Object(root))
            .expect("a tree made of maps, strings, and numbers always serializes");
        text.push('\n');
        text
    }

    pub fn parse(text: &str) -> Option<Self> {
        let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text) else {
            return None;
        };
        if root.get("version").and_then(Value::as_u64) != Some(VERSION) {
            return None;
        }

        let mut out = Self::default();
        if let Some(l) = root
            .get("language")
            .and_then(Value::as_str)
            .and_then(LanguageChoice::from_key)
        {
            out.language = l;
        }
        if let Some(v) = root
            .get("theme")
            .and_then(Value::as_str)
            .and_then(ThemeVariant::from_key)
        {
            out.appearance.variant = v;
        }
        if let Some(f) = root
            .get("body_family")
            .and_then(Value::as_str)
            .and_then(BodyFamily::from_key)
        {
            out.appearance.body_family = f;
        }

        if let Some(px) = root.get("body_size").and_then(Value::as_f64) {
            out.appearance = out.appearance.with_body_size_px(px as f32);
        }
        if let Some(d) = root
            .get("density")
            .and_then(Value::as_str)
            .and_then(Density::from_key)
        {
            out.appearance.density = d;
        }
        if let Some(v) = root.get("autosave").and_then(Value::as_bool) {
            out.autosave = v;
        }
        if let Some(v) = root.get("remote_images").and_then(Value::as_bool) {
            out.remote_images = v;
        }

        for variant in ThemeVariant::ALL {
            let Some(Value::Object(section)) = root.get(variant.key()) else {
                continue;
            };
            let dst = &mut out.appearance.colors[variant.index()];
            for slot in ColorSlot::ALL {
                let Some(raw) = lookup_path(section, slot.key()).and_then(Value::as_str) else {
                    continue;
                };
                if let Some(color) = ThemeColor::from_css_hex(raw) {
                    dst.set(*slot, color);
                }
            }
        }

        if let Some(Value::Object(keys)) = root.get("keys") {
            for (name, raw) in keys {
                let Some(cmd) = Cmd::from_key(name) else {
                    continue;
                };
                match raw {
                    Value::Null => out.keymap.put(cmd, None),
                    Value::String(text) => {
                        if let Some(chord) = Chord::parse(text) {
                            out.keymap.put(cmd, Some(chord));
                        }
                    }
                    _ => {}
                }
            }
        }
        Some(out)
    }
}

fn insert_path(into: &mut Map<String, Value>, path: &str, value: Value) {
    let mut cur = into;
    let mut it = path.split('.').peekable();
    while let Some(seg) = it.next() {
        if it.peek().is_none() {
            cur.insert(seg.to_string(), value);
            return;
        }
        let next = cur
            .entry(seg.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !next.is_object() {
            *next = Value::Object(Map::new());
        }
        cur = next
            .as_object_mut()
            .expect("the line above just ensured it is an object");
    }
}

fn lookup_path<'a>(from: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut cur = from;
    let mut it = path.split('.').peekable();
    while let Some(seg) = it.next() {
        let next = cur.get(seg)?;
        if it.peek().is_none() {
            return Some(next);
        }
        cur = next.as_object()?;
    }
    None
}

#[derive(Clone, Debug)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn discover() -> Option<Self> {
        Some(Self::at(crate::platform::paths::settings_path()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Option<Settings> {
        Settings::parse(&fs::read_to_string(&self.path).ok()?)
    }

    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        write_bytes_atomic(&self.path, settings.encode().as_bytes())
    }
}

pub fn load_or_default() -> Settings {
    SettingsStore::discover()
        .and_then(|s| s.load())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{Chord, Cmd, LanguageChoice, Settings, SettingsStore};
    use md_i18n::Lang;
    use md_theme::{BodyFamily, ColorSlot, Density, ThemeColor, ThemeVariant};
    use serde_json::Value;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn hex(h: &str) -> ThemeColor {
        ThemeColor::from_css_hex(h).expect("test hex must be valid")
    }

    fn unique_path(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("md-test-settings-{tag}-{}-{n}", std::process::id()));
        p.push("settings.json");
        p
    }

    fn cleanup(path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    fn chord(text: &str) -> Chord {
        Chord::parse(text).expect("test chords must be valid")
    }

    fn non_default() -> Settings {
        let mut s = Settings::default();

        s.keymap
            .set(Cmd::Find, chord("ctrl-alt-k"))
            .expect("must be a legal binding");
        s.keymap.clear(Cmd::Save);
        s.appearance.variant = ThemeVariant::OneLight;
        s.appearance.body_family = BodyFamily::Serif;
        s.appearance.density = Density::Relaxed;
        s.appearance = s.appearance.with_body_size_px(19.0);
        s.autosave = true;
        s.remote_images = true;
        s.language = LanguageChoice::Fixed(Lang::En);
        s.appearance.colors[ThemeVariant::OneLight.index()].set(ColorSlot::Canvas, hex("#fff8e7"));
        s.appearance.colors[ThemeVariant::OneLight.index()]
            .set(ColorSlot::SynKeyword, hex("#a449ab"));
        s.appearance.colors[ThemeVariant::OneDark.index()].set(ColorSlot::Body, hex("#010203"));
        s.appearance.colors[ThemeVariant::OneDark.index()].set(ColorSlot::AppBarBg, hex("#101418"));
        s
    }

    fn minimal() -> Value {
        serde_json::json!({ "version": 1 })
    }

    #[test]
    fn the_file_reads_like_something_a_person_would_edit() {
        let mut s = Settings::default();
        s.appearance.colors[ThemeVariant::OneDark.index()].set(ColorSlot::Canvas, hex("#1d2027"));
        s.appearance.colors[ThemeVariant::OneDark.index()]
            .set(ColorSlot::SynKeyword, hex("#b477cf"));
        s.appearance.colors[ThemeVariant::OneDark.index()].set(ColorSlot::AppBarBg, hex("#3b414d"));
        assert_eq!(
            s.encode(),
            concat!(
                "{\n",
                "  \"version\": 1,\n",
                "  \"language\": \"system\",\n",
                "  \"theme\": \"one-dark\",\n",
                "  \"body_family\": \"sans\",\n",
                "  \"body_size\": 16,\n",
                "  \"density\": \"normal\",\n",
                "  \"autosave\": false,\n",
                "  \"remote_images\": false,\n",
                "  \"one-dark\": {\n",
                "    \"editor\": {\n",
                "      \"canvas\": \"#1d2027\",\n",
                "      \"syntax\": {\n",
                "        \"keyword\": \"#b477cf\"\n",
                "      }\n",
                "    },\n",
                "    \"app\": {\n",
                "      \"bar_bg\": \"#3b414d\"\n",
                "    }\n",
                "  }\n",
                "}\n",
            )
        );
    }

    #[test]
    fn every_field_round_trips_through_the_text() {
        let s = non_default();
        assert_eq!(Settings::parse(&s.encode()), Some(s));
        let d = Settings::default();
        assert_eq!(Settings::parse(&d.encode()), Some(d));
    }

    #[test]
    fn a_version_it_does_not_know_is_refused_whole() {
        let text = non_default().encode();
        assert_eq!(
            Settings::parse(&text.replace("\"version\": 1", "\"version\": 2")),
            None
        );
        assert_eq!(Settings::parse(r#"{"theme": "one-light"}"#), None);

        assert_eq!(Settings::parse(r#"{"version": "1"}"#), None);
        assert_eq!(Settings::parse("{}"), None);
        assert_eq!(Settings::parse(""), None);
    }

    #[test]
    fn a_file_that_is_not_json_at_all_reads_as_nothing() {
        for text in [
            r#"{"version": 1,}"#,
            r#"{"version": 1 "theme":"#,
            "[1, 2, 3]",
            r#""just a string""#,
            "v=1\ntheme=one-light\n",
        ] {
            assert_eq!(Settings::parse(text), None, "{text}");
        }
    }

    #[test]
    fn unknown_keys_inside_a_known_version_are_skipped() {
        let mut root = minimal();
        root["theme"] = "one-light".into();
        root["cursor_blink"] = "never".into();

        root["gruvbox"] = serde_json::json!({ "editor": { "canvas": "#000000" } });
        let parsed = Settings::parse(&root.to_string()).expect("version 1 must be recognized");
        assert_eq!(parsed.appearance.variant, ThemeVariant::OneLight);
        for v in ThemeVariant::ALL {
            assert!(parsed.appearance.colors[v.index()].is_empty());
        }
    }

    #[test]
    fn a_retired_color_slot_key_is_skipped_without_touching_its_neighbors() {
        let text = r##"{
            "version": 1,
            "one-dark": {
                "editor": {
                    "caption": "#a9afbc",
                    "canvas": "#282c33",
                    "image_alt": "#878a98"
                }
            }
        }"##;
        let parsed = Settings::parse(text).expect("version must match");
        let dark = &parsed.appearance.colors[ThemeVariant::OneDark.index()];
        assert_eq!(
            dark.get(ColorSlot::Canvas),
            Some(hex("#282c33")),
            "skipping the caption row must not affect the two after it"
        );
        assert_eq!(dark.get(ColorSlot::ImageAlt), Some(hex("#878a98")));
        assert_eq!(dark.len(), 2);
    }

    #[test]
    fn a_bad_value_only_costs_its_own_field() {
        let text = r#"{
            "version": 1,
            "theme": "one-light",
            "density": "nrmal",
            "body_size": "huge",
            "autosave": "yes",
            "body_family": 7
        }"#;
        let parsed = Settings::parse(text).expect("version must match");
        let d = Settings::default();
        assert_eq!(parsed.appearance.variant, ThemeVariant::OneLight);
        assert_eq!(parsed.appearance.density, Density::default());
        assert_eq!(parsed.appearance.body_size_px, d.appearance.body_size_px);
        assert_eq!(parsed.autosave, d.autosave);
        assert_eq!(parsed.appearance.body_family, d.appearance.body_family);
    }

    #[test]
    fn the_language_choice_round_trips_including_follow_the_system() {
        for choice in [
            LanguageChoice::System,
            LanguageChoice::Fixed(Lang::ZhCn),
            LanguageChoice::Fixed(Lang::En),
        ] {
            let s = Settings {
                language: choice,
                ..Default::default()
            };
            let text = s.encode();
            assert_eq!(
                Settings::parse(&text).map(|s| s.language),
                Some(choice),
                "{text}"
            );
            assert_eq!(LanguageChoice::from_key(choice.key()), Some(choice));
        }

        let text = r#"{"version": 1, "language": "klingon", "theme": "one-light"}"#;
        let parsed = Settings::parse(text).expect("version must match");
        assert_eq!(parsed.language, LanguageChoice::System);
        assert_eq!(parsed.appearance.variant, ThemeVariant::OneLight);
        assert_eq!(
            LanguageChoice::from_key("zh"),
            None,
            "only whole labels are recognized"
        );
    }

    #[test]
    fn a_fixed_choice_does_not_ask_the_system() {
        use std::cell::Cell;
        let asked = Cell::new(0u32);
        let probe = || {
            asked.set(asked.get() + 1);
            Lang::ZhCn
        };

        assert_eq!(LanguageChoice::System.resolve_with(probe), Lang::ZhCn);
        assert_eq!(
            asked.get(),
            1,
            "\"follow system\" must really ask the system"
        );

        assert_eq!(
            LanguageChoice::Fixed(Lang::En).resolve_with(probe),
            Lang::En,
            "whoever hardcoded en must still see English on a Chinese system"
        );
        assert_eq!(asked.get(), 1, "a fixed choice must not ask the system");
        for lang in Lang::ALL {
            assert_eq!(LanguageChoice::Fixed(lang).resolve_with(probe), lang);
        }
        assert_eq!(asked.get(), 1);

        assert_eq!(LanguageChoice::System.resolve(), Lang::from_system());
    }

    #[test]
    fn an_out_of_range_size_is_clamped_on_read() {
        for (raw, want) in [("200", 24.0), ("1", 12.0), ("15.4", 15.0)] {
            let text = format!(r#"{{"version": 1, "body_size": {raw}}}"#);
            let parsed = Settings::parse(&text).expect("version must match");
            assert_eq!(parsed.appearance.body_size_px, want, "{raw}");
        }
    }

    #[test]
    fn a_settings_file_without_overrides_stays_short() {
        let text = Settings::default().encode();
        let Some(Value::Object(root)) = serde_json::from_str(&text).ok() else {
            panic!("{text}");
        };
        assert_eq!(root.len(), 8, "{text}");
        for v in ThemeVariant::ALL {
            assert!(!root.contains_key(v.key()), "{text}");
        }
        assert!(text.ends_with('\n'), "must end with a newline");
    }

    #[test]
    fn each_variants_colors_are_written_under_its_own_name() {
        let text = non_default().encode();
        let root: Value = serde_json::from_str(&text).expect("{text}");
        assert_eq!(root["one-light"]["editor"]["canvas"], "#fff8e7", "{text}");
        assert_eq!(
            root["one-light"]["editor"]["syntax"]["keyword"], "#a449ab",
            "{text}"
        );
        assert_eq!(root["one-dark"]["editor"]["body"], "#010203", "{text}");

        assert_eq!(root["one-dark"]["app"]["bar_bg"], "#101418", "{text}");

        assert!(root["one-light"]["editor"]["body"].is_null(), "{text}");

        for (_, section) in root.as_object().expect("must be an object") {
            if let Some(map) = section.as_object() {
                for k in map.keys() {
                    assert!(!k.contains('.'), "{k} was not expanded into nested objects");
                }
            }
        }
    }

    #[test]
    fn a_color_entry_it_cannot_place_is_skipped_alone() {
        let text = r##"{
            "version": 1,
            "one-dark": {
                "editor": {
                    "no_such_slot": "#000000",
                    "canvas": "not-a-color",
                    "syntax": "should-be-an-object",
                    "caret": "#ff8800"
                },
                "no_such_group": { "whatever": "#000000" },
                "app": { "bar_bg": 42 }
            }
        }"##;
        let parsed = Settings::parse(text).expect("version must match");
        let dark = &parsed.appearance.colors[ThemeVariant::OneDark.index()];
        assert_eq!(dark.get(ColorSlot::Caret), Some(hex("#ff8800")));
        assert_eq!(dark.len(), 1, "only that one entry should remain");
        assert!(parsed.appearance.colors[ThemeVariant::OneLight.index()].is_empty());
    }

    #[test]
    fn the_order_of_the_keys_does_not_matter() {
        let colors = r##""one-light": {"editor": {"canvas": "#fff8e7"}}"##;
        let before = format!(r#"{{"version": 1, {colors}, "theme": "one-light"}}"#);
        let after = format!(r#"{{"version": 1, "theme": "one-light", {colors}}}"#);
        assert_eq!(Settings::parse(&before), Settings::parse(&after));
        let parsed = Settings::parse(&before).expect("version must match");
        assert_eq!(
            parsed.appearance.colors().get(ColorSlot::Canvas),
            Some(hex("#fff8e7"))
        );
    }

    #[test]
    fn every_single_slot_survives_the_round_trip() {
        let mut s = Settings::default();
        let dark = ThemeVariant::OneDark.index();

        for (i, slot) in ColorSlot::ALL.iter().enumerate() {
            let v = 0x10_00_00 + i as u32 * 0x01_01_01;
            s.appearance.colors[dark].set(*slot, hex(&format!("#{v:06x}")));
        }
        assert_eq!(s.appearance.colors[dark].len(), ColorSlot::COUNT);
        let back = Settings::parse(&s.encode()).expect("must read back what we wrote");
        assert_eq!(back, s);
        for slot in ColorSlot::ALL {
            assert_eq!(
                back.appearance.colors[dark].get(*slot),
                s.appearance.colors[dark].get(*slot),
                "{} did not come back",
                slot.key()
            );
        }
    }

    #[test]
    fn a_pasted_syntax_block_lands_as_is() {
        let text = r##"{
            "version": 1,
            "one-dark": {
                "editor": {
                    "syntax": {
                        "keyword": "#c678dd",
                        "string": "#98c379",
                        "comment": "#5c6370",
                        "type": "#e5c07b"
                    }
                }
            }
        }"##;
        let parsed = Settings::parse(text).expect("version must match");
        let dark = &parsed.appearance.colors[ThemeVariant::OneDark.index()];
        assert_eq!(dark.get(ColorSlot::SynKeyword), Some(hex("#c678dd")));
        assert_eq!(dark.get(ColorSlot::SynString), Some(hex("#98c379")));
        assert_eq!(dark.get(ColorSlot::SynComment), Some(hex("#5c6370")));
        assert_eq!(dark.get(ColorSlot::SynType), Some(hex("#e5c07b")));
        assert_eq!(dark.len(), 4, "only these four should be recognized");
    }

    #[test]
    fn the_keys_section_only_holds_what_changed() {
        let text = Settings::default().encode();
        assert!(
            !text.contains("keys"),
            "nothing changed, so this section must not exist: {text}"
        );

        let mut s = Settings::default();
        s.keymap
            .set(Cmd::ToggleOutline, chord("ctrl-alt-o"))
            .expect("must be legal");
        let root: Value = serde_json::from_str(&s.encode()).expect("must be JSON");
        assert_eq!(root["keys"]["toggle_outline"], "ctrl-alt-o");
        assert_eq!(
            root["keys"].as_object().expect("must be an object").len(),
            1,
            "only that one should be written"
        );
    }

    #[test]
    fn an_unbound_command_stays_unbound_across_a_restart() {
        let mut s = Settings::default();
        s.keymap.clear(Cmd::Find);
        let text = s.encode();
        let root: Value = serde_json::from_str(&text).expect("must be JSON");
        assert!(root["keys"]["find"].is_null(), "{text}");
        assert!(
            text.contains("\"find\": null"),
            "must really write out the word null: {text}"
        );

        let back = Settings::parse(&text).expect("version must match");
        assert!(
            back.keymap.chord_for(Cmd::Find).is_none(),
            "a cleared binding must stay cleared"
        );
        assert_eq!(back, s);

        let none = Settings::parse(r#"{"version": 1, "keys": {}}"#).expect("version must match");
        assert_eq!(
            none.keymap.chord_for(Cmd::Find),
            Cmd::Find.default_chord().as_ref(),
            "an absent row must keep the default chord"
        );
    }

    #[test]
    fn a_key_entry_it_cannot_read_is_skipped_alone() {
        let text = r#"{
            "version": 1,
            "keys": {
                "no_such_command": "ctrl-alt-j",
                "undo": "ctrl+alt+z",
                "redo": 7,
                "quit": ["ctrl-alt-q"],
                "open": "ctrl-alt-p"
            }
        }"#;
        let parsed = Settings::parse(text).expect("version must match");

        assert_eq!(
            parsed.keymap.chord_for(Cmd::Open),
            Some(&chord("ctrl-alt-p"))
        );

        for cmd in [Cmd::Undo, Cmd::Redo, Cmd::Quit] {
            assert_eq!(
                parsed.keymap.chord_for(cmd),
                cmd.default_chord().as_ref(),
                "{} must fall back to its default chord",
                cmd.key()
            );
        }

        let changed: Vec<&str> = parsed.keymap.overrides().map(|(c, _)| c.key()).collect();
        assert_eq!(changed, ["open"]);
    }

    #[test]
    fn two_commands_written_onto_one_key_both_survive_the_read() {
        let text = r#"{
            "version": 1,
            "keys": { "find": "ctrl-alt-d", "save": "ctrl-alt-d" }
        }"#;
        let parsed = Settings::parse(text).expect("version must match");
        let want = chord("ctrl-alt-d");
        assert_eq!(parsed.keymap.chord_for(Cmd::Find), Some(&want));
        assert_eq!(parsed.keymap.chord_for(Cmd::Save), Some(&want));
    }

    #[test]
    fn a_hand_written_file_may_bind_what_the_settings_page_would_refuse() {
        use crate::keymap::Mods;
        let reserved = Chord::new(Mods::primary(), "right");
        let text = format!(
            r#"{{"version": 1, "keys": {{"find": "{}"}}}}"#,
            reserved.unparse()
        );
        let parsed = Settings::parse(&text).expect("version must match");
        assert_eq!(
            parsed.keymap.chord_for(Cmd::Find),
            Some(&reserved),
            "the file must be honored as written"
        );

        let mut page = Settings::default().keymap;
        assert!(page.set(Cmd::Find, reserved).is_err());
    }

    #[test]
    fn every_single_command_survives_the_round_trip() {
        let mut s = Settings::default();

        for (i, cmd) in Cmd::ALL.iter().enumerate() {
            let key = format!("ctrl-alt-f{}", i + 1);
            s.keymap
                .set(*cmd, Chord::parse(&key).expect("f1–f24 must all parse"))
                .unwrap_or_else(|e| panic!("{key} must be bindable: {e:?}"));
        }
        assert_eq!(s.keymap.overrides().count(), Cmd::COUNT);
        let text = s.encode();
        let back = Settings::parse(&text).expect("must read back what we wrote");
        assert_eq!(back, s, "{text}");
        for (i, cmd) in Cmd::ALL.iter().enumerate() {
            assert_eq!(
                back.keymap.chord_for(*cmd),
                Some(&chord(&format!("ctrl-alt-f{}", i + 1))),
                "{} did not come back",
                cmd.key()
            );
        }
    }

    #[test]
    fn save_then_load_survives_a_missing_directory() {
        let path = unique_path("round");
        cleanup(&path);
        let store = SettingsStore::at(path.clone());
        assert_eq!(
            store.load(),
            None,
            "nothing written yet must read back as nothing"
        );
        let want = non_default();
        store.save(&want).expect("save");
        assert_eq!(store.load(), Some(want));
        cleanup(&path);
    }

    #[test]
    fn a_corrupt_file_reads_as_nothing_rather_than_panicking() {
        let path = unique_path("corrupt");
        cleanup(&path);
        std::fs::create_dir_all(path.parent().expect("must have a parent directory"))
            .expect("mkdir");
        std::fs::write(&path, b"\x00\x01not a config at all").expect("write");
        assert_eq!(SettingsStore::at(path.clone()).load(), None);
        cleanup(&path);
    }

    fn full_example() -> Settings {
        let mut s = Settings::default();
        for variant in ThemeVariant::ALL {
            let theme = variant.document_theme();
            let dst = &mut s.appearance.colors[variant.index()];
            for slot in ColorSlot::ALL {
                dst.set(*slot, slot.read(&theme));
            }
        }
        s.keymap
            .set(Cmd::ToggleOutline, chord("ctrl-alt-o"))
            .expect("must be legal");
        s.keymap
            .set(Cmd::ToggleTheme, chord("ctrl-alt-t"))
            .expect("must be legal");
        s.keymap.clear(Cmd::Quit);
        s
    }

    #[test]
    fn the_documented_example_parses_back_completely() {
        let want = full_example();
        let text = want.encode();
        assert_eq!(Settings::parse(&text), Some(want.clone()), "{text}");
        for variant in ThemeVariant::ALL {
            assert_eq!(
                want.appearance.colors[variant.index()].len(),
                ColorSlot::COUNT,
                "the {} section must list every entry",
                variant.key()
            );
        }
    }
}

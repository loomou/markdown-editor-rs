use gpui::{Keystroke, Modifiers, SharedString};
use md_core::document::TableOp;
use md_i18n::Key;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
}

impl Mods {
    pub const fn primary() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self {
                platform: true,
                ctrl: false,
                alt: false,
                shift: false,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Self {
                ctrl: true,
                alt: false,
                shift: false,
                platform: false,
            }
        }
    }

    pub const fn primary_shift() -> Self {
        let mut m = Self::primary();
        m.shift = true;
        m
    }

    pub const fn shift() -> Self {
        Self {
            shift: true,
            ctrl: false,
            alt: false,
            platform: false,
        }
    }

    pub const fn none() -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            platform: false,
        }
    }

    pub fn has_non_shift(self) -> bool {
        self.ctrl || self.alt || self.platform
    }
}

impl From<Modifiers> for Mods {
    fn from(m: Modifiers) -> Self {
        Self {
            ctrl: m.control,
            alt: m.alt,
            shift: m.shift,
            platform: m.platform,
        }
    }
}

impl From<Mods> for Modifiers {
    fn from(m: Mods) -> Self {
        Modifiers {
            control: m.ctrl,
            alt: m.alt,
            shift: m.shift,
            platform: m.platform,
            function: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub mods: Mods,
    pub key: SharedString,
}

impl Chord {
    pub fn new(mods: Mods, key: &'static str) -> Self {
        Self {
            mods,
            key: SharedString::new_static(key),
        }
    }

    pub fn from_keystroke(k: &Keystroke) -> Option<Self> {
        plausible_key_name(&k.key).then(|| Self {
            mods: k.modifiers.into(),
            key: SharedString::new(k.key.to_ascii_lowercase()),
        })
    }

    pub fn unparse(&self) -> String {
        let mut s = String::new();
        if self.mods.ctrl {
            s.push_str("ctrl-");
        }
        if self.mods.alt {
            s.push_str("alt-");
        }
        if self.mods.platform {
            #[cfg(target_os = "macos")]
            s.push_str("cmd-");
            #[cfg(not(target_os = "macos"))]
            s.push_str("win-");
        }
        if self.mods.shift {
            s.push_str("shift-");
        }
        s.push_str(&self.key);
        s
    }

    pub fn display(&self) -> String {
        #[cfg(target_os = "macos")]
        {
            self.display_mac()
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.unparse()
        }
    }

    #[cfg(target_os = "macos")]
    fn display_mac(&self) -> String {
        let mut s = String::new();
        if self.mods.ctrl {
            s.push('\u{2303}');
        }
        if self.mods.alt {
            s.push('\u{2325}');
        }
        if self.mods.shift {
            s.push('\u{21e7}');
        }
        if self.mods.platform {
            s.push('\u{2318}');
        }
        s.push_str(&display_key(&self.key));
        s
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::from_keystroke(&Keystroke::parse(text).ok()?)
    }

    pub fn matches(&self, k: &Keystroke) -> bool {
        Mods::from(k.modifiers) == self.mods && k.key.eq_ignore_ascii_case(&self.key)
    }
}

#[cfg(target_os = "macos")]
fn display_key(key: &str) -> String {
    match key {
        "enter" | "return" => "\u{21a9}".into(),
        "tab" => "\u{21e5}".into(),
        "escape" => "\u{238b}".into(),
        "backspace" => "\u{232b}".into(),
        "delete" => "\u{2326}".into(),
        "up" => "\u{2191}".into(),
        "down" => "\u{2193}".into(),
        "left" => "\u{2190}".into(),
        "right" => "\u{2192}".into(),
        "space" => "Space".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => {
                    let mut s: String = c.to_uppercase().collect();
                    s.extend(chars);
                    s
                }
                None => String::new(),
            }
        }
    }
}

macro_rules! commands {
    ($( $variant:ident $key:literal $label:ident $win:tt $mac:tt )*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(usize)]
        pub enum Cmd { $($variant,)* }

        impl Cmd {
            pub const ALL: &'static [Cmd] = &[$(Self::$variant,)*];

            pub const COUNT: usize = Self::ALL.len();

            pub fn key(self) -> &'static str {
                match self { $(Self::$variant => $key,)* }
            }

            pub fn from_key(key: &str) -> Option<Self> {
                match key {
                    $($key => Some(Self::$variant),)*
                    _ => None,
                }
            }

            pub fn label(self) -> Key {
                match self { $(Self::$variant => Key::$label,)* }
            }

            pub fn debug_name(self) -> &'static str {
                match self { $(Self::$variant => stringify!($variant),)* }
            }

            fn index(self) -> usize {
                self as usize
            }

            pub fn default_chord(self) -> Option<Chord> {
                match self {
                    $(Self::$variant => default_chord!($win $mac),)*
                }
            }
        }
    };
}

macro_rules! default_chord {
    (- -) => {
        None
    };
    (($wm:expr, $wk:expr) ($mm:expr, $mk:expr)) => {{
        #[cfg(target_os = "macos")]
        {
            Some(Chord::new($mm, $mk))
        }
        #[cfg(not(target_os = "macos"))]
        {
            Some(Chord::new($wm, $wk))
        }
    }};
}

commands! {
    New         "new"          MenuNew         (Mods::primary(), "n")       (Mods::primary(), "n")
    Open        "open"         MenuOpen        (Mods::primary(), "o")       (Mods::primary(), "o")
    Save        "save"         Save            (Mods::primary(), "s")       (Mods::primary(), "s")
    SaveAs      "save_as"      MenuSaveAs      (Mods::primary_shift(), "s") (Mods::primary_shift(), "s")
    Quit        "quit"         MenuExit        (Mods::primary(), "q")       (Mods::primary(), "q")

    Undo        "undo"         MenuUndo        (Mods::primary(), "z")       (Mods::primary(), "z")
    Redo        "redo"         MenuRedo        (Mods::primary(), "y")       (Mods::primary_shift(), "z")
    Cut         "cut"          MenuCut         (Mods::primary(), "x")       (Mods::primary(), "x")
    Copy        "copy"         MenuCopy        (Mods::primary(), "c")       (Mods::primary(), "c")
    Paste       "paste"        MenuPaste       (Mods::primary(), "v")       (Mods::primary(), "v")
    SelectAll   "select_all"   MenuSelectAll   (Mods::primary(), "a")       (Mods::primary(), "a")

    Find        "find"         MenuFind        (Mods::primary(), "f")       (Mods::primary(), "f")
    FindNext    "find_next"    CmdFindNext     (Mods::none(), "f3")         (Mods::primary(), "g")
    FindPrev    "find_prev"    CmdFindPrev     (Mods::shift(), "f3")        (Mods::primary_shift(), "g")

    InsertTable "insert_table" MenuInsertTable (Mods::primary(), "t")       (Mods::primary(), "t")
    TableRowBelow "table_row_below" TableInsertRowBelow
                                               (Mods::primary(), "enter")   (Mods::primary(), "enter")

    ToggleOutline "toggle_outline" MenuOutline  -                            -
    ToggleTheme   "toggle_theme"   Theme        -                            -
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    NeedsModifier,
    Reserved(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    Bare,
    WithShift,
    WithShiftAndPrimary,
    Primary,
}

const RESERVED: &[(&str, Reach)] = &[
    ("left", Reach::WithShiftAndPrimary),
    ("right", Reach::WithShiftAndPrimary),
    ("up", Reach::WithShiftAndPrimary),
    ("down", Reach::WithShiftAndPrimary),
    ("home", Reach::WithShiftAndPrimary),
    ("end", Reach::WithShiftAndPrimary),
    ("pageup", Reach::WithShift),
    ("pagedown", Reach::WithShift),
    ("backspace", Reach::WithShiftAndPrimary),
    ("delete", Reach::WithShiftAndPrimary),
    ("enter", Reach::WithShift),
    ("tab", Reach::WithShift),
    ("[", Reach::Primary),
    ("]", Reach::Primary),
    ("escape", Reach::Bare),
    ("f5", Reach::Bare),
    ("f6", Reach::Bare),
];

impl Reach {
    fn mods(self) -> Vec<Mods> {
        let primary = Mods::primary();
        if self == Reach::Primary {
            return vec![primary];
        }
        let mut out = vec![Mods::none(), Mods::shift()];
        if self == Reach::Bare {
            out.truncate(1);
            return out;
        }
        if self == Reach::WithShiftAndPrimary {
            out.push(primary);
            out.push(Mods::primary_shift());
            #[cfg(target_os = "macos")]
            {
                out.push(Mods {
                    alt: true,
                    ..Mods::none()
                });
                out.push(Mods {
                    alt: true,
                    shift: true,
                    ..Mods::none()
                });
            }
        }
        out
    }
}

fn plausible_key_name(key: &str) -> bool {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(_), None) => true,
        (Some(_), Some(_)) => key.chars().all(|c| c.is_alphanumeric()),
        _ => false,
    }
}

fn is_printable(key: &str) -> bool {
    Keystroke {
        modifiers: Modifiers::default(),
        key: key.to_owned(),
        key_char: None,
    }
    .with_simulated_ime()
    .key_char
    .is_some()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    bound: [Option<Chord>; Cmd::COUNT],
}

impl Default for Keymap {
    fn default() -> Self {
        let mut bound = [const { None }; Cmd::COUNT];
        for cmd in Cmd::ALL {
            bound[cmd.index()] = cmd.default_chord();
        }
        Self { bound }
    }
}

impl Keymap {
    pub fn chord_for(&self, cmd: Cmd) -> Option<&Chord> {
        self.bound[cmd.index()].as_ref()
    }

    pub fn lookup(&self, k: &Keystroke) -> Option<Cmd> {
        Cmd::ALL
            .iter()
            .copied()
            .find(|cmd| self.chord_for(*cmd).is_some_and(|c| c.matches(k)))
    }

    pub fn set(&mut self, cmd: Cmd, chord: Chord) -> Result<Option<Cmd>, Refusal> {
        for (key, reach) in RESERVED {
            if !chord.key.eq_ignore_ascii_case(key) {
                continue;
            }
            if reach.mods().contains(&chord.mods) {
                return Err(Refusal::Reserved(chord.unparse()));
            }
        }
        if is_printable(&chord.key) && !chord.mods.has_non_shift() {
            return Err(Refusal::NeedsModifier);
        }
        let taken = Cmd::ALL
            .iter()
            .copied()
            .find(|c| *c != cmd && self.chord_for(*c) == Some(&chord));
        if let Some(other) = taken {
            self.bound[other.index()] = None;
        }
        self.bound[cmd.index()] = Some(chord);
        Ok(taken)
    }

    pub fn clear(&mut self, cmd: Cmd) {
        self.bound[cmd.index()] = None;
    }

    pub fn reset_all(&mut self) {
        *self = Self::default();
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn overrides(&self) -> impl Iterator<Item = (Cmd, Option<&Chord>)> {
        Cmd::ALL.iter().copied().filter_map(|cmd| {
            let now = self.chord_for(cmd);
            (now != cmd.default_chord().as_ref()).then_some((cmd, now))
        })
    }

    pub(crate) fn put(&mut self, cmd: Cmd, chord: Option<Chord>) {
        self.bound[cmd.index()] = chord;
    }
}

pub fn table_op_chord_cmd(op: TableOp) -> Option<Cmd> {
    match op {
        TableOp::InsertRowBelow => Some(Cmd::TableRowBelow),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::Cmd;
    use super::{Chord, Keymap, Mods, RESERVED, Refusal, is_printable};
    use gpui::{Keystroke, Modifiers};

    fn keystroke(text: &str) -> Keystroke {
        Keystroke::parse(text).expect("test keystrokes must parse")
    }

    #[test]
    fn the_written_form_is_the_one_gpui_reads_back() {
        for cmd in Cmd::ALL {
            let Some(chord) = cmd.default_chord() else {
                continue;
            };
            let text = chord.unparse();
            let k = Keystroke::parse(&text).unwrap_or_else(|e| {
                panic!(
                    "{} wrote `{text}`, but gpui does not accept it: {e}",
                    cmd.key()
                )
            });
            assert_eq!(Mods::from(k.modifiers), chord.mods, "{text}");
            assert_eq!(k.key, chord.key.as_ref(), "{text}");
            assert_eq!(Chord::parse(&text).as_ref(), Some(&chord), "{text}");
        }
    }

    #[test]
    fn display_is_glyphs_on_mac_and_unparse_elsewhere() {
        let save = Chord::new(Mods::primary(), "s");
        let save_as = Chord::new(Mods::primary_shift(), "s");
        let next = Cmd::FindNext
            .default_chord()
            .expect("find next must have a default key");
        #[cfg(target_os = "macos")]
        {
            assert_eq!(save.display(), "\u{2318}S", "save should display as ⌘S");
            assert_eq!(
                save_as.display(),
                "\u{21e7}\u{2318}S",
                "save as should display as ⇧⌘S"
            );
            assert_eq!(
                next.display(),
                "\u{2318}G",
                "find next should display as ⌘G"
            );
            assert_eq!(
                Chord::new(Mods::none(), "f3").display(),
                "F3",
                "a function key with no modifiers should still display as letters"
            );
            assert_ne!(
                save.display(),
                save.unparse(),
                "on mac, the display and the on-disk form should not be the same string"
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(save.display(), save.unparse());
            assert_eq!(save_as.display(), save_as.unparse());
            assert_eq!(next.unparse(), "f3");
            assert_eq!(next.display(), "f3");
        }
    }

    #[test]
    fn probe_chords_used_in_width_tests_differ_from_factory_display() {
        let cases = [(Cmd::TableRowBelow, "ctrl-f9"), (Cmd::Save, "ctrl-alt-k")];
        for (cmd, probe) in cases {
            let factory = cmd.default_chord().expect("default chord").display();
            let rebound = Chord::parse(probe).expect("written correctly").display();
            assert_ne!(
                factory.chars().count(),
                rebound.chars().count(),
                "{}'s default `{factory}` and probe `{rebound}` have the same character count, so a mac monospace font cannot tell them apart",
                cmd.key()
            );
        }
    }

    #[test]
    fn every_modifier_survives_the_round_trip() {
        let all = Mods {
            ctrl: true,
            alt: true,
            shift: true,
            platform: true,
        };
        for mods in [
            Mods::none(),
            Mods {
                ctrl: true,
                ..Mods::none()
            },
            Mods {
                alt: true,
                ..Mods::none()
            },
            Mods::shift(),
            Mods {
                platform: true,
                ..Mods::none()
            },
            all,
        ] {
            let chord = Chord::new(mods, "f7");
            let text = chord.unparse();
            assert_eq!(Chord::parse(&text), Some(chord.clone()), "{text}");
            let k = keystroke(&text);
            assert_eq!(Mods::from(k.modifiers), mods, "{text}");
            assert!(chord.matches(&k), "{text} does not recognize itself");
        }
    }

    #[test]
    fn the_table_has_no_duplicate_column() {
        let mut keys: Vec<&str> = Cmd::ALL.iter().map(|c| c.key()).collect();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "two commands share an on-disk name");

        let mut labels: Vec<&str> = Cmd::ALL.iter().map(|c| c.label().debug_name()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total, "two commands share the same label key");

        let mut chords: Vec<String> = Cmd::ALL
            .iter()
            .filter_map(|c| c.default_chord())
            .map(|c| c.unparse())
            .collect();
        let bound = chords.len();
        chords.sort();
        chords.dedup();
        assert_eq!(
            chords.len(),
            bound,
            "two commands are bound to the same key by default: {chords:?}"
        );
        for cmd in Cmd::ALL {
            assert_eq!(Cmd::from_key(cmd.key()), Some(*cmd));
        }
        assert_eq!(Cmd::from_key("no_such_command"), None);
    }

    #[test]
    fn the_defaults_would_all_pass_the_rules() {
        let mut km = Keymap::default();
        for cmd in Cmd::ALL {
            let Some(chord) = cmd.default_chord() else {
                continue;
            };
            km.set(*cmd, chord.clone()).unwrap_or_else(|e| {
                panic!(
                    "set() would not accept {}'s default key {}: {e:?}",
                    cmd.key(),
                    chord.unparse()
                )
            });
        }
        assert!(
            km.is_default(),
            "after resetting the defaults it should be default again"
        );
    }

    #[test]
    fn each_default_keystroke_finds_its_own_command() {
        let km = Keymap::default();
        for cmd in Cmd::ALL {
            let Some(chord) = cmd.default_chord() else {
                continue;
            };
            let k = keystroke(&chord.unparse());
            assert_eq!(km.lookup(&k), Some(*cmd), "{}", chord.unparse());
        }
        for cmd in Cmd::ALL.iter().filter(|c| c.default_chord().is_none()) {
            assert!(
                km.chord_for(*cmd).is_none(),
                "{} should ship unset",
                cmd.key()
            );
        }
        assert_eq!(km.lookup(&keystroke("ctrl-alt-shift-f9")), None);
    }

    #[test]
    fn an_extra_modifier_is_a_different_chord() {
        let km = Keymap::default();
        let save = Cmd::Save
            .default_chord()
            .expect("save must have a default key");
        assert_eq!(km.lookup(&keystroke(&save.unparse())), Some(Cmd::Save));

        let mut louder = save.clone();
        louder.mods.shift = true;
        assert_eq!(km.lookup(&keystroke(&louder.unparse())), Some(Cmd::SaveAs));

        let mut noisy = save;
        noisy.mods.alt = true;
        assert_eq!(
            km.lookup(&keystroke(&noisy.unparse())),
            None,
            "an extra alt should not be recognized"
        );
    }

    #[test]
    fn binding_over_another_command_takes_the_key_and_names_the_loser() {
        let mut km = Keymap::default();
        let save = Cmd::Save
            .default_chord()
            .expect("save must have a default key");
        let taken = km
            .set(Cmd::Find, save.clone())
            .expect("this is a legal binding");
        assert_eq!(
            taken,
            Some(Cmd::Save),
            "it should name save as the one taken over"
        );
        assert_eq!(km.chord_for(Cmd::Find), Some(&save));
        assert!(
            km.chord_for(Cmd::Save).is_none(),
            "the stolen command should become unset"
        );
        assert_eq!(km.lookup(&keystroke(&save.unparse())), Some(Cmd::Find));

        let again = km.set(Cmd::Find, save.clone()).expect("legal");
        assert_eq!(
            again, None,
            "rebinding its own chord should not report a victim"
        );
        assert_eq!(km.chord_for(Cmd::Find), Some(&save));
    }

    #[test]
    fn the_rules_say_which_rule_was_broken() {
        let mut km = Keymap::default();
        let primary = Mods::primary();

        for key in ["a", "1", "space", "-"] {
            assert_eq!(
                km.set(Cmd::Find, Chord::new(Mods::none(), key)),
                Err(Refusal::NeedsModifier),
                "bare {key} should require a modifier"
            );
            assert_eq!(
                km.set(Cmd::Find, Chord::new(Mods::shift(), key)),
                Err(Refusal::NeedsModifier),
                "shift-{key} should also require a modifier"
            );
        }

        let cases = [
            (Mods::none(), "escape"),
            (Mods::none(), "left"),
            (Mods::shift(), "left"),
            (Mods::none(), "tab"),
            (Mods::shift(), "tab"),
            (Mods::none(), "enter"),
            (Mods::shift(), "enter"),
            (Mods::none(), "f5"),
            (primary, "["),
            (primary, "]"),
            (primary, "right"),
            (Mods::primary_shift(), "right"),
            (primary, "backspace"),
        ];
        for (mods, key) in cases {
            let chord = Chord::new(mods, key);
            let want = Refusal::Reserved(chord.unparse());
            assert_eq!(
                km.set(Cmd::Find, chord.clone()),
                Err(want),
                "{} should be reported as reserved",
                chord.unparse()
            );
        }

        assert_eq!(km.chord_for(Cmd::Find), Cmd::Find.default_chord().as_ref());

        let mut free = vec!["ctrl-f9", "alt-k", "f7", "shift-f7", "ctrl-alt-right"];
        if !cfg!(target_os = "macos") {
            free.push("alt-right");
        }
        for text in free {
            let chord = Chord::parse(text).expect("written correctly");
            assert!(
                km.set(Cmd::Find, chord).is_ok(),
                "{text} should be bindable"
            );
        }
    }

    #[test]
    fn each_reserved_row_covers_exactly_its_reach() {
        let bare = &["escape", "f5", "f6"][..];
        let with_shift = &["pageup", "pagedown", "enter", "tab"][..];
        let with_primary = &[
            "left",
            "right",
            "up",
            "down",
            "home",
            "end",
            "backspace",
            "delete",
        ][..];
        let primary_only = &["[", "]"][..];

        let shift = Mods::shift();
        let primary = Mods::primary();
        let primary_shift = Mods::primary_shift();

        let refused = |mods: Mods, key: &'static str| {
            matches!(
                Keymap::default().set(Cmd::Find, Chord::new(mods, key)),
                Err(Refusal::Reserved(_))
            )
        };
        let allowed = |mods: Mods, key: &'static str| {
            Keymap::default()
                .set(Cmd::Find, Chord::new(mods, key))
                .is_ok()
        };

        let listed: std::collections::BTreeSet<&str> = RESERVED.iter().map(|(k, _)| *k).collect();
        let expected: std::collections::BTreeSet<&str> = bare
            .iter()
            .chain(with_shift)
            .chain(with_primary)
            .chain(primary_only)
            .copied()
            .collect();
        assert_eq!(
            listed, expected,
            "the reserved table's rows do not match what is written here"
        );
        assert_eq!(
            RESERVED.len(),
            expected.len(),
            "the reserved table has duplicate rows"
        );

        for key in bare {
            assert!(refused(Mods::none(), key), "bare {key} should be blocked");
            assert!(allowed(shift, key), "shift-{key} should not be refused");
        }
        for key in with_shift {
            assert!(refused(Mods::none(), key), "bare {key} should be blocked");
            assert!(refused(shift, key), "shift-{key} should be blocked");
            assert!(
                allowed(primary, key),
                "primary modifier + {key} should not be refused"
            );
        }
        for key in with_primary {
            assert!(refused(Mods::none(), key), "bare {key} should be blocked");
            assert!(refused(shift, key), "shift-{key} should be blocked");
            assert!(
                refused(primary, key),
                "primary modifier + {key} should be refused"
            );
            assert!(
                refused(primary_shift, key),
                "primary modifier + shift + {key} should be refused"
            );
        }
        for key in primary_only {
            assert_eq!(
                Keymap::default().set(Cmd::Find, Chord::new(Mods::none(), key)),
                Err(Refusal::NeedsModifier),
                "bare {key} is printable, so it should demand a modifier"
            );
            assert!(
                refused(primary, key),
                "primary modifier + {key} should be refused"
            );
            assert!(
                allowed(primary_shift, key),
                "primary modifier + shift + {key} should not be refused (ordered-list style chords)"
            );
        }

        let mut km = Keymap::default();
        for (key, _) in RESERVED {
            let _ = km.set(Cmd::Find, Chord::new(Mods::none(), key));
        }
        assert_eq!(km.chord_for(Cmd::Find), Cmd::Find.default_chord().as_ref());
    }

    #[test]
    fn a_chord_no_keyboard_can_produce_does_not_parse() {
        for text in [
            "ctrl+s",
            "ctrl+alt+z",
            "ctrl-alt+z",
            "ctrl_s",
            "ctrl s",
            "ctrl-s-s",
            "ctrl-<s>",
            "",
            "ctrl-f 3",
            "ctrl-page down",
        ] {
            assert_eq!(Chord::parse(text), None, "`{text}` should not parse");
        }
        assert_eq!(Chord::parse("ctrl-"), None);
        assert_eq!(Chord::parse("ctrl-nosuch-"), Chord::parse("ctrl--"));
        for text in [
            "ctrl-s",
            "ctrl-alt-z",
            "shift-f3",
            "f3",
            "ctrl-enter",
            "secondary-s",
            "ctrl-ü",
            "ctrl-[",
            "ctrl--",
        ] {
            assert!(Chord::parse(text).is_some(), "`{text}` should parse");
        }
    }

    #[test]
    fn the_line_between_typing_and_not_typing() {
        for key in ["f1", "f9", "f12", "f24", "f35", "escape", "home", "insert"] {
            assert!(!is_printable(key), "{key} should not count as printable");
        }
        for key in ["a", "z", "1", "-", "space", "tab", "enter", "["] {
            assert!(is_printable(key), "{key} should count as printable");
        }
        assert!(is_printable("ü"));
        assert!(
            is_printable("f36"),
            "gpui's table stops at f35; anything above is an unseen key name"
        );
    }

    #[test]
    fn only_the_changed_rows_show_up_as_overrides() {
        let mut km = Keymap::default();
        assert!(km.is_default());
        assert_eq!(
            km.overrides().count(),
            0,
            "with nothing changed it should be empty"
        );

        km.clear(Cmd::Save);
        let got: Vec<_> = km.overrides().map(|(c, ch)| (c, ch.cloned())).collect();
        assert_eq!(got, vec![(Cmd::Save, None)]);
        assert!(!km.is_default());

        let chord = Chord::parse("ctrl-alt-o").expect("written correctly");
        km.set(Cmd::ToggleOutline, chord.clone()).expect("legal");
        let got: Vec<_> = km.overrides().map(|(c, ch)| (c, ch.cloned())).collect();
        assert_eq!(
            got,
            vec![(Cmd::Save, None), (Cmd::ToggleOutline, Some(chord))],
            "it should follow the Cmd::ALL order"
        );

        km.reset_all();
        assert!(
            km.is_default(),
            "after restoring defaults nothing should be overridden"
        );
        assert_eq!(km.overrides().count(), 0);
    }

    #[test]
    fn the_index_lines_up_with_the_table() {
        for (i, cmd) in Cmd::ALL.iter().enumerate() {
            assert_eq!(
                cmd.index(),
                i,
                "{}'s index is not its position in the table",
                cmd.key()
            );
        }
        assert_eq!(
            Cmd::COUNT,
            18,
            "the command table changed length; check whether the settings page still fits"
        );
    }

    #[test]
    fn setting_one_command_leaves_the_others_alone() {
        for cmd in Cmd::ALL {
            let mut km = Keymap::default();
            let chord = Chord::parse("ctrl-alt-shift-f8").expect("written correctly");
            km.set(*cmd, chord.clone()).expect("legal");
            assert_eq!(km.chord_for(*cmd), Some(&chord), "{}", cmd.key());
            for other in Cmd::ALL.iter().filter(|c| *c != cmd) {
                assert_eq!(
                    km.chord_for(*other),
                    other.default_chord().as_ref(),
                    "changing {} moved {}",
                    cmd.key(),
                    other.key()
                );
            }
        }
    }

    #[test]
    fn a_keystroke_with_no_key_name_is_not_a_chord() {
        let blank = Keystroke {
            modifiers: Modifiers::default(),
            key: String::new(),
            key_char: None,
        };
        assert_eq!(
            Chord::from_keystroke(&blank),
            None,
            "an empty key name must not fabricate a chord"
        );
        let with_mods = Keystroke {
            modifiers: Modifiers::from(Mods::primary()),
            key: String::new(),
            key_char: None,
        };
        assert_eq!(
            Chord::from_keystroke(&with_mods),
            None,
            "an empty key name with modifiers must not fabricate a chord either"
        );
    }

    #[test]
    fn the_table_never_holds_a_duplicate_it_cannot_resolve() {
        let mut km = Keymap::default();
        let chord = Chord::parse("ctrl-alt-d").expect("written correctly");
        km.put(Cmd::Save, Some(chord.clone()));
        km.put(Cmd::Find, Some(chord.clone()));
        let k = keystroke(&chord.unparse());
        assert_eq!(km.lookup(&k), Some(Cmd::Save));
    }
}

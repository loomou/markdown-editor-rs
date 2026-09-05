use gpui::Modifiers;

pub fn primary_down(m: &Modifiers) -> bool {
    #[cfg(target_os = "macos")]
    {
        m.platform && !m.control && !m.alt
    }
    #[cfg(not(target_os = "macos"))]
    {
        m.control && !m.platform && !m.alt
    }
}

pub fn has_chord(m: &Modifiers) -> bool {
    m.control || m.alt || m.platform
}

pub fn word_mod_down(m: &Modifiers) -> bool {
    #[cfg(target_os = "macos")]
    {
        m.alt && !m.control && !m.platform
    }
    #[cfg(not(target_os = "macos"))]
    {
        primary_down(m)
    }
}

#[cfg(test)]
mod tests {
    use super::{has_chord, primary_down, word_mod_down};
    use gpui::Modifiers;

    fn none() -> Modifiers {
        Modifiers::default()
    }

    #[test]
    fn primary_matches_os_modifier() {
        let mut m = none();
        #[cfg(target_os = "macos")]
        {
            m.platform = true;
        }
        #[cfg(not(target_os = "macos"))]
        {
            m.control = true;
        }
        assert!(primary_down(&m));
        assert!(has_chord(&m));
    }

    #[test]
    fn primary_rejects_the_other_os_modifier() {
        let mut m = none();
        #[cfg(target_os = "macos")]
        {
            m.control = true;
        }
        #[cfg(not(target_os = "macos"))]
        {
            m.platform = true;
        }
        assert!(!primary_down(&m));
        assert!(has_chord(&m));
    }

    #[test]
    fn unmodified_keys_are_not_chords() {
        let m = none();
        assert!(!primary_down(&m));
        assert!(!word_mod_down(&m));
        assert!(!has_chord(&m));
    }

    #[test]
    fn word_mod_matches_option_on_mac_and_ctrl_elsewhere() {
        let mut m = none();
        #[cfg(target_os = "macos")]
        {
            m.alt = true;
        }
        #[cfg(not(target_os = "macos"))]
        {
            m.control = true;
        }
        assert!(word_mod_down(&m));
        assert!(has_chord(&m));
    }

    #[test]
    fn word_mod_rejects_primary_on_mac() {
        let mut m = none();
        #[cfg(target_os = "macos")]
        {
            m.platform = true;
            assert!(!word_mod_down(&m));
            assert!(primary_down(&m));
        }
        #[cfg(not(target_os = "macos"))]
        {
            m.control = true;
            assert!(word_mod_down(&m));
            assert!(primary_down(&m));
        }
    }
}

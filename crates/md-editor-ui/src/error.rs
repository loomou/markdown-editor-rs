use std::fmt;
use std::io;
use std::path::PathBuf;

use md_i18n::Key;

#[derive(Debug)]
pub enum Error {
    Read { path: PathBuf, source: io::Error },
    Save { path: PathBuf, source: io::Error },
    NotMarkdown { path: PathBuf },
}

impl Error {
    fn io_source(&self) -> Option<&io::Error> {
        match self {
            Self::Read { source, .. } | Self::Save { source, .. } => Some(source),
            Self::NotMarkdown { .. } => None,
        }
    }

    pub fn message_in(&self, lang: md_i18n::Lang) -> String {
        match self {
            Self::Read { path, source } => {
                let mut msg = format!(
                    "{} {}: {source}",
                    md_i18n::t_in(lang, Key::ErrCannotRead),
                    path.display()
                );
                if source.kind() == io::ErrorKind::InvalidData {
                    msg.push(' ');
                    msg.push_str(md_i18n::t_in(lang, Key::ErrUtf8Only));
                }
                msg
            }
            Self::Save { path, source } => format!(
                "{} {}: {source}",
                md_i18n::t_in(lang, Key::ErrCannotSave),
                path.display()
            ),
            Self::NotMarkdown { path } => format!(
                "{} {}",
                md_i18n::t_in(lang, Key::ErrNotMarkdown),
                path.display()
            ),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message_in(md_i18n::current()))
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.io_source().map(|e| e as _)
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use md_i18n::Key;
    use std::io;
    use std::path::PathBuf;

    #[test]
    fn read_display_includes_path_and_source() {
        let err = Error::Read {
            path: PathBuf::from("doc.md"),
            source: io::Error::new(io::ErrorKind::NotFound, "nope"),
        };
        let msg = err.to_string();
        assert!(msg.contains("doc.md"), "{msg}");
        assert!(msg.contains("nope"), "{msg}");
    }

    #[test]
    fn reading_and_saving_do_not_share_one_sentence() {
        let path = PathBuf::from("doc.md");
        let read = Error::Read {
            path: path.clone(),
            source: io::Error::new(io::ErrorKind::NotFound, "nope"),
        };
        let save = Error::Save {
            path,
            source: io::Error::new(io::ErrorKind::NotFound, "nope"),
        };
        assert_ne!(read.to_string(), save.to_string());
    }

    #[test]
    fn the_sentence_follows_the_language() {
        use md_i18n::Lang;
        let cases = [
            (
                Key::ErrCannotRead,
                Error::Read {
                    path: PathBuf::from("doc.md"),
                    source: io::Error::new(io::ErrorKind::NotFound, "nope"),
                },
            ),
            (
                Key::ErrCannotSave,
                Error::Save {
                    path: PathBuf::from("doc.md"),
                    source: io::Error::new(io::ErrorKind::NotFound, "nope"),
                },
            ),
            (
                Key::ErrNotMarkdown,
                Error::NotMarkdown {
                    path: PathBuf::from("notes.txt"),
                },
            ),
        ];
        for (key, err) in cases {
            for lang in Lang::ALL {
                let shown = err.message_in(lang);
                let want = md_i18n::t_in(lang, key);
                assert!(
                    shown.starts_with(want),
                    "under {} the message should start with `{want}` but shows `{shown}`",
                    lang.key()
                );
            }
            assert_ne!(
                err.message_in(Lang::ZhCn),
                err.message_in(Lang::En),
                "{} must not say the same sentence in both languages",
                key.debug_name()
            );
        }
    }

    #[test]
    fn invalid_data_read_mentions_utf8_only() {
        use md_i18n::Lang;
        let err = Error::Read {
            path: PathBuf::from("doc.md"),
            source: io::Error::new(io::ErrorKind::InvalidData, "not utf-8"),
        };
        for lang in Lang::ALL {
            let shown = err.message_in(lang);
            let hint = md_i18n::t_in(lang, Key::ErrUtf8Only);
            assert!(
                shown.contains(hint),
                "under {} the message should mention `{hint}` but shows `{shown}`",
                lang.key()
            );
        }
        let missing = Error::Read {
            path: PathBuf::from("doc.md"),
            source: io::Error::new(io::ErrorKind::NotFound, "nope"),
        };
        let save_invalid = Error::Save {
            path: PathBuf::from("doc.md"),
            source: io::Error::new(io::ErrorKind::InvalidData, "not utf-8"),
        };
        for lang in Lang::ALL {
            let hint = md_i18n::t_in(lang, Key::ErrUtf8Only);
            assert!(
                !missing.message_in(lang).contains(hint),
                "a missing file must not say `{hint}`: {}",
                missing.message_in(lang)
            );
            assert!(
                !save_invalid.message_in(lang).contains(hint),
                "a failed save must not say `{hint}`: {}",
                save_invalid.message_in(lang)
            );
        }
    }

    #[test]
    fn the_io_error_stays_reachable_as_a_source() {
        use std::error::Error as _;
        let err = Error::Save {
            path: PathBuf::from("doc.md"),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
        };
        let source = err
            .source()
            .expect("the underlying io error should still be reachable");
        assert!(source.to_string().contains("denied"));
    }
}

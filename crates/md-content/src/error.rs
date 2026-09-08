use std::fmt;

use md_i18n::Key;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    Ours(Key),
    Theirs(String),
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ours(key) => f.write_str(md_i18n::t(*key)),
            Self::Theirs(msg) => f.write_str(msg),
        }
    }
}

impl From<Key> for Reason {
    fn from(key: Key) -> Self {
        Self::Ours(key)
    }
}

impl From<String> for Reason {
    fn from(msg: String) -> Self {
        Self::Theirs(msg)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Image(Reason),
    Mermaid(Reason),
    Math(Reason),
}

impl Error {
    pub fn reason(&self) -> &Reason {
        match self {
            Self::Image(r) | Self::Mermaid(r) | Self::Math(r) => r,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.reason().fmt(f)
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::{Error, Reason};
    use md_i18n::Key;
    use md_i18n::Lang;

    #[test]
    fn our_half_translates_and_their_half_does_not() {
        let theirs = Reason::Theirs("unexpected token at 3:14".to_owned());
        assert_eq!(
            theirs.to_string(),
            "unexpected token at 3:14",
            "the third-party message must not be touched"
        );

        let key = Key::DiagramEmpty;
        let shown = Reason::Ours(key).to_string();
        let table = [md_i18n::t_in(Lang::ZhCn, key), md_i18n::t_in(Lang::En, key)];
        assert!(
            table.contains(&shown.as_str()),
            "`{shown}` is not what {} says in any language ({table:?})",
            key.debug_name()
        );
        assert_ne!(
            Reason::Ours(Key::DiagramEmpty).to_string(),
            Reason::Ours(Key::ImageEmpty).to_string(),
            "two keys render the same message"
        );
        assert_ne!(
            md_i18n::t_in(Lang::ZhCn, key),
            md_i18n::t_in(Lang::En, key),
            "our own message should translate between languages"
        );
    }

    #[test]
    fn display_still_yields_the_message() {
        let err = Error::Mermaid(Reason::Theirs("boom".to_owned()));
        assert_eq!(err.to_string(), "boom");
        assert_eq!(err.reason(), &Reason::Theirs("boom".to_owned()));
    }

    #[test]
    fn source_is_none_by_design() {
        let err = Error::Image(Reason::Theirs("boom".to_owned()));
        assert!(std::error::Error::source(&err).is_none());
    }
}

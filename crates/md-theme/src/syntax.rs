use super::paint::ThemeColor;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SyntaxRole {
    Default,
    Comment,
    Keyword,
    String,
    Character,
    Special,
    Symbol,
    Number,
    Function,
    Macro,
    TypeName,
    Property,
    Operator,
    Parameter,
    Builtin,
    Punctuation,
    Label,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyntaxTokens {
    pub default: ThemeColor,
    pub comment: ThemeColor,
    pub keyword: ThemeColor,
    pub string: ThemeColor,
    pub character: ThemeColor,
    pub special: ThemeColor,
    pub symbol: ThemeColor,
    pub number: ThemeColor,
    pub function: ThemeColor,
    pub macro_name: ThemeColor,
    pub type_name: ThemeColor,
    pub property: ThemeColor,
    pub operator: ThemeColor,
    pub parameter: ThemeColor,
    pub builtin: ThemeColor,
    pub punctuation: ThemeColor,
    pub label: ThemeColor,
}

fn rgb(hex: u32) -> ThemeColor {
    ThemeColor::from_srgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub(crate) fn mocha_base() -> ThemeColor {
    rgb(0x1e1e2e)
}

impl SyntaxTokens {
    pub(crate) fn formal() -> Self {
        Self {
            default: rgb(0xcdd6f4),
            comment: rgb(0x9399b2),
            keyword: rgb(0xcba6f7),
            string: rgb(0xa6e3a1),
            character: rgb(0x94e2d5),
            special: rgb(0xf5c2e7),
            symbol: rgb(0xf2cdcd),
            number: rgb(0xfab387),
            function: rgb(0x89b4fa),
            macro_name: rgb(0xf5e0dc),
            type_name: rgb(0xf9e2af),
            property: rgb(0xb4befe),
            operator: rgb(0x89dceb),
            parameter: rgb(0xeba0ac),
            builtin: rgb(0xf38ba8),
            punctuation: rgb(0x9399b2),
            label: rgb(0x74c7ec),
        }
    }

    pub(crate) fn one_dark() -> Self {
        Self {
            default: rgb(0xdce0e5),
            comment: rgb(0x878a98),
            keyword: rgb(0xb477cf),
            string: rgb(0xa1c181),
            character: rgb(0x56b6c2),
            special: rgb(0xb477cf),
            symbol: rgb(0xe06c75),
            number: rgb(0xdfc184),
            function: rgb(0x74ade8),
            macro_name: rgb(0xe5c07b),
            type_name: rgb(0xe5c07b),
            property: rgb(0x74ade8),
            operator: rgb(0x56b6c2),
            parameter: rgb(0xa9afbc),
            builtin: rgb(0xe06c75),
            punctuation: rgb(0x878a98),
            label: rgb(0x56b6c2),
        }
    }

    pub(crate) fn one_light() -> Self {
        Self {
            default: rgb(0x242529),
            comment: rgb(0x7e8086),
            keyword: rgb(0xa449ab),
            string: rgb(0x649f57),
            character: rgb(0x3882b7),
            special: rgb(0xa449ab),
            symbol: rgb(0xd36151),
            number: rgb(0xad6e25),
            function: rgb(0x5c78e2),
            macro_name: rgb(0xc18401),
            type_name: rgb(0xc18401),
            property: rgb(0x5c78e2),
            operator: rgb(0x3882b7),
            parameter: rgb(0x58585a),
            builtin: rgb(0xd36151),
            punctuation: rgb(0x7e8086),
            label: rgb(0x3882b7),
        }
    }
    pub fn color(self, role: SyntaxRole) -> ThemeColor {
        match role {
            SyntaxRole::Default => self.default,
            SyntaxRole::Comment => self.comment,
            SyntaxRole::Keyword => self.keyword,
            SyntaxRole::String => self.string,
            SyntaxRole::Character => self.character,
            SyntaxRole::Special => self.special,
            SyntaxRole::Symbol => self.symbol,
            SyntaxRole::Number => self.number,
            SyntaxRole::Function => self.function,
            SyntaxRole::Macro => self.macro_name,
            SyntaxRole::TypeName => self.type_name,
            SyntaxRole::Property => self.property,
            SyntaxRole::Operator => self.operator,
            SyntaxRole::Parameter => self.parameter,
            SyntaxRole::Builtin => self.builtin,
            SyntaxRole::Punctuation => self.punctuation,
            SyntaxRole::Label => self.label,
        }
    }

    pub fn fingerprint(self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for c in [
            self.default,
            self.comment,
            self.keyword,
            self.string,
            self.character,
            self.special,
            self.symbol,
            self.number,
            self.function,
            self.macro_name,
            self.type_name,
            self.property,
            self.operator,
            self.parameter,
            self.builtin,
            self.punctuation,
            self.label,
        ] {
            c.h.to_bits().hash(&mut h);
            c.s.to_bits().hash(&mut h);
            c.l.to_bits().hash(&mut h);
            c.a.to_bits().hash(&mut h);
        }
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{SyntaxTokens, mocha_base};

    #[test]
    fn mocha_tokens_match_catppuccin_hex() {
        let t = SyntaxTokens::formal();
        assert_eq!(t.default.to_css_hex(), "#cdd6f4");
        assert_eq!(t.comment.to_css_hex(), "#9399b2");
        assert_eq!(t.keyword.to_css_hex(), "#cba6f7");
        assert_eq!(t.string.to_css_hex(), "#a6e3a1");
        assert_eq!(t.character.to_css_hex(), "#94e2d5");
        assert_eq!(t.special.to_css_hex(), "#f5c2e7");
        assert_eq!(t.symbol.to_css_hex(), "#f2cdcd");
        assert_eq!(t.number.to_css_hex(), "#fab387");
        assert_eq!(t.function.to_css_hex(), "#89b4fa");
        assert_eq!(t.macro_name.to_css_hex(), "#f5e0dc");
        assert_eq!(t.type_name.to_css_hex(), "#f9e2af");
        assert_eq!(t.property.to_css_hex(), "#b4befe");
        assert_eq!(t.operator.to_css_hex(), "#89dceb");
        assert_eq!(t.parameter.to_css_hex(), "#eba0ac");
        assert_eq!(t.builtin.to_css_hex(), "#f38ba8");
        assert_eq!(t.punctuation.to_css_hex(), "#9399b2");
        assert_eq!(t.label.to_css_hex(), "#74c7ec");
        assert_eq!(mocha_base().to_css_hex(), "#1e1e2e");
    }
}

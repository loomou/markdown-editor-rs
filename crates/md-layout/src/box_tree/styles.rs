use crate::style::BoxLayoutStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BoxStyleId(u16);

impl BoxStyleId {
    #[cfg(test)]
    pub(crate) const ZERO: Self = BoxStyleId(0);
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BoxStyleStore {
    entries: Vec<BoxLayoutStyle>,
    last: Option<(BoxLayoutStyle, BoxStyleId)>,
}

impl BoxStyleStore {
    pub(crate) fn intern(&mut self, style: BoxLayoutStyle) -> BoxStyleId {
        if let Some((last, id)) = self.last
            && last == style
        {
            return id;
        }
        if let Some(id) = self
            .entries
            .iter()
            .position(|candidate| *candidate == style)
        {
            let id = BoxStyleId(u16::try_from(id).expect("interned box style id must fit u16"));
            self.last = Some((style, id));
            return id;
        }
        let id = u16::try_from(self.entries.len())
            .expect("BoxStyleId exhausted (maximum 65536 distinct box styles)");
        let id = BoxStyleId(id);
        self.entries.push(style);
        self.last = Some((style, id));
        id
    }

    pub(crate) fn get(&self, id: BoxStyleId) -> &BoxLayoutStyle {
        self.entries
            .get(id.0 as usize)
            .expect("BoxNode has an unknown BoxStyleId")
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

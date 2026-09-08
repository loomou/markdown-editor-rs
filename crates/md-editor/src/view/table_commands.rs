use md_core::document::TableOp;
use md_i18n::Key;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableMenuEntry {
    Separator,
    Item {
        label: Key,
        op: TableOp,
        danger: bool,
    },
}

pub static TABLE_MENU: &[TableMenuEntry] = &[
    TableMenuEntry::Item {
        label: Key::TableInsertRowAbove,
        op: TableOp::InsertRowAbove,
        danger: false,
    },
    TableMenuEntry::Item {
        label: Key::TableInsertRowBelow,
        op: TableOp::InsertRowBelow,
        danger: false,
    },
    TableMenuEntry::Item {
        label: Key::TableMoveRowUp,
        op: TableOp::MoveRowUp,
        danger: false,
    },
    TableMenuEntry::Item {
        label: Key::TableMoveRowDown,
        op: TableOp::MoveRowDown,
        danger: false,
    },
    TableMenuEntry::Separator,
    TableMenuEntry::Item {
        label: Key::TableInsertColLeft,
        op: TableOp::InsertColumnLeft,
        danger: false,
    },
    TableMenuEntry::Item {
        label: Key::TableInsertColRight,
        op: TableOp::InsertColumnRight,
        danger: false,
    },
    TableMenuEntry::Item {
        label: Key::TableMoveColLeft,
        op: TableOp::MoveColumnLeft,
        danger: false,
    },
    TableMenuEntry::Item {
        label: Key::TableMoveColRight,
        op: TableOp::MoveColumnRight,
        danger: false,
    },
    TableMenuEntry::Separator,
    TableMenuEntry::Item {
        label: Key::TableDeleteRow,
        op: TableOp::DeleteRow,
        danger: false,
    },
    TableMenuEntry::Item {
        label: Key::TableDeleteCol,
        op: TableOp::DeleteColumn,
        danger: false,
    },
    TableMenuEntry::Separator,
    TableMenuEntry::Item {
        label: Key::TableDeleteTable,
        op: TableOp::DeleteTable,
        danger: true,
    },
];

#[cfg(test)]
mod tests {
    use super::{TABLE_MENU, TableMenuEntry};
    use md_core::document::TableOp;

    #[test]
    fn table_menu_order_matches_t8() {
        let labels: Vec<_> = TABLE_MENU
            .iter()
            .map(|e| match e {
                TableMenuEntry::Separator => "---",
                TableMenuEntry::Item { label, .. } => md_i18n::t_in(md_i18n::Lang::En, *label),
            })
            .collect();
        assert_eq!(
            labels,
            [
                "Insert Row Above",
                "Insert Row Below",
                "Move Row Up",
                "Move Row Down",
                "---",
                "Insert Column Left",
                "Insert Column Right",
                "Move Column Left",
                "Move Column Right",
                "---",
                "Delete Row",
                "Delete Column",
                "---",
                "Delete Table",
            ]
        );
        let danger: Vec<TableOp> = TABLE_MENU
            .iter()
            .filter_map(|e| match e {
                TableMenuEntry::Item {
                    op, danger: true, ..
                } => Some(*op),
                _ => None,
            })
            .collect();
        assert_eq!(danger, [TableOp::DeleteTable]);
    }

    #[test]
    fn only_insert_row_below_carries_a_key() {
        use crate::keymap::Cmd;
        for entry in TABLE_MENU {
            let TableMenuEntry::Item { op, .. } = entry else {
                continue;
            };
            let want = (*op == TableOp::InsertRowBelow).then_some(Cmd::TableRowBelow);
            assert_eq!(
                crate::keymap::table_op_chord_cmd(*op),
                want,
                "{op:?} must show the expected key"
            );
        }
    }
}

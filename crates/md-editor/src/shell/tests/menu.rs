use super::support::{context_labels, mono_px, stop_blink, test_doc, text_px};
use crate::keymap::Cmd;
use crate::shell::menu::MenuEntry::{Item, Separator, Submenu};
use crate::shell::menu::{
    FLYOUT_GAP, MENU_PAD, MENU_W, MenuId, clamp_menu_pos, entries_for, flyout_screen_rect,
    menu_entries, place_table_flyout, submenu_row_top,
};
use crate::shell::{Shell, VIEW_MARGIN};
use crate::view::table_commands::TABLE_MENU;
use gpui::{
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext, point, px, size,
};
use md_i18n::Key;

#[test]
fn menu_position_stays_non_negative_in_tiny_viewports() {
    let pos = clamp_menu_pos(
        point(px(0.0), px(0.0)),
        MenuId::File,
        size(px(100.0), px(80.0)),
        false,
    );
    assert!(pos.x >= px(0.0));
    assert!(pos.y >= px(0.0));
}

#[test]
fn table_flyout_opens_to_the_right_when_there_is_room() {
    let popup = point(px(100.0), px(80.0));
    let viewport = size(px(1200.0), px(800.0));
    let place = place_table_flyout(popup, viewport, MenuId::Context, true);
    let row_w = MENU_W - MENU_PAD * 2.0;
    assert!(
        (place.x - (row_w + FLYOUT_GAP)).abs() < 0.5,
        "x={}",
        place.x
    );
    assert!((place.y + 5.0).abs() < 0.5, "y={}", place.y);
}

#[test]
fn table_flyout_flips_left_near_the_right_edge() {
    let popup = point(px(1100.0), px(80.0));
    let viewport = size(px(1200.0), px(800.0));
    let place = place_table_flyout(popup, viewport, MenuId::Context, true);
    assert!(place.x < 0.0, "expected left flip, x={}", place.x);
    let (x, y, w, h) = flyout_screen_rect(popup, viewport, MenuId::Context, true);
    assert!(x + w <= 1200.0 - VIEW_MARGIN + 0.5);
    assert!(y >= VIEW_MARGIN - 0.5);
    assert!(y + h <= 800.0 - VIEW_MARGIN + 0.5);
}

#[test]
fn table_flyout_stays_in_viewport_near_the_bottom() {
    let popup = point(px(40.0), px(500.0));
    let viewport = size(px(800.0), px(600.0));
    let (x, y, w, h) = flyout_screen_rect(popup, viewport, MenuId::Context, true);
    assert!(x >= VIEW_MARGIN - 0.5, "x={x}");
    assert!(y >= VIEW_MARGIN - 0.5, "y={y}");
    assert!(x + w <= 800.0 - VIEW_MARGIN + 0.5, "right={}", x + w);
    assert!(y + h <= 600.0 - VIEW_MARGIN + 0.5, "bottom={}", y + h);
    assert!(h > 200.0);
}

#[test]
fn table_submenu_row_sits_below_paste() {
    let y = submenu_row_top(MenuId::Context, true);
    assert!(
        (y - (MENU_PAD + 26.0 + 26.0 + 9.0 + 26.0 + 26.0 + 9.0)).abs() < 0.5,
        "y={y}"
    );
}

#[test]
fn context_menu_hides_table_when_caret_is_outside() {
    assert_eq!(
        context_labels(false),
        [
            "Undo",
            "Redo",
            "---",
            "Copy",
            "Paste",
            "---",
            "Select All",
            "Find",
            "---",
            "Insert Table…",
        ]
    );
    assert!(
        entries_for(MenuId::File, true)
            .iter()
            .all(|e| !matches!(e, Submenu { .. }))
    );
    assert!(
        entries_for(MenuId::Edit, true)
            .iter()
            .all(|e| !matches!(e, Submenu { .. }))
    );
    assert!(
        entries_for(MenuId::View, true)
            .iter()
            .all(|e| !matches!(e, Submenu { .. }))
    );
}

#[test]
fn context_menu_inserts_table_submenu_when_caret_is_in_table() {
    assert_eq!(
        context_labels(true),
        [
            "Undo",
            "Redo",
            "---",
            "Copy",
            "Paste",
            "---",
            "Table",
            "---",
            "Select All",
            "Find",
        ]
    );
    let table = entries_for(MenuId::Context, true)
        .into_iter()
        .find_map(|e| match e {
            Submenu { label, items } => Some((label, items)),
            _ => None,
        })
        .expect("Table submenu");
    assert_eq!(md_i18n::t_in(md_i18n::Lang::En, table.0), "Table");
    assert!(std::ptr::eq(table.1, TABLE_MENU));
}

#[gpui::test]
fn the_menu_bar_paints_the_words_that_the_key_table_holds(cx: &mut TestAppContext) {
    use md_i18n::{Lang, current, t_in};
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.run_until_parked();

    const PAD: f32 = 9.0 * 2.0;
    let here = current();
    let other = if here == Lang::ZhCn {
        Lang::En
    } else {
        Lang::ZhCn
    };
    let mut discriminating = 0usize;
    for (selector, key) in [
        ("menubar:MenuBarFile", Key::MenuBarFile),
        ("menubar:MenuBarEdit", Key::MenuBarEdit),
        ("menubar:MenuBarView", Key::MenuBarView),
        ("menubar:MenuBarGo", Key::MenuBarGo),
        ("menubar:MenuBarHelp", Key::MenuBarHelp),
    ] {
        let drawn = f32::from(
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} should be painted"))
                .size
                .width,
        );
        let want = text_px(cx, t_in(here, key), 13.0) + PAD;
        assert!(
            (drawn - want).abs() < 1.0,
            "{} painted {drawn}px; by `{}` it should be {want}px, so a different string was likely painted",
            key.debug_name(),
            t_in(here, key)
        );
        if (want - (text_px(cx, t_in(other, key), 13.0) + PAD)).abs() >= 1.0 {
            discriminating += 1;
        }
    }
    assert!(
        discriminating > 0,
        "precondition: at least one cell must measure differently between Chinese and English, or this test cannot detect the language"
    );
}

#[gpui::test]
fn the_menu_shows_the_key_the_table_holds(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.settings.keymap.clear(Cmd::Save);
            s.sync_editor_keymap(cx);
            s.open_menu = Some((MenuId::File, point(px(0.), px(30.))));
            cx.notify();
        });
    });
    cx.run_until_parked();

    for cmd in [Cmd::New, Cmd::Open, Cmd::SaveAs, Cmd::Quit] {
        let chord = cmd
            .default_chord()
            .expect("these four ship with a default key")
            .display();
        let selector: &'static str = match cmd {
            Cmd::New => "menukb:MenuNew",
            Cmd::Open => "menukb:MenuOpen",
            Cmd::SaveAs => "menukb:MenuSaveAs",
            _ => "menukb:MenuExit",
        };
        let drawn = f32::from(
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} should be painted"))
                .size
                .width,
        );
        let want = mono_px(cx, &chord, 10.5);
        assert!(
            (drawn - want).abs() < 1.0,
            "{} cell painted {drawn}px; by `{chord}` it should be {want}px, so a different key was likely painted",
            cmd.key()
        );
    }
    assert!(
        cx.debug_bounds("menukb:Save").is_none(),
        "the save key was cleared, so that cell should not be painted"
    );
}

#[test]
fn every_menu_label_has_a_chinese_form_too() {
    use md_i18n::{Lang, t_in};
    let keys: Vec<Key> = [MenuId::File, MenuId::Edit, MenuId::View, MenuId::Help]
        .iter()
        .flat_map(|id| menu_entries(*id))
        .chain(entries_for(MenuId::Context, true).iter())
        .filter_map(|e| match e {
            Item { label, .. } | Submenu { label, .. } => Some(*label),
            Separator => None,
        })
        .collect();
    assert!(
        keys.len() > 15,
        "precondition: the menu should have a dozen-plus entries, {}",
        keys.len()
    );
    for key in keys {
        let (zh, en) = (t_in(Lang::ZhCn, key), t_in(Lang::En, key));
        assert_ne!(
            zh,
            en,
            "{} is the same in Chinese and English, as if untranslated",
            key.debug_name()
        );
    }
}

fn click_at(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn bar_menu_popup_aligns_to_the_label_left(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.run_until_parked();

    let file = cx
        .debug_bounds("menubar:MenuBarFile")
        .expect("the File cell should be painted");
    let edit = cx
        .debug_bounds("menubar:MenuBarEdit")
        .expect("the Edit cell should be painted");
    assert!(
        f32::from(file.size.width) > 20.0,
        "precondition: the File cell must be wide enough to click on its right side: {file:?}"
    );

    let at_right = point(file.origin.x + file.size.width - px(2.), file.center().y);
    click_at(cx, at_right);
    let (id, pos) = shell
        .read_with(cx, |s, _| s.open_menu)
        .expect("clicking File should open the dropdown");
    assert_eq!(id, MenuId::File);
    assert!(
        (f32::from(pos.x) - f32::from(file.origin.x)).abs() < 0.5,
        "a click on the right should still align to the left edge: popup.x={} label.x={}",
        pos.x,
        file.origin.x
    );

    let hover = point(edit.origin.x + edit.size.width - px(2.), edit.center().y);
    cx.simulate_event(MouseMoveEvent {
        position: hover,
        pressed_button: None,
        modifiers: gpui::Modifiers::default(),
    });
    cx.run_until_parked();
    let (id, pos) = shell
        .read_with(cx, |s, _| s.open_menu)
        .expect("hovering to Edit should switch the dropdown");
    assert_eq!(id, MenuId::Edit);
    assert!(
        (f32::from(pos.x) - f32::from(edit.origin.x)).abs() < 0.5,
        "hovering over should also align to the left edge: popup.x={} label.x={}",
        pos.x,
        edit.origin.x
    );
}

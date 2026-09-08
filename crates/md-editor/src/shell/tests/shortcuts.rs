use super::support::{key_for, mono_px, stop_blink, temp_settings_path, test_doc, text_px};
use crate::keymap::{Chord, Cmd, Mods};
use crate::shell::CloseFind;
use crate::shell::Shell;
use gpui::MouseUpEvent;
use gpui::{Entity, MouseButton, MouseDownEvent};
use gpui::{TestAppContext, VisualTestContext};
use md_i18n::{Key, t as t18};

fn open_shortcuts_page(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    stop_blink(shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.show_settings = true;
            s.settings_nav = 3;
            cx.notify();
        });
    });
    cx.run_until_parked();
}

fn leaked_bounds(cx: &mut VisualTestContext, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
}

fn click_shortcut_row(cmd: Cmd, cx: &mut VisualTestContext) {
    let selector: &'static str = match cmd {
        Cmd::Save => "kb:Save",
        Cmd::Undo => "kb:Undo",
        Cmd::ToggleOutline => "kb:ToggleOutline",
        Cmd::Find => "kb:Find",
        other => panic!("this one has no registered selector: {}", other.key()),
    };
    let cell = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} should be drawn"));
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: cell.center(),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: cell.center(),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn clicking_a_row_then_pressing_a_key_rebinds_it(cx: &mut TestAppContext) {
    let path = temp_settings_path("rec-save");
    let store = crate::store::settings::SettingsStore::at(path.clone());
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.adopt_settings_store(
                Some(crate::store::settings::SettingsStore::at(path.clone())),
                cx,
            );
        });
    });
    open_shortcuts_page(&shell, cx);

    click_shortcut_row(Cmd::Save, cx);
    assert_eq!(
        shell.read_with(cx, |s, _| s.recording),
        Some(Cmd::Save),
        "clicking that row should enter the armed state"
    );

    cx.simulate_keystrokes("ctrl-alt-k");
    cx.run_until_parked();

    assert_eq!(
        shell.read_with(cx, |s, _| s.recording),
        None,
        "a successful recording should exit the armed state"
    );
    assert_eq!(
        shell
            .read_with(cx, |s, _| s.settings.keymap.chord_for(Cmd::Save).cloned())
            .map(|c| c.unparse()),
        Some("ctrl-alt-k".to_owned()),
        "the keymap should now hold the new shortcut"
    );
    assert_eq!(
        store
            .load()
            .and_then(|s| s.keymap.chord_for(Cmd::Save).cloned())
            .map(|c| c.unparse()),
        Some("ctrl-alt-k".to_owned()),
        "the new shortcut should have been written to disk"
    );
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn recording_does_not_run_the_command_it_records(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.editor.update(cx, |editor, cx| {
                let sel = md_core::document::Sel::collapsed(editor.state.cursor);
                editor.state.cursor = editor
                    .state
                    .doc
                    .apply(sel, md_core::document::Command::Insert { text: "x".into() });
                cx.notify();
            });
        });
    });
    let dirty = shell.read_with(cx, |s, app| {
        s.editor.read(app).state.doc.document.to_markdown()
    });
    assert!(
        dirty.contains('x'),
        "precondition: that stroke should have landed in the document"
    );

    open_shortcuts_page(&shell, cx);
    click_shortcut_row(Cmd::ToggleOutline, cx);
    cx.simulate_keystrokes(&key_for(Cmd::Undo));
    cx.run_until_parked();

    assert_eq!(
        shell.read_with(cx, |s, app| s
            .editor
            .read(app)
            .state
            .doc
            .document
            .to_markdown()),
        dirty,
        "the recording keypress also ran the command — the document was undone one step"
    );
    assert_eq!(
        shell
            .read_with(cx, |s, _| s
                .settings
                .keymap
                .chord_for(Cmd::ToggleOutline)
                .cloned())
            .map(|c| c.unparse()),
        Some(key_for(Cmd::Undo)),
        "that stroke should have been recorded under the toggle-outline command"
    );
    assert_eq!(
        shell.read_with(cx, |s, _| s.settings.keymap.chord_for(Cmd::Undo).cloned()),
        None,
        "the stolen shortcut should become unset"
    );
}

#[gpui::test]
fn backspace_clears_and_escape_cancels(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    cx.update(|_, app| {
        app.bind_keys([gpui::KeyBinding::new("escape", CloseFind, None)]);
    });
    open_shortcuts_page(&shell, cx);
    let before = key_for(Cmd::Save);

    click_shortcut_row(Cmd::Save, cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |s, _| s.recording),
        None,
        "Esc should exit the armed state"
    );
    assert_eq!(
        shell
            .read_with(cx, |s, _| s.settings.keymap.chord_for(Cmd::Save).cloned())
            .map(|c| c.unparse()),
        Some(before),
        "the shortcut should be unchanged after Esc"
    );

    click_shortcut_row(Cmd::Save, cx);
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |s, _| s.settings.keymap.chord_for(Cmd::Save).cloned()),
        None,
        "backspace should clear that shortcut"
    );
    assert_eq!(
        shell.read_with(cx, |s, _| s.recording),
        None,
        "after clearing, it should also exit the armed state"
    );
}

#[gpui::test]
fn a_refused_combination_says_which_kind_and_stays_armed(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    open_shortcuts_page(&shell, cx);
    click_shortcut_row(Cmd::ToggleOutline, cx);

    cx.simulate_keystrokes("a");
    cx.run_until_parked();
    let needs_mod = shell
        .read_with(cx, |s, _| s.record_note.clone())
        .expect("it should say why");
    assert_eq!(
        needs_mod.0,
        Cmd::ToggleOutline,
        "that sentence should hang on this row"
    );
    assert_eq!(
        needs_mod.1,
        md_i18n::t(Key::ShortcutNeedsModifier),
        "bare `a` should say \"needs a modifier\""
    );
    assert_eq!(
        shell.read_with(cx, |s, _| s.recording),
        Some(Cmd::ToggleOutline),
        "a refusal should stay in the waiting-to-record state and let the user keep pressing"
    );

    let reserved_chord = Chord::new(Mods::primary(), "right").unparse();
    cx.simulate_keystrokes(&reserved_chord);
    cx.run_until_parked();
    let reserved = shell
        .read_with(cx, |s, _| s.record_note.clone())
        .expect("it should say why");
    assert_ne!(
        reserved.1, needs_mod.1,
        "the two refusals should say different things — the same wording would not tell the user how to fix it"
    );
    assert!(
        reserved.1.contains(&reserved_chord),
        "it should say which shortcut is taken: `{}`",
        reserved.1
    );
    assert_eq!(
        shell.read_with(cx, |s, _| s.recording),
        Some(Cmd::ToggleOutline),
        "after the second refusal it must stay armed"
    );
    assert_eq!(
        shell.read_with(cx, |s, _| s
            .settings
            .keymap
            .chord_for(Cmd::ToggleOutline)
            .cloned()),
        None,
        "both strokes must be refused, leaving the command unset"
    );
}

#[gpui::test]
fn the_shortcut_page_shows_the_key_the_table_holds(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    open_shortcuts_page(&shell, cx);

    let save = Cmd::Save
        .default_chord()
        .expect("save ships with a default key")
        .display();
    let bound = f32::from(
        cx.debug_bounds("kb:Save")
            .expect("the button on the save row must be painted")
            .size
            .width,
    );
    let unset = f32::from(
        cx.debug_bounds("kb:ToggleOutline")
            .expect("the button on the toggle-outline row must be painted")
            .size
            .width,
    );
    const PAD: f32 = 10.0 * 2.0 + 2.0;
    let want_bound = mono_px(cx, &save, 11.5) + PAD;
    let want_unset = text_px(cx, t18(Key::ShortcutUnset), 11.5) + PAD;
    assert!(
        (want_bound - want_unset).abs() >= 1.0,
        "precondition: `{save}` and \"{}\" must measure differently",
        t18(Key::ShortcutUnset)
    );
    assert!(
        (bound - want_bound).abs() < 1.0,
        "the save button painted {bound}px, but by `{save}` it should be {want_bound}px",
    );
    assert!(
        (unset - want_unset).abs() < 1.0,
        "the toggle-outline button painted {unset}px, but by \"{}\" it should be {want_unset}px",
        t18(Key::ShortcutUnset)
    );

    click_shortcut_row(Cmd::Save, cx);
    let armed = f32::from(
        cx.debug_bounds("kb:Save")
            .expect("the button must still be painted while armed")
            .size
            .width,
    );
    let want_armed = text_px(cx, t18(Key::ShortcutRecording), 11.5) + PAD;
    assert!(
        (want_armed - want_bound).abs() >= 1.0,
        "precondition: \"{}\" and `{save}` must measure differently",
        t18(Key::ShortcutRecording)
    );
    assert!(
        (armed - want_armed).abs() < 1.0,
        "the armed button painted {armed}px, but by \"{}\" it should be {want_armed}px",
        t18(Key::ShortcutRecording)
    );

    cx.simulate_keystrokes("ctrl-alt-k");
    cx.run_until_parked();
    let after = f32::from(
        cx.debug_bounds("kb:Save")
            .expect("the button must still be painted after recording")
            .size
            .width,
    );
    let recorded = Chord::parse("ctrl-alt-k").expect("must parse");
    let want_after = mono_px(cx, &recorded.display(), 11.5) + PAD;
    assert!(
        (want_after - want_bound).abs() >= 1.0,
        "precondition: `{}` and the stock `{save}` must measure differently",
        recorded.display()
    );
    assert!(
        (after - want_after).abs() < 1.0,
        "after recording it paints {after}px, and pressing `{}` should give {want_after}px — \
         this cell looks like it read the factory table instead of the current one",
        recorded.display()
    );
}

#[gpui::test]
fn the_reset_row_shows_up_only_after_a_change(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    open_shortcuts_page(&shell, cx);
    assert!(
        cx.debug_bounds("btn:shortcuts-reset").is_none(),
        "with no override yet, that row must not be painted"
    );

    click_shortcut_row(Cmd::Save, cx);
    cx.simulate_keystrokes("ctrl-alt-k");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("btn:shortcuts-reset").is_some(),
        "after one override, that row must be painted"
    );

    let cell = cx
        .debug_bounds("btn:shortcuts-reset")
        .expect("it was just asserted to be drawn");
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: cell.center(),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: cell.center(),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |s, _| s.settings.keymap.is_default()),
        "clicking it must restore every shortcut to the factory defaults"
    );
}

#[gpui::test]
fn every_shortcut_row_still_fits_in_the_other_language(cx: &mut TestAppContext) {
    use md_i18n::{Lang, t_in};
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    open_shortcuts_page(&shell, cx);

    click_shortcut_row(Cmd::ToggleOutline, cx);
    cx.simulate_keystrokes("a");
    cx.run_until_parked();
    let probe = Cmd::ToggleOutline;
    let note_box = leaked_bounds(cx, &format!("note:{}", probe.label().debug_name()))
        .expect("precondition: after a refusal the note below must be painted");
    let kb_box = leaked_bounds(cx, &format!("kb:{}", probe.debug_name()))
        .expect("precondition: that button must be painted");
    let side_by_side = note_box.top() < kb_box.bottom() && kb_box.top() < note_box.bottom();

    let pad = f32::from(kb_box.size.width) - text_px(cx, md_i18n::t(Key::ShortcutRecording), 11.5);
    assert!(
        (0.0..60.0).contains(&pad),
        "precondition: the button padding measured {pad}px, which is absurd — that cell is probably not painting the recording text"
    );
    const GAP: f32 = 8.0;
    let mut discriminating = 0;
    for cmd in Cmd::ALL.iter().copied() {
        let label = cmd.label();
        let selector = format!("row:{}", label.debug_name());
        let column = f32::from(
            leaked_bounds(cx, &selector)
                .unwrap_or_else(|| panic!("precondition: row {selector} must be painted"))
                .size
                .width,
        );
        assert!(
            column > 100.0,
            "{selector} is only {column}px wide; the ruler is wrong"
        );
        let mut widths = vec![];
        for lang in Lang::ALL {
            let recording = text_px(cx, t_in(lang, Key::ShortcutRecording), 11.5);
            let unset = text_px(cx, t_in(lang, Key::ShortcutUnset), 11.5);
            let mut button = recording.max(unset);
            if let Some(chord) = cmd.default_chord() {
                button = button.max(mono_px(cx, &chord.display(), 11.5));
            }
            let notes = [
                t_in(lang, Key::ShortcutNeedsModifier).to_owned(),
                md_i18n::fmt::shortcut_reserved_in(lang, "ctrl-right"),
                md_i18n::fmt::shortcut_taken_from_in(lang, Key::MenuSaveAs),
            ];
            let name = text_px(cx, t_in(lang, label), 13.0);
            let mut widest_note: f32 = 0.0;
            for note in &notes {
                widest_note = widest_note.max(text_px(cx, note, 11.5));
            }
            let left = if side_by_side {
                name.max(widest_note)
            } else {
                name
            };
            let need = left + GAP + button + pad;
            assert!(
                need <= column,
                "row {} needs {need}px under {}, but the content column is only {column}px — the translation is too long; \
                 the left would push the right button out (left {left}px + button {}px; the sentence {})",
                cmd.key(),
                lang.key(),
                button + pad,
                if side_by_side {
                    "side by side with the button, counted on the left"
                } else {
                    "on its own line, not counted in"
                },
            );
            if !side_by_side {
                assert!(
                    widest_note <= column,
                    "under {}, the widest sentence of {} needs {widest_note}px, but the content column is only {column}px — \
                     it would wrap or get cut off",
                    cmd.key(),
                    lang.key(),
                );
            }
            widths.push(need);
        }
        if (widths[0] - widths[1]).abs() >= 1.0 {
            discriminating += 1;
        }
    }
    assert!(
        discriminating > 0,
        "precondition: of the eighteen rows at least one must measure differently in Chinese and English, or this test cannot see the language"
    );
}

#[gpui::test]
fn leaving_the_settings_page_while_armed_does_not_eat_every_key(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    open_shortcuts_page(&shell, cx);
    click_shortcut_row(Cmd::Save, cx);
    assert_eq!(
        shell.read_with(cx, |s, _| s.recording),
        Some(Cmd::Save),
        "precondition: it must be armed"
    );

    let cell = cx
        .debug_bounds("btn:btn-settings")
        .expect("the settings button must be painted");
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: cell.center(),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: cell.center(),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    assert!(
        !shell.read_with(cx, |s, _| s.show_settings),
        "precondition: the settings page must be closed"
    );

    cx.simulate_input("z");
    cx.run_until_parked();
    assert!(
        shell
            .read_with(cx, |s, app| s
                .editor
                .read(app)
                .state
                .doc
                .document
                .to_markdown())
            .contains('z'),
        "the key typed after leaving the settings page was eaten — the armed state is still alive"
    );
}

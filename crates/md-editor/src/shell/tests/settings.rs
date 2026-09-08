use super::support::{stop_blink, temp_settings_path, test_doc, text_px};
use crate::shell::Shell;
use gpui::MouseUpEvent;
use gpui::TestAppContext;
use gpui::{MouseButton, MouseDownEvent, px};
use md_i18n::Key;
use md_theme::{ColorSlot, Density, ThemeColor, ThemeVariant};

#[gpui::test]
fn settings_load_at_boot_and_write_back_on_change(cx: &mut TestAppContext) {
    let path = temp_settings_path("roundtrip");
    let store = crate::store::settings::SettingsStore::at(path.clone());
    let mut want = crate::store::settings::Settings::default();
    want.appearance.variant = ThemeVariant::OneLight;
    want.appearance.density = Density::Relaxed;
    want.appearance = want.appearance.with_body_size_px(21.0);
    want.autosave = true;
    want.remote_images = true;
    store.save(&want).expect("write a settings file first");

    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.adopt_settings_store(
                Some(crate::store::settings::SettingsStore::at(path.clone())),
                cx,
            );
        });
    });

    let (settings, theme, autosave, remote_images) = shell.read_with(cx, |s, app| {
        let editor = s.editor.read(app);
        (
            s.settings.clone(),
            editor.state.theme,
            editor.autosave(),
            editor.remote_images(),
        )
    });
    assert_eq!(
        settings, want,
        "the values read from disk should take over wholesale"
    );
    assert_eq!(theme, want.appearance.document_theme());
    assert!(autosave, "autosave must follow the configuration too");
    assert!(
        remote_images,
        "the remote images switch must be synced to the editor too"
    );

    cx.update(|_, app| {
        shell.update(app, |s, cx| s.toggle_variant(cx));
    });
    let on_disk = store.load().expect("the change should already be on disk");
    assert_eq!(on_disk.appearance.variant, ThemeVariant::OneDark);
    assert_eq!(
        on_disk.appearance.density,
        Density::Relaxed,
        "other entries must not be touched in passing"
    );
    assert!(on_disk.autosave);
    assert!(on_disk.remote_images);

    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn the_settings_button_hands_over_a_file_that_exists(cx: &mut TestAppContext) {
    let path = temp_settings_path("open");
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);

    assert_eq!(
        cx.update(|_, app| shell.update(app, |s, _| s.settings_file_for_open())),
        None,
        "with no settings file location there should be nothing to give"
    );

    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.adopt_settings_store(
                Some(crate::store::settings::SettingsStore::at(path.clone())),
                cx,
            );
        });
    });
    assert!(!path.exists(), "premise: the file should not exist yet");

    let got = cx.update(|_, app| shell.update(app, |s, _| s.settings_file_for_open()));
    assert_eq!(got.as_deref(), Some(path.as_path()));
    assert!(path.exists(), "the file should be dropped in along the way");
    let seeded = crate::store::settings::SettingsStore::at(path.clone())
        .load()
        .expect("the change must have been written through");
    assert_eq!(seeded, shell.read_with(cx, |s, _| s.settings.clone()));

    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn the_appearance_tab_offers_one_button_instead_of_every_swatch(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.show_settings = true;
            cx.notify();
        });
    });
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("btn:open-settings").is_some(),
        "the appearance tab should draw the \"edit settings.json\" button"
    );
    assert!(
        cx.debug_bounds("swatch:editor.canvas").is_none(),
        "the per-item swatches should already be collapsed"
    );
    assert!(
        cx.debug_bounds("note:no-settings-home").is_some(),
        "when saving fails it should say so"
    );
}

#[gpui::test]
fn the_settings_button_lines_up_with_the_controls_above_it(cx: &mut TestAppContext) {
    let path = temp_settings_path("quiet");
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.adopt_settings_store(
                Some(crate::store::settings::SettingsStore::at(path.clone())),
                cx,
            );
            s.show_settings = true;
            cx.notify();
        });
    });
    cx.run_until_parked();

    let btn = cx
        .debug_bounds("btn:open-settings")
        .expect("the appearance row should have painted its buttons");
    let tgl = cx
        .debug_bounds("tgl:tgl-autosave")
        .expect("precondition: the autosave row is in the same column");
    assert!(
        (btn.right() - tgl.right()).abs() < px(1.),
        "the button's right edge should align with the controls above: {:?} vs {:?}",
        btn.right(),
        tgl.right()
    );

    assert!(
        cx.debug_bounds("note:no-settings-home").is_none(),
        "if saving works there should be no text next to the buttons"
    );

    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn every_settings_row_still_fits_in_the_other_language(cx: &mut TestAppContext) {
    use md_i18n::{Lang, t_in};
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.show_settings = true;
            cx.notify();
        });
    });
    cx.run_until_parked();

    const CELL_PAD: f32 = 12.0 * 2.0;
    const GAP: f32 = 8.0;
    for (row, selector, title, hint, cells) in [
        (
            "language",
            "row:SetLanguage",
            Key::SetLanguage,
            Key::SetLanguageHint,
            &[
                Key::SetLanguageSystem,
                Key::SetLanguageZh,
                Key::SetLanguageEn,
            ][..],
        ),
        (
            "theme",
            "row:Theme",
            Key::Theme,
            Key::SetThemeHint,
            &[Key::SetThemeDark, Key::SetThemeLight][..],
        ),
        (
            "font",
            "row:SetBodyFont",
            Key::SetBodyFont,
            Key::SetBodyFontHint,
            &[Key::SetFontSerif, Key::SetFontSans][..],
        ),
        (
            "density",
            "row:SetDensity",
            Key::SetDensity,
            Key::SetDensityHint,
            &[
                Key::SetDensityCompact,
                Key::SetDensityNormal,
                Key::SetDensityRelaxed,
            ][..],
        ),
        (
            "startup",
            "row:SetStartup",
            Key::SetStartup,
            Key::SetStartupHint,
            &[Key::SetStartupNew, Key::SetStartupLast][..],
        ),
    ] {
        let column = f32::from(
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("premise: the {row} row should be drawn"))
                .size
                .width,
        );
        assert!(
            column > 100.0,
            "row {row} is only {column}px wide; the ruler is wrong"
        );
        for lang in Lang::ALL {
            let control: f32 = cells
                .iter()
                .map(|c| text_px(cx, t_in(lang, *c), 12.0) + CELL_PAD)
                .sum::<f32>()
                + 2.0;
            let left =
                text_px(cx, t_in(lang, title), 13.0).max(text_px(cx, t_in(lang, hint), 11.5));
            assert!(
                left + GAP + control <= column,
                "row {row} needs {}px under {}, but the content column is only {column}px — the translation is too long; \
                 the control would push the title out (title/description {left}px + control {control}px)",
                lang.key(),
                left + GAP + control,
            );
        }
    }
}

#[gpui::test]
fn the_settings_page_paints_the_words_that_the_key_table_holds(cx: &mut TestAppContext) {
    use md_i18n::{Lang, current, t_in};
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.show_settings = true;
            cx.notify();
        });
    });
    cx.run_until_parked();

    const PAD: f32 = 12.0 * 2.0;
    let here = current();
    let other = if here == Lang::ZhCn {
        Lang::En
    } else {
        Lang::ZhCn
    };
    let mut discriminating = 0usize;
    for (selector, key) in [
        ("seg:SetLanguageSystem", Key::SetLanguageSystem),
        ("seg:SetLanguageZh", Key::SetLanguageZh),
        ("seg:SetLanguageEn", Key::SetLanguageEn),
        ("seg:SetThemeDark", Key::SetThemeDark),
        ("seg:SetThemeLight", Key::SetThemeLight),
        ("seg:SetFontSerif", Key::SetFontSerif),
        ("seg:SetFontSans", Key::SetFontSans),
        ("seg:SetDensityCompact", Key::SetDensityCompact),
        ("seg:SetDensityNormal", Key::SetDensityNormal),
        ("seg:SetDensityRelaxed", Key::SetDensityRelaxed),
        ("seg:SetStartupNew", Key::SetStartupNew),
        ("seg:SetStartupLast", Key::SetStartupLast),
    ] {
        let drawn = f32::from(
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} should be painted"))
                .size
                .width,
        );
        let want = text_px(cx, t_in(here, key), 12.0) + PAD;
        assert!(
            (drawn - want).abs() < 1.0,
            "{} painted {drawn}px; by `{}` it should be {want}px, so a different string was likely painted",
            key.debug_name(),
            t_in(here, key)
        );
        if (want - (text_px(cx, t_in(other, key), 12.0) + PAD)).abs() >= 1.0 {
            discriminating += 1;
        }
    }
    assert!(
        discriminating > 0,
        "premise: this page needs at least one cell whose Chinese and English widths differ, otherwise the test cannot see the language"
    );
}

#[gpui::test]
fn picking_a_language_writes_it_down_without_changing_this_run(cx: &mut TestAppContext) {
    use crate::store::settings::LanguageChoice;
    use md_i18n::Lang;

    let path = temp_settings_path("language");
    let store = crate::store::settings::SettingsStore::at(path.clone());
    let want = crate::store::settings::Settings {
        language: LanguageChoice::Fixed(Lang::ZhCn),
        ..Default::default()
    };
    store.save(&want).expect("write a settings file first");

    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.adopt_settings_store(
                Some(crate::store::settings::SettingsStore::at(path.clone())),
                cx,
            );
            s.show_settings = true;
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |s, _| s.settings.language),
        LanguageChoice::Fixed(Lang::ZhCn),
        "the entry in the file should take over"
    );

    let before = md_i18n::current();
    let cell = cx
        .debug_bounds("seg:SetLanguageEn")
        .expect("the English segment must be drawn");
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

    assert_eq!(
        store.load().map(|s| s.language),
        Some(LanguageChoice::Fixed(Lang::En)),
        "clicking English should persist en"
    );
    assert_eq!(
        md_i18n::current(),
        before,
        "this press must not change the process language — switching languages needs a restart"
    );

    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn coming_back_to_the_window_picks_up_edits_made_outside(cx: &mut TestAppContext) {
    let path = temp_settings_path("reload");
    let store = crate::store::settings::SettingsStore::at(path.clone());
    store
        .save(&crate::store::settings::Settings::default())
        .expect("write a factory config first");

    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.adopt_settings_store(
                Some(crate::store::settings::SettingsStore::at(path.clone())),
                cx,
            );
        });
    });
    assert!(
        shell.read_with(cx, |s, _| s.is_dark()),
        "premise: the factory default is dark"
    );

    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.settings.appearance = s.settings.appearance.with_body_size_px(21.0);
            s.appearance_changed(cx);
        });
    });
    cx.update(|_, app| {
        shell.update(app, |s, cx| s.reload_settings_if_changed(cx));
    });
    assert_eq!(
        shell.read_with(cx, |s, _| s.settings.appearance.body_size_px),
        21.0,
        "our own write was read back as an external change"
    );

    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.recolor(
                ColorSlot::Canvas,
                |_| ThemeColor::from_css_hex("#123456").expect("hex"),
                cx,
            );
        });
    });
    cx.update(|_, app| {
        shell.update(app, |s, cx| s.reload_settings_if_changed(cx));
    });
    assert_eq!(
        shell.read_with(cx, |s, _| s
            .settings
            .appearance
            .colors()
            .get(ColorSlot::Canvas)),
        ThemeColor::from_css_hex("#123456"),
        "the file did not move, yet the in-hand color not yet saved was wiped back to the stale on-disk value"
    );

    let mut outside = crate::store::settings::Settings::default();
    outside.appearance.variant = ThemeVariant::OneLight;
    outside.appearance = outside.appearance.with_body_size_px(13.0);
    outside.appearance.colors[ThemeVariant::OneLight.index()].set(
        ColorSlot::Canvas,
        ThemeColor::from_css_hex("#fff8e7").expect("hex"),
    );
    bump_mtime_back(&path);
    store.save(&outside).expect("change it from outside once");

    cx.update(|_, app| {
        shell.update(app, |s, cx| s.reload_settings_if_changed(cx));
    });
    assert_eq!(
        shell.read_with(cx, |s, _| s.settings.clone()),
        outside,
        "it should be read back"
    );
    assert!(
        !shell.read_with(cx, |s, _| s.is_dark()),
        "the shell should switch to light with it"
    );
    assert_eq!(
        shell.read_with(cx, |s, app| s.editor.read(app).state.theme),
        outside.appearance.document_theme(),
    );

    bump_mtime_back(&path);
    std::fs::write(&path, b"{ this is not json").expect("write it broken");
    cx.update(|_, app| {
        shell.update(app, |s, cx| s.reload_settings_if_changed(cx));
    });
    assert_eq!(
        shell.read_with(cx, |s, _| s.settings.clone()),
        outside,
        "when it cannot be parsed it should keep the last usable settings"
    );

    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn bump_mtime_back(path: &std::path::Path) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let Ok(mtime) = meta.modified() else {
        return;
    };
    let back = mtime - std::time::Duration::from_secs(2);
    let _ = std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(back));
}

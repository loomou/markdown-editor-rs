use std::path::{Path, PathBuf};

pub fn is_markdown_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("md" | "markdown")
    )
}

pub fn with_markdown_extension(path: PathBuf) -> PathBuf {
    if is_markdown_path(&path) {
        return path;
    }
    match path.extension() {
        Some(ext) if !ext.is_empty() => path,
        _ => {
            let mut path = path;
            path.set_extension("md");
            path
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    Ignore,
    Url(String),
    Local(PathBuf),
    ConfirmLocal(PathBuf),
}

pub fn classify_dest(dest: &str, source_path: Option<&Path>) -> OpenTarget {
    let dest = dest.trim();
    if dest.is_empty() || dest.starts_with('#') {
        return OpenTarget::Ignore;
    }
    let lower = dest.to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("data:") {
        return OpenTarget::Ignore;
    }
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
    {
        return OpenTarget::Url(dest.to_owned());
    }
    if lower.starts_with("file:") {
        let Ok(url) = gpui::http_client::Url::parse(dest) else {
            return OpenTarget::Ignore;
        };
        let Ok(path) = url.to_file_path() else {
            return OpenTarget::Ignore;
        };
        return OpenTarget::ConfirmLocal(normalize_open_path(path));
    }
    if has_uri_scheme(dest) {
        return OpenTarget::Ignore;
    }
    let remote_path = is_unc_path(dest);
    let raw = PathBuf::from(dest);
    let path = if remote_path || raw.is_absolute() || looks_like_windows_drive(dest) {
        raw
    } else {
        source_path
            .and_then(Path::parent)
            .map(|parent| parent.join(&raw))
            .unwrap_or(raw)
    };
    let path = normalize_open_path(path);
    if !remote_path && safe_local_extension(&path) {
        OpenTarget::Local(path)
    } else {
        OpenTarget::ConfirmLocal(path)
    }
}

fn safe_local_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some(
            "md" | "markdown"
                | "txt"
                | "pdf"
                | "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "webp"
                | "svg"
                | "bmp"
                | "tif"
                | "tiff"
        )
    )
}

fn is_unc_path(dest: &str) -> bool {
    dest.starts_with(r"\\") || dest.starts_with("//")
}

fn looks_like_windows_drive(dest: &str) -> bool {
    let bytes = dest.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn has_uri_scheme(dest: &str) -> bool {
    if looks_like_windows_drive(dest) {
        return false;
    }
    let Some((scheme, _)) = dest.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn normalize_open_path(path: PathBuf) -> PathBuf {
    let path = path
        .canonicalize()
        .or_else(|_| std::path::absolute(&path))
        .unwrap_or(path);
    strip_verbatim(path)
}

#[cfg(windows)]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const VERBATIM: [u16; 4] = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];

    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    let Some(rest) = wide.strip_prefix(&VERBATIM) else {
        return path;
    };
    let unc: Vec<u16> = "UNC\\".encode_utf16().collect();
    if let Some(tail) = rest.strip_prefix(&unc[..]) {
        let mut out = "\\\\".encode_utf16().collect::<Vec<u16>>();
        out.extend_from_slice(tail);
        return PathBuf::from(std::ffi::OsString::from_wide(&out));
    }
    if rest.len() >= 2
        && u8::try_from(rest[0]).is_ok_and(|b| b.is_ascii_alphabetic())
        && rest[1] == b':' as u16
    {
        return PathBuf::from(std::ffi::OsString::from_wide(rest));
    }
    path
}

#[cfg(not(windows))]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    path
}

#[cfg(windows)]
fn wide_z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub fn gpui_hwnd() -> windows_sys::Win32::Foundation::HWND {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
    };

    const GPUI_WINDOW_CLASS: &[u16] = &[90, 101, 100, 58, 58, 87, 105, 110, 100, 111, 119, 0];

    unsafe extern "system" fn find_ours(hwnd: HWND, lparam: isize) -> i32 {
        // SAFETY: lparam is passed through verbatim by EnumWindows and points at the
        // `target` local below.
        let out = unsafe { &mut *(lparam as *mut HWND) };
        let mut pid = 0u32;
        // SAFETY: hwnd is a valid window handed to the callback; pid points at a valid local.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        let mut class = [0u16; 32];
        // SAFETY: the class buffer is large enough for the class name ("Zed::Window", 11 chars).
        let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), 32) } as usize;
        let is_gpui = class[..len] == GPUI_WINDOW_CLASS[..GPUI_WINDOW_CLASS.len() - 1];
        // SAFETY: same as above, hwnd comes from the system callback.
        if pid == std::process::id() && unsafe { IsWindowVisible(hwnd) != 0 } && is_gpui {
            *out = hwnd;
            return 0;
        }
        1
    }

    // SAFETY: target is a local; EnumWindows fills it through lparam within this call.
    unsafe {
        let mut target: HWND = std::ptr::null_mut();
        EnumWindows(Some(find_ours), &mut target as *mut HWND as isize);
        target
    }
}

#[cfg(windows)]
fn markdown_filter_wide() -> Vec<u16> {
    let label = format!(
        "{} (*.md;*.markdown)",
        md_i18n::t(md_i18n::Key::DlgMarkdownFilter)
    );
    let mut filter = Vec::new();
    filter.extend(label.encode_utf16());
    filter.push(0);
    filter.extend("*.md;*.markdown".encode_utf16());
    filter.push(0);
    filter.push(0);
    filter
}

#[cfg(windows)]
fn fill_file_buffer(name: &std::ffi::OsStr, capacity: usize) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    let mut file = vec![0u16; capacity];
    if capacity == 0 {
        return file;
    }
    let wide: Vec<u16> = name.encode_wide().collect();
    let n = wide.len().min(capacity - 1);
    file[..n].copy_from_slice(&wide[..n]);
    file
}

#[cfg(windows)]
fn path_from_file_buffer(file: &[u16]) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;

    let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
    (len > 0).then(|| PathBuf::from(std::ffi::OsString::from_wide(&file[..len])))
}

#[cfg(windows)]
pub fn pick_markdown_file(
    owner: windows_sys::Win32::Foundation::HWND,
) -> Option<std::path::PathBuf> {
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_DONTADDTORECENT, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY,
        OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };

    let filter = markdown_filter_wide();
    let title = wide_z(md_i18n::t(md_i18n::Key::DlgOpenMarkdown));
    let def_ext: Vec<u16> = "md\0".encode_utf16().collect();
    let mut file = vec![0u16; 32768];

    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        hInstance: std::ptr::null_mut(),
        lpstrFilter: filter.as_ptr(),
        lpstrCustomFilter: std::ptr::null_mut(),
        nMaxCustFilter: 0,
        nFilterIndex: 1,
        lpstrFile: file.as_mut_ptr(),
        nMaxFile: file.len() as u32,
        lpstrFileTitle: std::ptr::null_mut(),
        nMaxFileTitle: 0,
        lpstrInitialDir: std::ptr::null(),
        lpstrTitle: title.as_ptr(),
        Flags: OFN_EXPLORER
            | OFN_FILEMUSTEXIST
            | OFN_PATHMUSTEXIST
            | OFN_HIDEREADONLY
            | OFN_NOCHANGEDIR
            | OFN_DONTADDTORECENT,
        nFileOffset: 0,
        nFileExtension: 0,
        lpstrDefExt: def_ext.as_ptr(),
        lCustData: 0,
        lpfnHook: None,
        lpTemplateName: std::ptr::null(),
        pvReserved: std::ptr::null_mut(),
        dwReserved: 0,
        FlagsEx: 0,
    };

    // SAFETY: ofn is fully initialized above; the system only reads the buffers it points at.
    let ok = unsafe { GetOpenFileNameW(&mut ofn) } != 0;
    if !ok {
        return None;
    }
    path_from_file_buffer(&file)
}

#[cfg(windows)]
pub fn pick_markdown_save_path(
    owner: windows_sys::Win32::Foundation::HWND,
    suggested: Option<&Path>,
) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetSaveFileNameW, OFN_DONTADDTORECENT, OFN_EXPLORER, OFN_HIDEREADONLY, OFN_NOCHANGEDIR,
        OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };

    let filter = markdown_filter_wide();
    let title = wide_z(md_i18n::t(md_i18n::Key::DlgSaveMarkdown));
    let def_ext: Vec<u16> = "md\0".encode_utf16().collect();
    let name = suggested
        .and_then(|p| p.file_name())
        .unwrap_or_else(|| std::ffi::OsStr::new("untitled.md"));
    let mut file = fill_file_buffer(name, 32768);

    let mut initial_dir: Vec<u16> = Vec::new();
    let initial_ptr = suggested
        .and_then(|p| p.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .map(|dir| {
            initial_dir = dir.as_os_str().encode_wide().chain(Some(0)).collect();
            initial_dir.as_ptr()
        })
        .unwrap_or(std::ptr::null());

    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        hInstance: std::ptr::null_mut(),
        lpstrFilter: filter.as_ptr(),
        lpstrCustomFilter: std::ptr::null_mut(),
        nMaxCustFilter: 0,
        nFilterIndex: 1,
        lpstrFile: file.as_mut_ptr(),
        nMaxFile: file.len() as u32,
        lpstrFileTitle: std::ptr::null_mut(),
        nMaxFileTitle: 0,
        lpstrInitialDir: initial_ptr,
        lpstrTitle: title.as_ptr(),
        Flags: OFN_EXPLORER
            | OFN_PATHMUSTEXIST
            | OFN_HIDEREADONLY
            | OFN_NOCHANGEDIR
            | OFN_OVERWRITEPROMPT
            | OFN_DONTADDTORECENT,
        nFileOffset: 0,
        nFileExtension: 0,
        lpstrDefExt: def_ext.as_ptr(),
        lCustData: 0,
        lpfnHook: None,
        lpTemplateName: std::ptr::null(),
        pvReserved: std::ptr::null_mut(),
        dwReserved: 0,
        FlagsEx: 0,
    };

    // SAFETY: ofn is fully initialized above; the system only reads the buffers it points at.
    let ok = unsafe { GetSaveFileNameW(&mut ofn) } != 0;
    if !ok {
        return None;
    }
    path_from_file_buffer(&file)
}

#[cfg(test)]
mod ext_tests {
    use super::{OpenTarget, classify_dest, is_markdown_path, with_markdown_extension};
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn a_bare_name_gets_md() {
        assert_eq!(
            with_markdown_extension(PathBuf::from("notes")),
            PathBuf::from("notes.md")
        );
        assert_eq!(
            with_markdown_extension(PathBuf::from("dir/notes")),
            PathBuf::from("dir/notes.md")
        );

        assert_eq!(
            with_markdown_extension(PathBuf::from("notes.")),
            PathBuf::from("notes.md")
        );
    }

    #[test]
    fn markdown_names_stay_put() {
        for name in ["notes.md", "notes.markdown", "Notes.MD"] {
            let path = PathBuf::from(name);
            assert!(is_markdown_path(&path), "{name}");
            assert_eq!(with_markdown_extension(path.clone()), path);
        }
    }

    #[test]
    fn a_different_extension_is_left_alone() {
        let path = PathBuf::from("notes.txt");
        assert!(!is_markdown_path(&path));
        assert_eq!(with_markdown_extension(path.clone()), path);
    }

    #[test]
    fn local_executables_and_unc_paths_require_confirmation() {
        let source = Path::new("C:/notes/readme.md");
        assert!(matches!(
            classify_dest("payload.exe", Some(source)),
            OpenTarget::ConfirmLocal(path) if path.ends_with("payload.exe")
        ));
        assert!(matches!(
            classify_dest(r"\\server\share\guide.md", Some(source)),
            OpenTarget::ConfirmLocal(path) if path.to_string_lossy().contains("server")
        ));
    }

    #[test]
    fn a_real_link_target_never_carries_a_verbatim_prefix() {
        use std::fs;

        let dir = std::env::temp_dir();
        let name = format!("md-test-verbatim-{}.md", std::process::id());
        let file = dir.join(&name);
        fs::write(&file, "probe\n").expect("seed");
        let source = dir.join("readme.md");
        let target = match classify_dest(&name, Some(&source)) {
            OpenTarget::Local(path) => path,
            other => panic!("expected Local, got {other:?}"),
        };
        let _ = fs::remove_file(&file);
        assert!(target.is_absolute(), "{target:?}");
        assert!(target.ends_with(&name), "{target:?}");
        assert!(
            !target.to_string_lossy().starts_with(r"\\?\"),
            "a verbatim prefix leaked into the open path: {target:?}"
        );
    }

    #[test]
    fn ordinary_documents_are_safe_but_unknown_schemes_are_blocked() {
        let source = Path::new("C:/notes/readme.md");
        assert!(matches!(
            classify_dest("guide.pdf", Some(source)),
            OpenTarget::Local(path) if path.ends_with("guide.pdf")
        ));
        assert_eq!(
            classify_dest("custom://run", Some(source)),
            OpenTarget::Ignore
        );
        assert_eq!(
            classify_dest("https://example.com", Some(source)),
            OpenTarget::Url("https://example.com".into())
        );
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::{
        fill_file_buffer, markdown_filter_wide, path_from_file_buffer, strip_verbatim, wide_z,
    };
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn verbatim_prefixes_are_stripped_to_the_plain_forms() {
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\C:\notes\a.md")),
            PathBuf::from(r"C:\notes\a.md")
        );
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\UNC\server\share\a.md")),
            PathBuf::from(r"\\server\share\a.md")
        );

        assert_eq!(
            strip_verbatim(PathBuf::from(r"C:\notes\a.md")),
            PathBuf::from(r"C:\notes\a.md")
        );
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\BootPartition\a.md")),
            PathBuf::from(r"\\?\BootPartition\a.md")
        );
    }

    #[test]
    fn the_wide_string_ends_where_win32_expects() {
        let w = wide_z("ab");
        assert_eq!(w, vec![b'a' as u16, b'b' as u16, 0]);
        assert_eq!(
            wide_z(""),
            vec![0],
            "even the empty string needs its terminator"
        );

        let title = wide_z(md_i18n::t(md_i18n::Key::DlgOpenMarkdown));
        assert_eq!(title.iter().filter(|u| **u == 0).count(), 1, "{title:?}");
        assert!(title.len() > 1, "the title must not be empty");
    }

    #[test]
    fn the_filter_is_label_pattern_and_double_nul() {
        let label = format!(
            "{} (*.md;*.markdown)",
            md_i18n::t(md_i18n::Key::DlgMarkdownFilter)
        );
        assert!(!label.contains('\0'), "{label}");
        let f = markdown_filter_wide();
        assert!(f.len() >= 3, "{f:?}");
        assert_eq!(f[f.len() - 2], 0);
        assert_eq!(f[f.len() - 1], 0);
        assert_eq!(
            f.iter().filter(|u| **u == 0).count(),
            3,
            "the label, the pattern, and the terminator each need one NUL: {f:?}"
        );
    }

    #[test]
    fn a_dialog_buffer_round_trips_unicode_and_stops_at_nul() {
        let file = fill_file_buffer(OsStr::new("café.md"), 64);
        assert_eq!(path_from_file_buffer(&file), Some(PathBuf::from("café.md")));

        let suffix = "ignored.md".encode_utf16();
        let mut with_suffix = "chosen.md".encode_utf16().collect::<Vec<_>>();
        with_suffix.push(0);
        with_suffix.extend(suffix);
        assert_eq!(
            path_from_file_buffer(&with_suffix),
            Some(PathBuf::from("chosen.md"))
        );
    }

    #[test]
    fn a_dialog_buffer_reserves_its_last_cell_for_nul() {
        let file = fill_file_buffer(OsStr::new("abcdef"), 4);
        assert_eq!(file.len(), 4);
        assert_eq!(file[3], 0);
        assert_eq!(path_from_file_buffer(&file), Some(PathBuf::from("abc")));
        assert!(fill_file_buffer(OsStr::new("x"), 0).is_empty());
        assert_eq!(path_from_file_buffer(&[]), None);
        assert_eq!(path_from_file_buffer(&[0, b'x' as u16]), None);
    }
}

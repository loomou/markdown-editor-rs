use super::Shell;
use gpui::{
    App, Div, Entity, InteractiveElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Stateful, Window, WindowControlArea,
};

#[cfg(windows)]
use crate::platform::open_markdown::gpui_hwnd;

#[cfg(windows)]
fn raise_gpui_window() {
    use windows_sys::Win32::Foundation::{FALSE, TRUE};
    use windows_sys::Win32::System::Threading::AttachThreadInput;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, HWND_TOP, SWP_NOMOVE,
        SWP_NOSIZE, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowPos,
    };

    let target = gpui_hwnd();
    if target.is_null() {
        return;
    }
    unsafe {
        let mut target_pid = 0u32;
        let target_tid = GetWindowThreadProcessId(target, &mut target_pid);
        let fg = GetForegroundWindow();
        let mut fg_pid = 0u32;
        let fg_tid = if fg.is_null() {
            0
        } else {
            GetWindowThreadProcessId(fg, &mut fg_pid)
        };
        let attached = fg_tid != 0
            && target_tid != 0
            && fg_tid != target_tid
            && AttachThreadInput(target_tid, fg_tid, TRUE) != 0;
        SetForegroundWindow(target);
        BringWindowToTop(target);
        SetWindowPos(
            target,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        if attached {
            AttachThreadInput(target_tid, fg_tid, FALSE);
        }
    }
}

#[cfg(windows)]
fn set_gpui_topmost(topmost: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };

    let target = gpui_hwnd();
    if target.is_null() {
        return;
    }
    let insert_after = if topmost {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    let flags = if topmost {
        SWP_NOMOVE | SWP_NOSIZE
    } else {
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
    };
    unsafe {
        SetWindowPos(target, insert_after, 0, 0, 0, 0, flags);
    }
}

#[cfg(windows)]
fn restore_zorder_after_caption_drag() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};

    let _ = std::thread::spawn(|| {
        for _ in 0..600 {
            std::thread::sleep(std::time::Duration::from_millis(16));
            let down = unsafe { GetAsyncKeyState(VK_LBUTTON as i32) as u16 } & 0x8000 != 0;
            if !down {
                break;
            }
        }
        set_gpui_topmost(false);
    });
}

#[cfg(windows)]
fn start_caption_drag(window: &Window) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HTCAPTION, PostMessageW, SC_MOVE, WM_SYSCOMMAND,
    };

    window.activate_window();
    raise_gpui_window();
    set_gpui_topmost(true);
    let target = gpui_hwnd();
    if target.is_null() {
        set_gpui_topmost(false);
        return;
    }
    unsafe {
        ReleaseCapture();
        PostMessageW(
            target,
            WM_SYSCOMMAND,
            (SC_MOVE as usize) | (HTCAPTION as usize),
            0,
        );
    }
    restore_zorder_after_caption_drag();
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
fn start_caption_drag(window: &Window) {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    window.activate_window();

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let ns_view = appkit.ns_view.as_ptr().cast::<Object>();

    unsafe {
        let ns_window: *mut Object = msg_send![ns_view, window];
        if ns_window.is_null() {
            return;
        }
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let event: *mut Object = msg_send![app, currentEvent];
        if event.is_null() {
            return;
        }
        let _: () = msg_send![ns_window, performWindowDragWithEvent: event];
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn start_caption_drag(window: &Window) {
    window.activate_window();
    window.start_window_move();
}

pub(super) fn toggle_zoom(window: &Window) {
    #[cfg(target_os = "macos")]
    window.titlebar_double_click();
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SW_MAXIMIZE, SW_NORMAL, ShowWindowAsync,
        };
        let hwnd = gpui_hwnd();
        if hwnd.is_null() {
            return;
        }
        let cmd = if window.is_maximized() {
            SW_NORMAL
        } else {
            SW_MAXIMIZE
        };
        unsafe {
            ShowWindowAsync(hwnd, cmd);
        }
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    window.zoom_window();
}

pub(super) fn bind_caption_drag(el: Stateful<Div>, this: Entity<Shell>) -> Stateful<Div> {
    el.window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, {
            let this = this.clone();
            move |ev: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                window.activate_window();
                #[cfg(windows)]
                raise_gpui_window();
                if ev.click_count >= 2 {
                    this.update(cx, |shell, _| shell.caption_should_move = false);
                    toggle_zoom(window);
                    return;
                }
                this.update(cx, |shell, _| shell.caption_should_move = true);
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let this = this.clone();
            move |_: &MouseUpEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |shell, _| shell.caption_should_move = false);
            }
        })
        .on_mouse_down_out({
            let this = this.clone();
            move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |shell, _| shell.caption_should_move = false);
            }
        })
        .on_mouse_move({
            let this = this;
            move |_: &MouseMoveEvent, window: &mut Window, cx: &mut App| {
                let should_move = this.update(cx, |shell, _| {
                    let should_move = shell.caption_should_move;
                    shell.caption_should_move = false;
                    should_move
                });
                if should_move {
                    start_caption_drag(window);
                }
            }
        })
}

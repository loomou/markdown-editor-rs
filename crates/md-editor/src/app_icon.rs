pub const APP_ID: &str = "md-test";

pub fn apply() {
    #[cfg(target_os = "macos")]
    macos::apply();
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    linux::apply();
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod macos {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use std::os::raw::c_void;

    const PNG: &[u8] = include_bytes!("../resources/app-icon.png");

    pub(super) fn apply() {
        unsafe {
            let data: *mut Object = msg_send![
                class!(NSData),
                dataWithBytes: PNG.as_ptr() as *const c_void
                length: PNG.len()
            ];
            if data.is_null() {
                return;
            }
            let image: *mut Object = msg_send![class!(NSImage), alloc];
            let image: *mut Object = msg_send![image, initWithData: data];
            if image.is_null() {
                return;
            }
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, setApplicationIconImage: image];
            let _: () = msg_send![image, release];
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, PropMode, Window};
    use x11rb::wrapper::ConnectionExt as _;

    const NET_WM_ICON: &[u8] = include_bytes!("../resources/linux/net-wm-icon.bin");

    pub(super) fn apply() {
        let Ok((conn, screen)) = x11rb::connect(None) else {
            return;
        };
        let Some(icon_atom) = intern(&conn, b"_NET_WM_ICON") else {
            return;
        };
        let Some(pid_atom) = intern(&conn, b"_NET_WM_PID") else {
            return;
        };
        let data = icon_u32s();
        if data.is_empty() {
            return;
        }
        let root = conn.setup().roots[screen].root;
        let pid = std::process::id();
        let mut stack = vec![root];
        while let Some(window) = stack.pop() {
            if window_pid(&conn, window, pid_atom) == Some(pid) {
                let _ = conn.change_property32(
                    PropMode::REPLACE,
                    window,
                    icon_atom,
                    AtomEnum::CARDINAL,
                    &data,
                );
            }
            if let Ok(cookie) = conn.query_tree(window)
                && let Ok(tree) = cookie.reply()
            {
                stack.extend(tree.children);
            }
        }
        let _ = conn.flush();
    }

    fn intern(conn: &impl Connection, name: &[u8]) -> Option<Atom> {
        conn.intern_atom(false, name)
            .ok()?
            .reply()
            .ok()
            .map(|r| r.atom)
    }

    fn window_pid(conn: &impl Connection, window: Window, pid_atom: Atom) -> Option<u32> {
        let cookie = conn
            .get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)
            .ok()?;
        let reply = cookie.reply().ok()?;
        reply.value32()?.next()
    }

    fn icon_u32s() -> Vec<u32> {
        NET_WM_ICON
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_x11_icon_blob_is_well_formed() {
        let bytes = include_bytes!("../resources/linux/net-wm-icon.bin");
        assert!(!bytes.is_empty());
        assert_eq!(bytes.len() % 4, 0);
        let words: Vec<u32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let mut i = 0;
        let mut sizes = 0u32;
        while i < words.len() {
            assert!(i + 2 <= words.len());
            let w = words[i] as usize;
            let h = words[i + 1] as usize;
            assert!(w > 0 && w <= 512 && h > 0 && h <= 512, "{w}x{h}");
            i += 2 + w * h;
            sizes += 1;
        }
        assert_eq!(i, words.len());
        assert!(sizes >= 1);
    }
}

use crate::platform::fs_atomic::write_bytes_atomic;
use gpui::{Bounds, Pixels, px};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const WINDOW_VERSION: u64 = 1;
const MIN_RESTORABLE_WIDTH: f32 = 200.0;
const MIN_RESTORABLE_HEIGHT: f32 = 120.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowGeometry {
    pub bounds: Bounds<Pixels>,
    pub maximized: bool,
}

impl WindowGeometry {
    pub fn from_window(bounds: Bounds<Pixels>, maximized: bool) -> Self {
        Self { bounds, maximized }
    }

    pub fn restore_bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    pub fn center_is_visible(&self, displays: &[Bounds<Pixels>]) -> bool {
        let center = self.bounds.center();
        displays.iter().any(|display| {
            center.x >= display.left()
                && center.x < display.right()
                && center.y >= display.top()
                && center.y < display.bottom()
        })
    }

    pub fn is_restorable(&self) -> bool {
        let (x, y) = (
            f32::from(self.bounds.origin.x),
            f32::from(self.bounds.origin.y),
        );
        let (w, h) = (
            f32::from(self.bounds.size.width),
            f32::from(self.bounds.size.height),
        );
        [x, y, w, h].iter().all(|v| v.is_finite())
            && w >= MIN_RESTORABLE_WIDTH
            && h >= MIN_RESTORABLE_HEIGHT
    }
}

#[derive(Clone, Debug)]
pub struct WindowStore {
    path: PathBuf,
}

impl WindowStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn discover() -> Option<Self> {
        Some(Self::at(crate::platform::paths::window_path()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Option<WindowGeometry> {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return None;
        };
        parse(&text).filter(WindowGeometry::is_restorable)
    }

    pub fn save(&self, geometry: WindowGeometry) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        write_bytes_atomic(&self.path, encode(geometry).as_bytes())
    }
}

fn encode(geometry: WindowGeometry) -> String {
    let mut root = serde_json::Map::new();
    root.insert("version".into(), WINDOW_VERSION.into());
    root.insert(
        "x".into(),
        f64::from(f32::from(geometry.bounds.origin.x)).into(),
    );
    root.insert(
        "y".into(),
        f64::from(f32::from(geometry.bounds.origin.y)).into(),
    );
    root.insert(
        "width".into(),
        f64::from(f32::from(geometry.bounds.size.width)).into(),
    );
    root.insert(
        "height".into(),
        f64::from(f32::from(geometry.bounds.size.height)).into(),
    );
    root.insert("maximized".into(), geometry.maximized.into());
    let mut text = serde_json::to_string_pretty(&serde_json::Value::Object(root))
        .expect("a tree made of maps, strings, and numbers always serializes");
    text.push('\n');
    text
}

fn parse(text: &str) -> Option<WindowGeometry> {
    let root = serde_json::from_str::<serde_json::Value>(text).ok()?;
    if root.get("version").and_then(serde_json::Value::as_u64) != Some(WINDOW_VERSION) {
        return None;
    }
    let field = |name: &str| root.get(name).and_then(serde_json::Value::as_f64);
    Some(WindowGeometry {
        bounds: Bounds {
            origin: gpui::point(px(field("x")? as f32), px(field("y")? as f32)),
            size: gpui::size(px(field("width")? as f32), px(field("height")? as f32)),
        },
        maximized: root.get("maximized")?.as_bool()?,
    })
}

#[cfg(test)]
mod tests {
    use super::{WindowGeometry, WindowStore, encode, parse};
    use gpui::{Bounds, px};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_path(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!(
            "markdown-editor-rs-window-{tag}-{}-{n}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&p);
        p
    }

    fn sample(maximized: bool) -> WindowGeometry {
        WindowGeometry {
            bounds: Bounds {
                origin: gpui::point(px(120.5), px(-40.0)),
                size: gpui::size(px(1024.0), px(768.0)),
            },
            maximized,
        }
    }

    #[test]
    fn an_offscreen_center_is_not_visible() {
        let displays = [Bounds {
            origin: gpui::point(px(0.0), px(0.0)),
            size: gpui::size(px(1920.0), px(1080.0)),
        }];
        assert!(sample(false).center_is_visible(&displays));
        let offscreen = WindowGeometry::from_window(
            Bounds {
                origin: gpui::point(px(5000.0), px(5000.0)),
                size: gpui::size(px(1024.0), px(768.0)),
            },
            false,
        );
        assert!(!offscreen.center_is_visible(&displays));
        let two = [
            Bounds {
                origin: gpui::point(px(0.0), px(0.0)),
                size: gpui::size(px(1920.0), px(1080.0)),
            },
            Bounds {
                origin: gpui::point(px(1920.0), px(0.0)),
                size: gpui::size(px(1280.0), px(1024.0)),
            },
        ];
        let straddling = WindowGeometry::from_window(
            Bounds {
                origin: gpui::point(px(1500.0), px(100.0)),
                size: gpui::size(px(1000.0), px(800.0)),
            },
            false,
        );
        assert!(straddling.center_is_visible(&two));
        assert!(!sample(false).center_is_visible(&[]));
    }

    #[test]
    fn a_store_round_trips_windowed_and_maximized_geometry() {
        let path = unique_path("roundtrip");
        let store = WindowStore::at(path.clone());
        for maximized in [false, true] {
            let geometry = sample(maximized);
            store.save(geometry).expect("save");
            assert_eq!(store.load(), Some(geometry));
        }
        assert!(path.is_file());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_or_corrupt_file_loads_none() {
        let store = WindowStore::at(unique_path("missing"));
        assert_eq!(store.load(), None);
        let path = unique_path("corrupt");
        std::fs::write(&path, "not json").expect("seed");
        assert_eq!(WindowStore::at(path.clone()).load(), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_unknown_version_loads_none() {
        assert_eq!(
            parse(
                r#"{"version": 99, "x": 0.0, "y": 0.0, "width": 800.0, "height": 600.0, "maximized": false}"#
            ),
            None
        );
        assert_eq!(parse(&encode(sample(false))), Some(sample(false)));
    }

    #[test]
    fn absurd_geometry_is_rejected_on_load() {
        for (w, h) in [(10.0, 600.0), (800.0, 0.0)] {
            let text = format!(
                r#"{{"version": 1, "x": 0.0, "y": 0.0, "width": {w}, "height": {h}, "maximized": false}}"#
            );
            assert!(parse(&text).is_some(), "parse should succeed: {w}x{h}");
            assert_eq!(load_from_text(&text), None, "load must reject: {w}x{h}");
        }
    }

    #[test]
    fn non_finite_values_do_not_survive_the_text_form() {
        let text = r#"{"version": 1, "x": 0.0, "y": 0.0, "width": NaN, "height": 600.0, "maximized": false}"#;
        assert_eq!(parse(text), None);
        assert_eq!(load_from_text(text), None);
    }

    fn load_from_text(text: &str) -> Option<WindowGeometry> {
        let path = unique_path("absurd-file");
        std::fs::write(&path, text).expect("seed");
        let loaded = WindowStore::at(path.clone()).load();
        let _ = std::fs::remove_file(&path);
        loaded
    }
}

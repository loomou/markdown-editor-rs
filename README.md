# Markdown Editor RS

A native desktop Markdown editor built with Rust and GPUI: you edit the rendered document directly, and the Markdown source appears when you enter an editing state.

> [!NOTE]
> This is an experimental project. It has only been verified on Windows so far; macOS and Linux have adaptations and CI builds but have not been verified on real machines. Do not rely on it for important files.

## Features

- **Native app** — built with Rust and GPUI, no browser or Electron runtime
- **WYSIWYG editing** — edit the rendered document directly; the Markdown source appears when you enter an editing state
- **Designed for long documents** — incremental, viewport-driven layout: only the visible region is measured and composed precisely

Standard Markdown content — tables, footnotes, task lists, GitHub-style alerts, YAML front matter, syntax-highlighted code (14 languages), Mermaid diagrams, LaTeX math, and images — is supported in the same document.

## Get the app

### From source (recommended)

Requires Rust 1.96 or newer:

```sh
cargo build --release --locked
```

The binary is produced at `target/release/md-editor` (`md-editor.exe` on Windows).

### From a release

Prebuilt binaries are published on the [releases page](https://github.com/loomou/markdown-editor-rs/releases) for Windows x64, macOS (Apple Silicon and Intel), and Linux x64.

- **Windows** — unzip and run `md-editor.exe`; it is statically linked and needs no extra runtime
- **macOS** — unzip and move `md-editor.app` to `Applications`; the bundle is unsigned, so open it via right-click → Open on first launch
- **Linux** — extract the tarball; windowing and font libraries (wayland, xkbcommon, x11, fontconfig, …) must be present on the system

## Platform status

| Platform | Status |
| --- | --- |
| Windows | Verified on real machines |
| macOS | Adaptations and CI builds only, not verified |
| Linux | Adaptations and CI builds only, not verified |

## Current limitations

- Saving preserves source forms where possible but normalizes line endings and some structures; byte-for-byte preservation is not guaranteed
- Autosave is off by default; unsaved edits have a separate recovery draft that does not replace deliberate saves
- Remote images are off by default; when enabled, public-address, size, and request limits still apply
- HTML source can be retained, but the editor is not a browser and does not execute page scripts
- Diagrams and math use the pinned rendering libraries; full Mermaid or LaTeX syntax coverage is not assumed

## License

MIT

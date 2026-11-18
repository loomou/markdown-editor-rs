# Markdown Editor RS

[English](README.md) | 简体中文

一款用 Rust 和 GPUI 构建的原生桌面 Markdown 编辑器：你直接在渲染后的文档上编辑，进入编辑状态时才会显示 Markdown 源码。

> [!NOTE]
> 这是一个实验性项目。目前只在 Windows 上验证过；macOS 和 Linux 已经做了适配、也有 CI 构建，但没有在真机上验证。请不要用它处理重要文件。

## 特性

- **原生应用** — 用 Rust 和 GPUI 构建，不需要浏览器或 Electron 运行时
- **所见即所得编辑** — 直接编辑渲染后的文档，进入编辑状态时才显示 Markdown 源码
- **为长文档设计** — 增量式、由视口驱动的布局：只对可见区域做精确测量和排版

标准 Markdown 内容都可以出现在同一篇文档里 —— 表格、脚注、任务列表、GitHub 风格提示框（alerts）、YAML front matter、带语法高亮的代码（14 种语言）、Mermaid 图表、LaTeX 数学公式和图片。

## 获取应用

### 从源码构建（推荐）

需要 Rust 1.96 或更新版本：

```sh
cargo build --release --locked
```

产物在 `target/release/md-editor`（Windows 上是 `md-editor.exe`）。

### 从 Release 下载

[releases 页面](https://github.com/loomou/markdown-editor-rs/releases)提供 Windows x64、macOS（Apple Silicon 和 Intel）以及 Linux x64 的预编译二进制。

- **Windows** — 解压后运行 `md-editor.exe`；它是静态链接的，不需要额外运行时
- **macOS** — 解压后把 `md-editor.app` 移到「应用程序」；这个 bundle 未签名，首次启动请用右键 → 打开
- **Linux** — 解压 tarball；系统上需要有窗口和字体相关的库（wayland、xkbcommon、x11、fontconfig 等）

## 平台状态

| 平台 | 状态 |
| --- | --- |
| Windows | 已在真机验证 |
| macOS | 只有适配和 CI 构建，未经验证 |
| Linux | 只有适配和 CI 构建，未经验证 |

## 当前限制

- 保存时会尽量保留原有的源码写法，但会统一换行符、规范化部分结构；不保证逐字节一致
- 自动保存默认关闭；未保存的修改有独立的恢复草稿，不会覆盖你主动保存的内容
- 远程图片默认关闭；启用之后，公网地址、大小和请求次数的限制仍然生效
- HTML 源码可以保留，但编辑器不是浏览器，不会执行页面脚本
- 图表和公式使用固定版本的渲染库；不保证覆盖完整的 Mermaid 或 LaTeX 语法

## 许可协议

MIT

# TermShot · 终端截图（Rust）

跨平台桌面托盘 / 菜单栏常驻截图工具（支持 **Windows** 与 **macOS**）。框选或滚动长图后可保存 PNG、复制图片或路径、贴图、识别文字与翻译，适合终端和日常办公场景。

没有传统主窗口。左键单击托盘 / 菜单栏图标可打开快捷面板，右键保留系统菜单；进程由系统托盘 / 菜单栏图标常驻。

本目录是 C# 版 TermShot 的 Rust 原生实现（Windows 使用 Win32，macOS 使用 AppKit / ScreenCaptureKit / Carbon），行为对齐原版。

## 功能

- **框选截图**：快捷键或托盘/菜单栏快捷面板，冻结当前画面，准星 + 放大镜，悬停吸附窗口，拖动框选
- **滚动长图**：框选后按 `R`。用鼠标滚轮向下滚，Enter 完成 / Esc 取消
- **截完后**：每次询问，或直接保存 / 复制图片 / 复制路径 / 贴到桌面
- **标注**：方框/椭圆（Tab，Shift 正方形/正圆）、直线/箭头（`A` Tab，Shift 45°）、铅笔 `B`、荧光笔 `H`、马赛克 `M`、文字 `X`、橡皮 `E`、撤销 `Z`
- **贴图**：钉在桌面上，可拖动、缩放、调整透明度；悬停工具条支持复制、主动 OCR、保存和关闭
- **文字识别 / 截图翻译**：DeepSeek 视觉（默认 `deepseek-flash`），在统一结果窗口中查看、复制或失败重试
- **剪贴板翻译**：复制文字后按独立快捷键，中英自动互译并在置顶结果窗口中上下显示原文与译文
- **隐私与剪贴板**：AI 仅在主动操作时调用，结果不会自动覆盖剪贴板，需要点击“复制结果”
- **快捷面板**：集中提供截图、翻译剪贴板、贴图管理、保存目录和设置入口
- **安装**：
  - **Windows**：免管理员，复制到 `%LocalAppData%\TermShot`，可选开机启动
  - **macOS**：免管理员，安装到 `~/Applications/TermShot.app`（`install.sh` / `uninstall.sh` 或 GUI 安装向导），配置存储在 `~/Library/Application Support/TermShot`，可选通过 `~/Library/LaunchAgents` 开机启动

默认截图快捷键 `Ctrl+Shift+S`，默认剪贴板翻译快捷键 `Ctrl+Shift+T`；两者均可在设置中修改（macOS 设置中也支持录制包含 `Cmd` 修饰键的快捷键）。

剪贴板翻译单次最多处理 20,000 个 Unicode 字符，需要先在设置中填写并可验证 DeepSeek API Key。中文内容默认译成英文，其他语言默认译成简体中文。

## 环境

- **Windows**：Windows 10 / 11
- **macOS**：macOS 14.0+（Apple Silicon / Intel，使用 ScreenCaptureKit 截图；首次截图需在 `系统设置 → 隐私与安全性 → 屏幕录制` 授权）
- [Rust](https://rustup.rs)（开发时；macOS 需 Xcode Command Line Tools）

## 从源码运行

### macOS

```bash
cargo run --release
```

### Windows

```powershell
cargo run --release
```

## 打包与安装

### macOS

```bash
./build.sh
```

生成 `dist/TermShot.app`、`dist/TermShot-macOS.zip` 以及安装/卸载脚本 `dist/install.sh`、`dist/uninstall.sh`（进入 `dist/` 目录也可运行 `./dist/install.sh`）。安装与卸载：

```bash
./install.sh     # 启动 GUI 安装向导（安装到 ~/Applications/TermShot.app；加 --silent 可静默安装）
./uninstall.sh   # 卸载应用、配置目录与 LaunchAgent 启动项
```

### Windows

```powershell
.\build.ps1
```

生成 `dist\TermShot.exe` 和 `dist\TermShot-Setup.exe`。

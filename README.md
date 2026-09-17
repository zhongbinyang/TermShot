# TermShot · 终端截图（Rust）

Windows 托盘常驻截图工具。框选或滚动长图后保存 PNG，并把带引号的文件路径写入剪贴板，方便直接粘到终端。

没有主窗口。进程只靠托盘图标存活；退出只走托盘菜单里的「退出」。

本目录是 C# 版 TermShot 的 Rust/Win32 实现，行为对齐原版。

## 功能

- **框选截图**：快捷键或托盘左键，冻结当前画面，准星 + 放大镜，悬停吸附窗口，拖动框选
- **滚动长图**：框选后按 `R`。用鼠标滚轮向下滚，Enter 完成 / Esc 取消
- **截完后**：每次询问，或直接保存 / 复制图片 / 复制路径 / 贴到桌面
- **标注**：方框/椭圆（Tab，Shift 正方形/正圆）、直线/箭头（`A` Tab，Shift 45°）、铅笔 `B`、荧光笔 `H`、马赛克 `M`、文字 `X`、橡皮 `E`、撤销 `Z`
- **贴图**：钉在桌面上，可缩放、拖动；配置 DeepSeek 后可复制识别出的全部文字
- **复制文字 / 翻译**：DeepSeek 视觉（默认 `deepseek-flash`）。翻译目标由模型根据画面文字决定
- **安装**：免管理员，复制到 `%LocalAppData%\TermShot`，可选开机启动

默认快捷键 `Ctrl+Shift+S`。

## 环境

- Windows 10 / 11
- [Rust](https://rustup.rs)（开发时）

## 从源码运行

```powershell
cd C:\Users\zhong\git\TermShot
cargo run --release
```

## 发布

```powershell
.\build.ps1
```

生成 `dist\TermShot.exe` 和 `dist\TermShot-Setup.exe`。

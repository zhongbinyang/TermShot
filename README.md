# TermShot · 终端截图

Windows 托盘常驻截图工具。框选或滚动长图后保存 PNG，并把带引号的文件路径写入剪贴板，方便直接粘到终端。

没有主窗口。进程只靠托盘图标存活；退出只走托盘菜单里的「退出」。

## 功能

- **框选截图**：快捷键或托盘左键，冻结当前画面，准星 + 放大镜，悬停吸附窗口，拖动框选
- **滚动长图**：框选后按 `R`。用鼠标滚轮向下滚，Enter 完成 / Esc 取消
- **截完后**：每次询问，或直接保存 / 复制图片 / 复制路径 / 贴到桌面
- **标注**：框选完成后可在询问工具条里画。同一按钮再点一次取消。方框/椭圆（Tab 切换，Shift 正方形/正圆）、直线/箭头（`A` 选箭头，Tab 切直线，Shift 45°）、铅笔 `B`、荧光笔 `H`、马赛克 `M`、文字 `X`（单击后输入，Enter 确认）、橡皮 `E`（只擦标注）、撤销 `Z`。保存 / 复制 / 贴图时一并写入图片。
- **贴图**：钉在桌面上，可缩放、拖动；配置 DeepSeek 后可复制识别出的全部文字
- **复制文字**：截完后把图发给 DeepSeek（默认 `deepseek-flash`）识别，全文写入剪贴板。API Key 在设置里填写
- **翻译**：截完后一次视觉调用，由模型根据图中文字选择目标语言，译文写入剪贴板
- **安装**：免管理员，复制到 `%LocalAppData%\TermShot`，可选开机启动

默认快捷键 `Ctrl+Shift+S`。保存目录默认是「图片 `\Screenshots`」，文件名 `yyyyMMdd-HHmmss.png`。

## 环境

- Windows 10 / 11
- [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0)（开发或从源码编译时）
- 发布包可做成自包含单文件，目标机器不必先装运行时
- 识别文字 / 翻译：需在设置里填写 [DeepSeek](https://platform.deepseek.com) API Key（默认模型 `deepseek-flash`）。未填写则不能识字或翻译

## 项目结构

```
test04/
├── README.md
├── .gitignore
└── TermShot/                 # 应用程序（WinForms，net8.0-windows）
    ├── TermShot.csproj
    ├── app.manifest          # Per-Monitor DPI
    ├── Program.cs            # 单实例、安装/卸载入口
    ├── TrayContext.cs        # 托盘、热键、截图流程
    ├── AppSettings.cs        # 设置（%LocalAppData%\TermShot\settings.json）
    ├── OverlayForm.cs        # 框选覆盖层
    ├── ActionToolbar.cs      # 截完后工具条
    ├── Annotation.cs         # 截图标注（方框/椭圆/直线/箭头/铅笔/荧光笔/马赛克/文字/橡皮）
    ├── CapturePinForm.cs     # 截完后询问（保存/复制/贴图）
    ├── PastePinForm.cs       # 桌面贴图
    ├── ScrollHudForm.cs      # 滚动截图边框与完成条
    ├── ScrollCaptureService.cs
    ├── Services.cs           # 抓屏、保存、剪贴板、安装
    ├── Native.cs             # Win32 / 主题色
    ├── WindowEnumerator.cs   # 窗口吸附
    ├── HotkeyWindow.cs
    ├── SettingsForm.cs
    ├── SetupForm.cs
    ├── OcrService.cs
    ├── DeepSeekClient.cs
    ├── VisionClient.cs
    ├── VisionImage.cs
    ├── TranslateService.cs
    ├── IconFactory.cs
    ├── Assets/               # 应用图标
    ├── build.ps1             # 发布到 ../dist
    ├── install.ps1
    └── uninstall.ps1
```

运行时产物在 `TermShot/bin`、`TermShot/obj`，发布输出在仓库根下的 `dist/`，均不入库。

## 从源码运行

```powershell
cd TermShot
dotnet run -c Release
```

## 发布安装包

在 `TermShot` 目录执行：

```powershell
.\build.ps1
```

会生成自包含单文件到 `dist\TermShot.exe`，并复制一份 `dist\TermShot-Setup.exe`。运行 Setup 或 `install.ps1` 即可安装。

只要运行时、不要自包含时：

```powershell
.\build.ps1 -FrameworkDependent
```

卸载：托盘退出后运行 `uninstall.ps1`，或 `TermShot.exe --uninstall`。

## 截图操作

| 操作 | 说明 |
| --- | --- |
| 托盘左键 / 「截图」/ 快捷键 | 框选 |
| 框选时按 `R` | 选区后滚轮拼接长图 |
| Esc / 右键 | 取消 |
| 询问工具条 | 标注：方框/椭圆 Tab，箭头 `A` / 直线 Tab，铅笔 `B`，荧光笔 `H`，马赛克 `M`，文字 `X`，橡皮 `E`，撤销 `Z`（Shift 约束正方形、正圆或 45°）· 贴图 `T` · 保存 `S` · 复制图片 `C` · 复制路径 `P` · 复制文字 `O` · 翻译 `L` |

滚动长图时，尽量只框会滚动的正文。滚完停稳再按 Enter。

「复制文字」会把截图发给 DeepSeek 视觉模型，识别结果写入剪贴板。贴图后也可右键「复制全部文字」。未填写 API Key 时这两项不可用。

「翻译」同样发给该模型，一次看图出译文并复制。目标语言由模型根据画面文字自行决定（多为中英互译）。

using System.Runtime.InteropServices;
using System.Text;

namespace TermShot;

internal static class NativeMethods
{
    public const int WM_HOTKEY = 0x0312;
    public const int WM_SETCURSOR = 0x0020;
    public const int HTCLIENT = 1;
    public const int HTTRANSPARENT = -1;
    public const int IDC_CROSS = 32515;
    public const int INPUT_MOUSE = 0;
    public const uint MOUSEEVENTF_WHEEL = 0x0800;
    public const int WHEEL_DELTA = 120;
    public const uint MOD_ALT = 0x0001;
    public const uint MOD_CONTROL = 0x0002;
    public const uint MOD_SHIFT = 0x0004;
    public const uint MOD_WIN = 0x0008;
    public const uint MOD_NOREPEAT = 0x4000;

    public const int SM_XVIRTUALSCREEN = 76;
    public const int SM_YVIRTUALSCREEN = 77;
    public const int SM_CXVIRTUALSCREEN = 78;
    public const int SM_CYVIRTUALSCREEN = 79;

    public const int DWMWA_EXTENDED_FRAME_BOUNDS = 9;
    public const int DWMWA_CLOAKED = 14;
    public const uint SRCCOPY = 0x00CC0020;
    public const int GWL_EXSTYLE = -20;
    public const int WS_EX_TOOLWINDOW = 0x00000080;
    public const int WS_EX_NOACTIVATE = 0x08000000;
    public const int WS_EX_TOPMOST = 0x00000008;
    public const int HWND_TOPMOST = -1;
    public const int CS_DROPSHADOW = 0x00020000;
    public const uint SWP_SHOWWINDOW = 0x0040;
    public const uint SWP_NOACTIVATE = 0x0010;
    public const uint SWP_HIDEWINDOW = 0x0080;
    public const uint SWP_NOMOVE = 0x0002;
    public const uint SWP_NOSIZE = 0x0001;
    public const int IMAGE_ICON = 1;
    public const int LR_DEFAULTSIZE = 0x0040;
    public const int IDI_APPLICATION = 32512;

    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool UnregisterHotKey(IntPtr hWnd, int id);

    [DllImport("user32.dll")]
    public static extern int GetSystemMetrics(int nIndex);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool IsIconic(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool IsWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    public static extern IntPtr GetDC(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern int ReleaseDC(IntPtr hWnd, IntPtr hDC);

    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int x, int y, int cx, int cy, uint uFlags);

    [DllImport("user32.dll")]
    public static extern int GetWindowLong(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll")]
    public static extern short GetAsyncKeyState(int vKey);

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int X, int Y);

    [DllImport("user32.dll")]
    public static extern IntPtr WindowFromPoint(POINT pt);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    [DllImport("user32.dll")]
    public static extern IntPtr SetCursor(IntPtr hCursor);

    [DllImport("user32.dll")]
    public static extern IntPtr LoadCursor(IntPtr hInstance, int lpCursorName);

    public static void ApplyCursor(Cursor? cursor)
    {
        var handle = IntPtr.Zero;
        try { if (cursor != null) handle = cursor.Handle; }
        catch { }
        if (handle == IntPtr.Zero)
            handle = LoadCursor(IntPtr.Zero, IDC_CROSS);
        if (handle != IntPtr.Zero)
            SetCursor(handle);
    }

    [DllImport("user32.dll")]
    public static extern bool DestroyIcon(IntPtr hIcon);

    [DllImport("gdi32.dll")]
    public static extern bool BitBlt(IntPtr hdcDest, int nXDest, int nYDest, int nWidth, int nHeight,
        IntPtr hdcSrc, int nXSrc, int nYSrc, uint dwRop);

    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr hwnd, int dwAttribute, out RECT pvAttribute, int cbAttribute);

    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr hwnd, int dwAttribute, out int pvAttribute, int cbAttribute);

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT
    {
        public int X;
        public int Y;
        public POINT(int x, int y) { X = x; Y = y; }
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct INPUT
    {
        public int type;
        public InputUnion U;
    }

    [StructLayout(LayoutKind.Explicit)]
    public struct InputUnion
    {
        [FieldOffset(0)] public MOUSEINPUT mi;
        [FieldOffset(0)] public KEYBDINPUT ki;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MOUSEINPUT
    {
        public int dx;
        public int dy;
        public uint mouseData;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct KEYBDINPUT
    {
        public ushort wVk;
        public ushort wScan;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;

        public readonly int Width => Right - Left;
        public readonly int Height => Bottom - Top;
        public readonly Rectangle ToRectangle() => Rectangle.FromLTRB(Left, Top, Right, Bottom);
    }

    public static Rectangle GetVirtualScreen()
    {
        int x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        int y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        int w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        int h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        return new Rectangle(x, y, w, h);
    }

    public static bool IsKeyDown(Keys key) => (GetAsyncKeyState((int)key) & 0x8000) != 0;

    public static void ScrollWheel(int delta)
    {
        var input = new INPUT
        {
            type = INPUT_MOUSE,
            U = new InputUnion
            {
                mi = new MOUSEINPUT
                {
                    dwFlags = MOUSEEVENTF_WHEEL,
                    mouseData = unchecked((uint)delta)
                }
            }
        };
        SendInput(1, [input], Marshal.SizeOf<INPUT>());
    }

    public static void ScrollWheelAt(int screenX, int screenY, int delta)
    {
        SetCursorPos(screenX, screenY);
        ScrollWheel(delta);
    }
}

internal static class Theme
{
    public static readonly Color Accent = Color.FromArgb(0x2D, 0xE2, 0xA8);
    public static readonly Color AccentHi = Color.FromArgb(0x5E, 0xF0, 0xC0);
    public static readonly Color AccentDark = Color.FromArgb(0x0B, 0x3D, 0x33);
    public static readonly Color Veil = Color.FromArgb(118, 6, 8, 12);
    public static readonly Color MagBg = Color.FromArgb(0x10, 0x14, 0x1C);
    public static readonly Color MagBorder = Color.FromArgb(0x2D, 0xE2, 0xA8);
    public static readonly Color Text = Color.FromArgb(0xE6, 0xED, 0xF3);
    public static readonly Color Dim = Color.FromArgb(0x8B, 0x94, 0xA0);
    public static readonly Color WinBg = Color.FromArgb(0x12, 0x16, 0x1C);
    public static readonly Color Panel = Color.FromArgb(0x1A, 0x21, 0x2B);
    public static readonly Color Border = Color.FromArgb(0x2A, 0x33, 0x40);
    public static readonly Color InputBg = Color.FromArgb(0x0D, 0x11, 0x17);
    public static readonly Color Danger = Color.FromArgb(0xF0, 0x71, 0x78);
    public static readonly Font Ui = new("Segoe UI", 9f);
    public static readonly Font UiSmall = new("Segoe UI", 8.25f);
    public static readonly Font UiBold = new("Segoe UI", 9f, FontStyle.Bold);
    public static readonly Font Mono = new("Consolas", 8.25f);
    public static readonly Font Hint = new("Segoe UI", 10f);
}

internal sealed class DarkMenuRenderer : ToolStripProfessionalRenderer
{
    public DarkMenuRenderer() : base(new DarkColors()) { }

    protected override void OnRenderItemText(ToolStripItemTextRenderEventArgs e)
    {
        e.TextColor = e.Item.Selected ? Color.FromArgb(0x06, 0x12, 0x10) : Theme.Text;
        base.OnRenderItemText(e);
    }

    protected override void OnRenderSeparator(ToolStripSeparatorRenderEventArgs e)
    {
        var y = e.Item.ContentRectangle.Top + e.Item.ContentRectangle.Height / 2;
        using var p = new Pen(Theme.Border);
        e.Graphics.DrawLine(p, 8, y, e.Item.Width - 8, y);
    }

    private sealed class DarkColors : ProfessionalColorTable
    {
        public override Color MenuBorder => Theme.Border;
        public override Color MenuStripGradientBegin => Theme.WinBg;
        public override Color MenuStripGradientEnd => Theme.WinBg;
        public override Color ImageMarginGradientBegin => Theme.WinBg;
        public override Color ImageMarginGradientEnd => Theme.WinBg;
        public override Color ImageMarginGradientMiddle => Theme.WinBg;
        public override Color ToolStripDropDownBackground => Theme.WinBg;
        public override Color MenuItemBorder => Theme.Accent;
        public override Color MenuItemSelected => Theme.Accent;
        public override Color MenuItemSelectedGradientBegin => Theme.Accent;
        public override Color MenuItemSelectedGradientEnd => Theme.Accent;
        public override Color SeparatorDark => Theme.Border;
        public override Color SeparatorLight => Theme.Border;
    }
}

internal readonly record struct WindowInfo(IntPtr Handle, Rectangle Bounds, string Title);

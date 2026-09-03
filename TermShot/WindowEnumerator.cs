using System.Text;

namespace TermShot;

internal static class WindowEnumerator
{
    private static readonly HashSet<string> SkipClasses = new(StringComparer.Ordinal)
    {
        "Progman",
        "WorkerW",
        "Shell_TrayWnd",
        "Shell_SecondaryTrayWnd",
        "NotifyIconOverflowWindow",
        "Xaml_WindowedPopupClass",
        "ForegroundStaging"
    };

    public static List<WindowInfo> Snapshot(IntPtr exclude)
    {
        var list = new List<WindowInfo>();
        NativeMethods.EnumWindows((hWnd, _) =>
        {
            if (hWnd == exclude) return true;
            if (!NativeMethods.IsWindowVisible(hWnd)) return true;
            if (NativeMethods.IsIconic(hWnd)) return true;
            if (IsCloaked(hWnd)) return true;

            var cls = GetClassName(hWnd);
            if (SkipClasses.Contains(cls)) return true;
            if (!TryGetVisibleBounds(hWnd, out var rect)) return true;
            if (rect.Width < 8 || rect.Height < 8) return true;

            list.Add(new WindowInfo(hWnd, rect, GetTitle(hWnd)));
            return true;
        }, IntPtr.Zero);
        return list;
    }

    public static WindowInfo? HitTest(IReadOnlyList<WindowInfo> windows, Point screenPoint)
    {
        foreach (var w in windows)
        {
            if (w.Bounds.Contains(screenPoint))
                return w;
        }
        return null;
    }

    public static Rectangle MonitorFromPoint(Point screenPoint)
    {
        foreach (var screen in Screen.AllScreens)
        {
            if (screen.Bounds.Contains(screenPoint))
                return screen.Bounds;
        }
        return Screen.PrimaryScreen?.Bounds ?? NativeMethods.GetVirtualScreen();
    }

    private static bool TryGetVisibleBounds(IntPtr hWnd, out Rectangle rect)
    {
        if (NativeMethods.DwmGetWindowAttribute(hWnd, NativeMethods.DWMWA_EXTENDED_FRAME_BOUNDS,
                out NativeMethods.RECT dwm, 16) == 0 && dwm.Width > 0 && dwm.Height > 0)
        {
            rect = dwm.ToRectangle();
            return true;
        }

        if (NativeMethods.GetWindowRect(hWnd, out var r) && r.Width > 0 && r.Height > 0)
        {
            rect = r.ToRectangle();
            return true;
        }

        rect = default;
        return false;
    }

    private static bool IsCloaked(IntPtr hWnd)
    {
        return NativeMethods.DwmGetWindowAttribute(hWnd, NativeMethods.DWMWA_CLOAKED, out int cloaked, 4) == 0
               && cloaked != 0;
    }

    private static string GetClassName(IntPtr hWnd)
    {
        var sb = new StringBuilder(256);
        NativeMethods.GetClassName(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }

    private static string GetTitle(IntPtr hWnd)
    {
        var sb = new StringBuilder(512);
        NativeMethods.GetWindowText(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }
}

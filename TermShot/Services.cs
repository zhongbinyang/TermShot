using System.Drawing.Imaging;
using Microsoft.Win32;

namespace TermShot;

internal static class CaptureService
{
    public static Bitmap CaptureVirtualScreen()
    {
        var vs = NativeMethods.GetVirtualScreen();
        return CaptureRect(vs);
    }

    public static Bitmap CaptureRect(Rectangle screenRect)
    {
        if (screenRect.Width <= 0 || screenRect.Height <= 0)
            throw new ArgumentOutOfRangeException(nameof(screenRect));

        var bmp = new Bitmap(screenRect.Width, screenRect.Height, PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(bmp);
        IntPtr hdcDest = g.GetHdc();
        IntPtr hdcSrc = NativeMethods.GetDC(IntPtr.Zero);
        try
        {
            NativeMethods.BitBlt(hdcDest, 0, 0, screenRect.Width, screenRect.Height,
                hdcSrc, screenRect.X, screenRect.Y, NativeMethods.SRCCOPY);
        }
        finally
        {
            g.ReleaseHdc(hdcDest);
            NativeMethods.ReleaseDC(IntPtr.Zero, hdcSrc);
        }
        return bmp;
    }

    public static Bitmap Crop(Bitmap source, Rectangle bitmapRect)
    {
        var bounds = new Rectangle(0, 0, source.Width, source.Height);
        bitmapRect.Intersect(bounds);
        if (bitmapRect.Width <= 0 || bitmapRect.Height <= 0)
            throw new InvalidOperationException("截取区域无效。");

        var bmp = new Bitmap(bitmapRect.Width, bitmapRect.Height, PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(bmp);
        g.DrawImage(source, new Rectangle(0, 0, bitmapRect.Width, bitmapRect.Height),
            bitmapRect, GraphicsUnit.Pixel);
        return bmp;
    }

    public static bool TrySavePng(Bitmap bitmap, AppSettings settings, out string savedPath, out string? error)
    {
        savedPath = "";
        error = null;
        try
        {
            var dir = settings.ResolvedSaveDirectory;
            Directory.CreateDirectory(dir);
            savedPath = NextPath(dir);
            bitmap.Save(savedPath, ImageFormat.Png);
            return true;
        }
        catch (Exception ex)
        {
            error = ex.Message;
            savedPath = "";
            return false;
        }
    }

    public static bool TrySetClipboardPath(string fullPath, bool quote)
    {
        var text = quote ? $"\"{fullPath}\"" : fullPath;
        return TrySetClipboardData(text);
    }

    public static bool TrySetClipboardImage(Bitmap bitmap)
    {
        using var clone = new Bitmap(bitmap);
        return TrySetClipboardData(clone);
    }

    public static bool TrySetClipboardText(string text)
    {
        if (string.IsNullOrEmpty(text))
            return false;
        return TrySetClipboardData(text);
    }

    private static bool TrySetClipboardData(object data)
    {
        for (int i = 0; i < 8; i++)
        {
            try
            {
                Clipboard.SetDataObject(data, copy: true, retryTimes: 5, retryDelay: 40);
                return true;
            }
            catch
            {
                Thread.Sleep(40);
            }
        }
        return false;
    }

    private static string NextPath(string dir)
    {
        var stamp = DateTime.Now.ToString("yyyyMMdd-HHmmss");
        var path = Path.Combine(dir, stamp + ".png");
        if (!File.Exists(path)) return path;
        for (int i = 1; i < 1000; i++)
        {
            path = Path.Combine(dir, $"{stamp}-{i}.png");
            if (!File.Exists(path)) return path;
        }
        return Path.Combine(dir, $"{stamp}-{Guid.NewGuid():N}.png");
    }
}

internal static class StartupService
{
    private const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string ValueName = "TermShot";

    public static string ExePath => Environment.ProcessPath
        ?? Path.Combine(AppContext.BaseDirectory, "TermShot.exe");

    public static void Apply(bool enabled, string? exePath = null)
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKey, writable: true)
            ?? throw new InvalidOperationException("无法打开开机启动注册表项。");
        if (enabled)
            key.SetValue(ValueName, $"\"{exePath ?? ExePath}\"");
        else
            key.DeleteValue(ValueName, throwOnMissingValue: false);
    }

    public static bool IsEnabled()
    {
        using var key = Registry.CurrentUser.OpenSubKey(RunKey, writable: false);
        var value = key?.GetValue(ValueName) as string;
        return !string.IsNullOrWhiteSpace(value);
    }
}

internal static class InstallService
{
    public static string InstallDir => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "TermShot");

    public static string InstalledExe => Path.Combine(InstallDir, "TermShot.exe");

    public static string StartMenuLink => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.StartMenu),
        "Programs", "TermShot.lnk");

    public static void Install(bool launch = true, bool startWithWindows = true)
    {
        var src = Environment.ProcessPath
            ?? throw new InvalidOperationException("找不到当前程序路径。");

        StopOtherInstances();
        Directory.CreateDirectory(InstallDir);

        if (!SamePath(src, InstalledExe))
        {
            var tmp = InstalledExe + ".new";
            File.Copy(src, tmp, overwrite: true);
            File.Copy(tmp, InstalledExe, overwrite: true);
            try { File.Delete(tmp); } catch { }
        }

        CreateShortcut(StartMenuLink, InstalledExe, "终端截图 - 框选保存 PNG，路径写入剪贴板");
        StartupService.Apply(startWithWindows, InstalledExe);

        var settings = AppSettings.Load();
        settings.StartWithWindows = startWithWindows;
        settings.Save();

        if (launch)
            LaunchInstalled();
    }

    public static void LaunchInstalled()
    {
        var psi = new System.Diagnostics.ProcessStartInfo(InstalledExe)
        {
            WorkingDirectory = InstallDir,
            UseShellExecute = true
        };
        System.Diagnostics.Process.Start(psi);
    }

    public static void StopOtherInstances()
    {
        foreach (var proc in System.Diagnostics.Process.GetProcessesByName("TermShot"))
        {
            try
            {
                if (proc.Id == Environment.ProcessId) continue;
                proc.Kill();
                proc.WaitForExit(4000);
            }
            catch { }
        }
    }

    public static bool SamePath(string a, string b) =>
        string.Equals(Path.GetFullPath(a), Path.GetFullPath(b), StringComparison.OrdinalIgnoreCase);

    public static void Uninstall()
    {
        try { StartupService.Apply(false); } catch { }
        try { if (File.Exists(StartMenuLink)) File.Delete(StartMenuLink); } catch { }

        foreach (var proc in System.Diagnostics.Process.GetProcessesByName("TermShot"))
        {
            try
            {
                if (proc.Id == Environment.ProcessId) continue;
                proc.Kill();
                proc.WaitForExit(3000);
            }
            catch { }
        }

        try
        {
            if (Directory.Exists(InstallDir))
                Directory.Delete(InstallDir, recursive: true);
        }
        catch
        {
            // running from install dir — delete on next reboot is not needed; leftover is ok
        }
    }

    private static void CreateShortcut(string linkPath, string target, string description)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(linkPath)!);
        var type = Type.GetTypeFromProgID("WScript.Shell")
            ?? throw new InvalidOperationException("无法创建快捷方式。");
        dynamic shell = Activator.CreateInstance(type)!;
        dynamic shortcut = shell.CreateShortcut(linkPath);
        shortcut.TargetPath = target;
        shortcut.WorkingDirectory = Path.GetDirectoryName(target);
        shortcut.Description = description;
        shortcut.IconLocation = target;
        shortcut.Save();
    }
}

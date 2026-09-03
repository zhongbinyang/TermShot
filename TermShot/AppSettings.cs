using System.Text.Json;
using System.Text.Json.Serialization;

namespace TermShot;

internal sealed class AppSettings
{
    public string SaveDirectory { get; set; } = "";
    public bool RegionSelect { get; set; } = true;
    public uint HotkeyModifiers { get; set; } = NativeMethods.MOD_CONTROL | NativeMethods.MOD_SHIFT;
    public int HotkeyKey { get; set; } = (int)Keys.S;
    public PostCaptureAction PostCaptureAction { get; set; } = PostCaptureAction.Ask;
    public bool QuotePath { get; set; } = true;
    public bool StartWithWindows { get; set; } = true;

    [JsonIgnore]
    public string ResolvedSaveDirectory =>
        string.IsNullOrWhiteSpace(SaveDirectory)
            ? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures), "Screenshots")
            : SaveDirectory;

    [JsonIgnore]
    public Keys HotkeyKeys => (Keys)HotkeyKey;

    public static string Dir => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "TermShot");

    public static string FilePath => Path.Combine(Dir, "settings.json");

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        ReadCommentHandling = JsonCommentHandling.Skip,
        AllowTrailingCommas = true
    };

    public static AppSettings Load()
    {
        try
        {
            if (File.Exists(FilePath))
            {
                var json = File.ReadAllText(FilePath);
                var loaded = JsonSerializer.Deserialize<AppSettings>(json, JsonOptions);
                if (loaded != null) return loaded;
            }
        }
        catch
        {
            // keep defaults
        }

        return new AppSettings();
    }

    public void Save()
    {
        Directory.CreateDirectory(Dir);
        var tmp = FilePath + ".tmp";
        File.WriteAllText(tmp, JsonSerializer.Serialize(this, JsonOptions));
        File.Copy(tmp, FilePath, overwrite: true);
        File.Delete(tmp);
    }

    public static string FormatHotkey(uint modifiers, Keys key)
    {
        var parts = new List<string>();
        if ((modifiers & NativeMethods.MOD_CONTROL) != 0) parts.Add("Ctrl");
        if ((modifiers & NativeMethods.MOD_SHIFT) != 0) parts.Add("Shift");
        if ((modifiers & NativeMethods.MOD_ALT) != 0) parts.Add("Alt");
        if ((modifiers & NativeMethods.MOD_WIN) != 0) parts.Add("Win");
        parts.Add(FormatKey(key));
        return string.Join("+", parts);
    }

    public string FormatHotkey() => FormatHotkey(HotkeyModifiers, HotkeyKeys);

    private static string FormatKey(Keys key) => key switch
    {
        Keys.D0 => "0",
        Keys.D1 => "1",
        Keys.D2 => "2",
        Keys.D3 => "3",
        Keys.D4 => "4",
        Keys.D5 => "5",
        Keys.D6 => "6",
        Keys.D7 => "7",
        Keys.D8 => "8",
        Keys.D9 => "9",
        Keys.Oemtilde => "`",
        Keys.OemMinus => "-",
        Keys.Oemplus => "=",
        Keys.OemOpenBrackets => "[",
        Keys.OemCloseBrackets => "]",
        Keys.OemPipe => "\\",
        Keys.OemSemicolon => ";",
        Keys.OemQuotes => "'",
        Keys.Oemcomma => ",",
        Keys.OemPeriod => ".",
        Keys.OemQuestion => "/",
        Keys.Space => "Space",
        Keys.PageUp => "PageUp",
        Keys.PageDown => "PageDown",
        Keys.PrintScreen => "PrintScreen",
        _ => key.ToString()
    };
}

internal enum PostCaptureAction
{
    Ask = 0,
    SaveImage = 1,
    CopyImage = 2,
    CopyPath = 3,
    Pin = 4
}

internal static class PostCaptureActions
{
    public static readonly (PostCaptureAction Value, string Text)[] All =
    [
        (PostCaptureAction.Ask, "每次询问（截完后选择）"),
        (PostCaptureAction.SaveImage, "保存图片"),
        (PostCaptureAction.CopyImage, "复制图片"),
        (PostCaptureAction.CopyPath, "复制保存图片的地址"),
        (PostCaptureAction.Pin, "贴到桌面")
    ];

    public static bool UsesPath(PostCaptureAction action) =>
        action is PostCaptureAction.Ask or PostCaptureAction.CopyPath;
}

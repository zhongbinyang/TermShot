namespace TermShot;

internal sealed class SettingsForm : Form
{
    private const int LeftPad = 32;
    private const int ContentW = 476;
    private const int BtnW = 84;
    private const int Gap = 10;

    private readonly AppSettings _live;
    private readonly Action _suspendHotkey;
    private readonly Action<AppSettings> _apply;

    private readonly TextBox _dir;
    private readonly CheckBox _region;
    private readonly ComboBox _action;
    private readonly CheckBox _quote;
    private readonly CheckBox _startup;
    private readonly Label _hotkey;
    private readonly Label _hint;
    private bool _recording;
    private uint _mods;
    private Keys _key;

    public SettingsForm(AppSettings live, Action suspendHotkey, Action<AppSettings> apply)
    {
        _live = live;
        _suspendHotkey = suspendHotkey;
        _apply = apply;
        _mods = live.HotkeyModifiers;
        _key = live.HotkeyKeys;

        AutoScaleMode = AutoScaleMode.Dpi;
        Text = "终端截图 — 设置";
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false;
        MinimizeBox = false;
        ShowInTaskbar = false;
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(540, 640);
        BackColor = Theme.WinBg;
        ForeColor = Theme.Text;
        Font = Theme.Ui;

        int y = 28;
        Controls.Add(MakeLabel("保存目录", LeftPad, y));
        y += 28;
        _dir = new TextBox
        {
            Left = LeftPad,
            Top = y,
            Width = ContentW - Gap - BtnW,
            Height = 32,
            Text = string.IsNullOrWhiteSpace(live.SaveDirectory)
                ? live.ResolvedSaveDirectory
                : live.SaveDirectory,
            BackColor = Theme.InputBg,
            ForeColor = Theme.Text,
            BorderStyle = BorderStyle.FixedSingle
        };
        var browse = MakeButton("浏览", LeftPad + _dir.Width + Gap, y - 1, BtnW, false);
        browse.Click += (_, _) => Browse();
        Controls.Add(_dir);
        Controls.Add(browse);

        y += 56;
        Controls.Add(MakeLabel("截图后", LeftPad, y));
        y += 28;
        _action = new ComboBox
        {
            Left = LeftPad,
            Top = y,
            Width = ContentW,
            Height = 32,
            DropDownStyle = ComboBoxStyle.DropDownList,
            BackColor = Theme.InputBg,
            ForeColor = Theme.Text,
            FlatStyle = FlatStyle.Flat,
            Font = Theme.Ui
        };
        foreach (var item in PostCaptureActions.All)
            _action.Items.Add(item.Text);
        var actionIndex = Array.FindIndex(PostCaptureActions.All, x => x.Value == live.PostCaptureAction);
        _action.SelectedIndex = actionIndex >= 0 ? actionIndex : 0;
        _action.SelectedIndexChanged += (_, _) => SyncQuoteEnabled();
        Controls.Add(_action);
        y += 40;
        MakeDim("每次询问会在选区旁弹出工具条，可画箭头、贴到桌面，或保存 / 复制。", LeftPad, y, ContentW);

        y += 52;
        Controls.Add(MakeLabel("快捷键", LeftPad, y));
        y += 28;
        _hotkey = MakeHotkeyBox(LeftPad, y, AppSettings.FormatHotkey(_mods, _key));
        var rec = MakeButton("录制", LeftPad + _hotkey.Width + Gap, y, BtnW, true);
        rec.Click += (_, _) => BeginRecord();
        Controls.Add(_hotkey);
        Controls.Add(rec);
        y += 42;
        _hint = MakeDim("点击录制后按下组合键，需包含 Ctrl / Shift / Alt / Win。", LeftPad, y, ContentW);

        y += 52;
        Controls.Add(MakeLabel("其他", LeftPad, y));
        y += 30;
        _region = MakeCheck("快捷键使用框选（关闭后改为截取鼠标所在屏幕）", LeftPad, y, live.RegionSelect);
        y += 28;
        MakeDim("托盘左键和菜单里的「截图」始终走框选，不受此项影响。", LeftPad + 20, y, ContentW - 20);

        y += 40;
        _quote = MakeCheck("剪贴板路径加引号（粘到命令行可直接用）", LeftPad, y, live.QuotePath);
        y += 36;
        _startup = MakeCheck("开机启动", LeftPad, y, live.StartWithWindows);

        var ok = MakeButton("保存", 340, 584, 88, true);
        var cancel = MakeButton("取消", 436, 584, 88, false);
        ok.Click += (_, _) => SaveAndClose();
        cancel.Click += (_, _) => Close();
        CancelButton = cancel;
        AcceptButton = ok;
        Controls.Add(ok);
        Controls.Add(cancel);

        KeyPreview = true;
        KeyDown += OnKeyDown;
        Shown += (_, _) => _suspendHotkey();
        SyncQuoteEnabled();
    }

    private PostCaptureAction SelectedAction()
    {
        var i = _action.SelectedIndex;
        if (i < 0 || i >= PostCaptureActions.All.Length)
            return PostCaptureAction.Ask;
        return PostCaptureActions.All[i].Value;
    }

    private void SyncQuoteEnabled()
    {
        var on = PostCaptureActions.UsesPath(SelectedAction());
        _quote.Enabled = on;
        _quote.ForeColor = on ? Theme.Text : Theme.Dim;
    }

    private void BeginRecord()
    {
        _recording = true;
        _hotkey.Text = "  按下组合键…";
        _hotkey.ForeColor = Color.White;
        _hint.Text = "正在录制，按 Esc 取消。";
        _hint.ForeColor = Theme.Accent;
    }

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (!_recording) return;
        e.SuppressKeyPress = true;
        e.Handled = true;
        if (e.KeyCode == Keys.Escape)
        {
            _recording = false;
            _hotkey.Text = "  " + AppSettings.FormatHotkey(_mods, _key);
            _hotkey.ForeColor = Theme.AccentHi;
            _hint.Text = "已取消录制。";
            _hint.ForeColor = Theme.Dim;
            return;
        }
        if (e.KeyCode is Keys.ControlKey or Keys.ShiftKey or Keys.Menu or Keys.LWin or Keys.RWin)
            return;

        uint mods = 0;
        if (e.Control) mods |= NativeMethods.MOD_CONTROL;
        if (e.Shift) mods |= NativeMethods.MOD_SHIFT;
        if (e.Alt) mods |= NativeMethods.MOD_ALT;
        if (NativeMethods.IsKeyDown(Keys.LWin) || NativeMethods.IsKeyDown(Keys.RWin))
            mods |= NativeMethods.MOD_WIN;

        if (mods == 0)
        {
            _hint.Text = "请带上 Ctrl / Shift / Alt / Win，避免占用普通按键。";
            _hint.ForeColor = Theme.Danger;
            return;
        }

        _mods = mods;
        _key = e.KeyCode;
        _recording = false;
        _hotkey.Text = "  " + AppSettings.FormatHotkey(_mods, _key);
        _hotkey.ForeColor = Theme.AccentHi;
        _hint.Text = "已录制 " + AppSettings.FormatHotkey(_mods, _key);
        _hint.ForeColor = Theme.Accent;
    }

    private void Browse()
    {
        using var dlg = new FolderBrowserDialog
        {
            Description = "选择截图保存目录",
            UseDescriptionForTitle = true,
            SelectedPath = Directory.Exists(_dir.Text) ? _dir.Text : _live.ResolvedSaveDirectory,
            ShowNewFolderButton = true
        };
        if (dlg.ShowDialog(this) == DialogResult.OK)
            _dir.Text = dlg.SelectedPath;
    }

    private void SaveAndClose()
    {
        var dir = _dir.Text.Trim();
        var def = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures), "Screenshots");
        _live.SaveDirectory = string.Equals(dir, def, StringComparison.OrdinalIgnoreCase) ? "" : dir;
        _live.RegionSelect = _region.Checked;
        _live.PostCaptureAction = SelectedAction();
        _live.QuotePath = _quote.Checked;
        _live.StartWithWindows = _startup.Checked;
        _live.HotkeyModifiers = _mods;
        _live.HotkeyKey = (int)_key;
        _live.Save();
        _apply(_live);
        DialogResult = DialogResult.OK;
        Close();
    }

    private static Label MakeHotkeyBox(int x, int y, string text) => new()
    {
        Left = x,
        Top = y,
        Width = ContentW - Gap - BtnW,
        Height = 32,
        TextAlign = ContentAlignment.MiddleLeft,
        BackColor = Theme.InputBg,
        ForeColor = Theme.AccentHi,
        BorderStyle = BorderStyle.FixedSingle,
        Font = Theme.UiBold,
        Text = "  " + text
    };

    private static Label MakeLabel(string text, int x, int y) => new()
    {
        Text = text,
        Left = x,
        Top = y,
        AutoSize = true,
        ForeColor = Theme.Text,
        Font = Theme.UiBold
    };

    private Label MakeDim(string text, int x, int y, int width)
    {
        var l = new Label
        {
            Text = text,
            Left = x,
            Top = y,
            Width = width,
            Height = 36,
            AutoSize = false,
            ForeColor = Theme.Dim,
            Font = Theme.UiSmall
        };
        Controls.Add(l);
        return l;
    }

    private CheckBox MakeCheck(string text, int x, int y, bool on)
    {
        var c = new CheckBox
        {
            Text = text,
            Left = x,
            Top = y,
            Width = ContentW,
            Height = 24,
            AutoSize = false,
            Checked = on,
            ForeColor = Theme.Text,
            FlatStyle = FlatStyle.Flat
        };
        c.FlatAppearance.BorderColor = Theme.Border;
        Controls.Add(c);
        return c;
    }

    private static Button MakeButton(string text, int x, int y, int w, bool accent)
    {
        var b = new Button
        {
            Text = text,
            Left = x,
            Top = y,
            Width = w,
            Height = 32,
            FlatStyle = FlatStyle.Flat,
            ForeColor = accent ? Color.FromArgb(0x06, 0x12, 0x10) : Theme.Text,
            BackColor = accent ? Theme.Accent : Theme.Panel,
            Font = Theme.UiBold,
            Cursor = Cursors.Hand
        };
        b.FlatAppearance.BorderColor = accent ? Theme.Accent : Theme.Border;
        b.FlatAppearance.MouseOverBackColor = accent
            ? Theme.AccentHi
            : Color.FromArgb(0x24, 0x2C, 0x38);
        return b;
    }
}

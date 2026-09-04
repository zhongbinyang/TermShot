using System.Drawing.Drawing2D;

namespace TermShot;

internal sealed class SettingsForm : Form
{
    private readonly float _scale;
    private int S(float px) => (int)Math.Round(px * _scale);

    private readonly AppSettings _live;
    private readonly Action _suspendHotkey;
    private readonly Action<AppSettings> _apply;

    // Controls
    private readonly ModernInput _dirInput;
    private readonly ModernButton _browseBtn;
    private readonly ModernCombo _actionCombo;
    private readonly HotkeyPill _hotkeyBox;
    private readonly ModernButton _recordBtn;
    private readonly ModernCheck _quoteCheck;
    private readonly ModernCheck _startupCheck;
    private readonly ModernCheck _ollamaCheck;
    private readonly ModernInput _hostInput;
    private readonly ModernInput _modelInput;
    private readonly LangSwitch _langSwitch;
    private readonly Label _statusHint;

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

        // 动态计算高分屏 DPI 缩放倍率（96 DPI 为 100% 基准）
        _scale = Math.Max(1.0f, DeviceDpi / 96f);

        // 采用精准的自主 DPI 缩放，避免系统 AutoScale 二次错位
        AutoScaleMode = AutoScaleMode.None;
        Text = "终端截图 — 设置";
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false;
        MinimizeBox = false;
        ShowInTaskbar = false;
        StartPosition = FormStartPosition.CenterScreen;
        BackColor = Theme.WinBg;
        ForeColor = Theme.Text;
        Font = Theme.Ui;
        KeyPreview = true;
        KeyDown += OnKeyDown;
        Shown += (_, _) =>
        {
            _suspendHotkey();
            try { NativeMethods.EnableDarkMode(Handle); } catch { }
        };

        // 基础尺寸定义（经 DPI 动态缩放）
        int formW = S(490);
        int padX = S(18);
        int cardW = formW - padX * 2;
        int fieldH = S(30);
        int btnW = S(60);
        int rowH = S(40);
        int cardPad = S(14);
        int labelW = S(66);
        int ctrlLeft = cardPad + labelW + S(8);
        int fullCtrlW = cardW - cardPad - ctrlLeft;
        int withBtnW = fullCtrlW - btnW - S(8);

        int y = S(16);

        // -------------------------------------------------------------
        // 分区 1：截图设置
        // -------------------------------------------------------------
        Controls.Add(CreateSectionTitle("截图", padX, y));
        y += S(24);

        var card1 = new CardPanel(cardW, _scale) { Left = padX, Top = y };
        int cy = cardPad;

        // 1. 保存目录
        _dirInput = new ModernInput(
            live.SaveDirectory.Length == 0 ? live.ResolvedSaveDirectory : live.SaveDirectory,
            withBtnW, fieldH, _scale)
        {
            Left = ctrlLeft,
            Top = cy
        };
        _browseBtn = new ModernButton("浏览", btnW, fieldH, false, _scale)
        {
            Left = _dirInput.Right + S(8),
            Top = cy
        };
        _browseBtn.Click += (_, _) => Browse();
        card1.Controls.Add(CreateRowLabel("保存目录", cardPad, cy, labelW, fieldH));
        card1.Controls.Add(_dirInput);
        card1.Controls.Add(_browseBtn);
        cy += rowH;

        // 2. 截图后
        _actionCombo = new ModernCombo(PostCaptureActions.All.Select(x => x.Text), fullCtrlW, fieldH, _scale)
        {
            Left = ctrlLeft,
            Top = cy
        };
        var actionIndex = Array.FindIndex(PostCaptureActions.All, x => x.Value == live.PostCaptureAction);
        _actionCombo.SelectedIndex = actionIndex >= 0 ? actionIndex : 0;
        _actionCombo.SelectedIndexChanged += (_, _) => SyncQuoteEnabled();
        card1.Controls.Add(CreateRowLabel("截图后", cardPad, cy, labelW, fieldH));
        card1.Controls.Add(_actionCombo);
        cy += rowH;

        // 3. 快捷键
        _hotkeyBox = new HotkeyPill(AppSettings.FormatHotkey(_mods, _key), withBtnW, fieldH, _scale)
        {
            Left = ctrlLeft,
            Top = cy
        };
        _recordBtn = new ModernButton("录制", btnW, fieldH, true, _scale)
        {
            Left = _hotkeyBox.Right + S(8),
            Top = cy
        };
        _recordBtn.Click += (_, _) => BeginRecord();
        card1.Controls.Add(CreateRowLabel("快捷键", cardPad, cy, labelW, fieldH));
        card1.Controls.Add(_hotkeyBox);
        card1.Controls.Add(_recordBtn);
        cy += rowH;

        // 分割线
        card1.AddDivider(cy + S(2));
        cy += S(10);

        // 4. 选项行（路径加引号 + 开机自启）
        _quoteCheck = new ModernCheck("路径加双引号", live.QuotePath, _scale)
        {
            Left = ctrlLeft,
            Top = cy,
            Size = new Size(S(130), fieldH)
        };
        _startupCheck = new ModernCheck("开机自启动", live.StartWithWindows, _scale)
        {
            Left = _quoteCheck.Right + S(12),
            Top = cy,
            Size = new Size(S(110), fieldH)
        };
        card1.Controls.Add(_quoteCheck);
        card1.Controls.Add(_startupCheck);
        cy += fieldH + cardPad;

        card1.Height = cy;
        Controls.Add(card1);
        y += card1.Height + S(16);

        // -------------------------------------------------------------
        // 分区 2：AI 识别与翻译设置
        // -------------------------------------------------------------
        Controls.Add(CreateSectionTitle("AI 识图与翻译", padX, y));
        y += S(24);

        var card2 = new CardPanel(cardW, _scale) { Left = padX, Top = y };
        cy = cardPad;

        // 1. 启用开关
        _ollamaCheck = new ModernCheck("启用 Ollama 本地模型（用于截图识字与一键翻译）", live.OllamaOcr, _scale)
        {
            Left = cardPad,
            Top = cy,
            Size = new Size(cardW - cardPad * 2, fieldH)
        };
        _ollamaCheck.CheckedChanged += (_, _) => SyncOllamaEnabled();
        card2.Controls.Add(_ollamaCheck);
        cy += rowH;

        card2.AddDivider(cy);
        cy += S(10);

        // 2. 服务地址
        _hostInput = new ModernInput(
            string.IsNullOrWhiteSpace(live.OllamaHost) ? AppSettings.DefaultOllamaHost : live.OllamaHost,
            fullCtrlW, fieldH, _scale)
        {
            Left = ctrlLeft,
            Top = cy
        };
        card2.Controls.Add(CreateRowLabel("服务地址", cardPad, cy, labelW, fieldH));
        card2.Controls.Add(_hostInput);
        cy += rowH;

        // 3. 模型名称
        _modelInput = new ModernInput(
            string.IsNullOrWhiteSpace(live.OllamaModel) ? AppSettings.DefaultOllamaModel : live.OllamaModel,
            fullCtrlW, fieldH, _scale)
        {
            Left = ctrlLeft,
            Top = cy
        };
        card2.Controls.Add(CreateRowLabel("模型名称", cardPad, cy, labelW, fieldH));
        card2.Controls.Add(_modelInput);
        cy += rowH;

        // 4. 翻译目标
        _langSwitch = new LangSwitch(live.TranslateTarget == TranslateTarget.English, S(136), fieldH, _scale)
        {
            Left = ctrlLeft,
            Top = cy
        };
        card2.Controls.Add(CreateRowLabel("翻译目标", cardPad, cy, labelW, fieldH));
        card2.Controls.Add(_langSwitch);
        cy += fieldH + cardPad;

        card2.Height = cy;
        Controls.Add(card2);
        y += card2.Height + S(16);

        // -------------------------------------------------------------
        // 底部状态与操作栏
        // -------------------------------------------------------------
        var footer = new Panel
        {
            Left = padX,
            Top = y,
            Width = cardW,
            Height = S(36)
        };

        _statusHint = new Label
        {
            Text = "",
            Left = 0,
            Top = 0,
            Width = cardW - S(170),
            Height = S(36),
            TextAlign = ContentAlignment.MiddleLeft,
            ForeColor = Theme.AccentHi,
            Font = Theme.UiSmall
        };
        footer.Controls.Add(_statusHint);

        int okW = S(76);
        int cancelW = S(70);
        var okBtn = new ModernButton("保存", okW, S(32), true, _scale)
        {
            Left = cardW - okW,
            Top = S(2)
        };
        var cancelBtn = new ModernButton("取消", cancelW, S(32), false, _scale)
        {
            Left = cardW - okW - S(8) - cancelW,
            Top = S(2)
        };

        okBtn.Click += (_, _) => SaveAndClose();
        cancelBtn.Click += (_, _) => Close();
        footer.Controls.Add(okBtn);
        footer.Controls.Add(cancelBtn);
        Controls.Add(footer);

        AcceptButton = okBtn;
        CancelButton = cancelBtn;

        y += footer.Height + S(16);
        ClientSize = new Size(formW, y);

        SyncQuoteEnabled();
        SyncOllamaEnabled();
    }

    private static Label CreateSectionTitle(string text, int x, int y)
    {
        return new Label
        {
            Text = text,
            Left = x,
            Top = y,
            AutoSize = true,
            Font = Theme.UiBold,
            ForeColor = Theme.AccentHi
        };
    }

    private static Label CreateRowLabel(string text, int x, int y, int width, int height)
    {
        return new Label
        {
            Text = text,
            Left = x,
            Top = y,
            Width = width,
            Height = height,
            TextAlign = ContentAlignment.MiddleLeft,
            Font = Theme.Ui,
            ForeColor = Theme.Dim
        };
    }

    private void SyncQuoteEnabled()
    {
        var action = SelectedAction();
        bool usesPath = PostCaptureActions.UsesPath(action);
        _quoteCheck.Enabled = usesPath;
        _quoteCheck.ForeColor = usesPath ? Theme.Text : Theme.Dim;
    }

    private void SyncOllamaEnabled()
    {
        bool on = _ollamaCheck.Checked;
        _hostInput.Enabled = on;
        _modelInput.Enabled = on;
        _langSwitch.Enabled = on;
    }

    private PostCaptureAction SelectedAction()
    {
        var i = _actionCombo.SelectedIndex;
        if (i < 0 || i >= PostCaptureActions.All.Length)
            return PostCaptureAction.Ask;
        return PostCaptureActions.All[i].Value;
    }

    private void BeginRecord()
    {
        _recording = true;
        _hotkeyBox.Text = "按下新快捷键…";
        _hotkeyBox.IsRecording = true;
        _statusHint.Text = "请按下组合键（需含 Ctrl/Shift/Alt/Win），按 Esc 取消";
        _statusHint.ForeColor = Theme.Accent;
    }

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (!_recording) return;
        e.SuppressKeyPress = true;
        e.Handled = true;

        if (e.KeyCode == Keys.Escape)
        {
            _recording = false;
            _hotkeyBox.Text = AppSettings.FormatHotkey(_mods, _key);
            _hotkeyBox.IsRecording = false;
            _statusHint.Text = "";
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
            _statusHint.Text = "请带上 Ctrl / Shift / Alt / Win，避免占用单键";
            _statusHint.ForeColor = Theme.Danger;
            return;
        }

        _mods = mods;
        _key = e.KeyCode;
        _recording = false;
        _hotkeyBox.Text = AppSettings.FormatHotkey(_mods, _key);
        _hotkeyBox.IsRecording = false;
        _statusHint.Text = "快捷键已更新为 " + AppSettings.FormatHotkey(_mods, _key);
        _statusHint.ForeColor = Theme.AccentHi;
    }

    private void Browse()
    {
        using var dlg = new FolderBrowserDialog
        {
            Description = "选择截图保存目录",
            UseDescriptionForTitle = true,
            SelectedPath = Directory.Exists(_dirInput.Value) ? _dirInput.Value : _live.ResolvedSaveDirectory,
            ShowNewFolderButton = true
        };
        if (dlg.ShowDialog(this) == DialogResult.OK)
            _dirInput.Value = dlg.SelectedPath;
    }

    private void SaveAndClose()
    {
        var dir = _dirInput.Value.Trim();
        var def = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures), "Screenshots");
        _live.SaveDirectory = string.Equals(dir, def, StringComparison.OrdinalIgnoreCase) ? "" : dir;
        _live.PostCaptureAction = SelectedAction();
        _live.QuotePath = _quoteCheck.Checked;
        _live.StartWithWindows = _startupCheck.Checked;
        _live.OllamaOcr = _ollamaCheck.Checked;
        var host = _hostInput.Value.Trim();
        var model = _modelInput.Value.Trim();
        _live.OllamaHost = string.IsNullOrWhiteSpace(host) ? AppSettings.DefaultOllamaHost : host;
        _live.OllamaModel = string.IsNullOrWhiteSpace(model) ? AppSettings.DefaultOllamaModel : model;
        _live.TranslateTarget = _langSwitch.IsEnglish ? TranslateTarget.English : TranslateTarget.ZhHans;
        _live.HotkeyModifiers = _mods;
        _live.HotkeyKey = (int)_key;
        _live.Save();
        _apply(_live);
        DialogResult = DialogResult.OK;
        Close();
    }

    // ==========================================
    // DPI 感知高质感控件
    // ==========================================

    private sealed class CardPanel : Panel
    {
        private readonly List<int> _dividers = [];
        private readonly float _scale;

        public CardPanel(int width, float scale)
        {
            _scale = scale;
            Width = width;
            BackColor = Color.FromArgb(0x16, 0x1B, 0x24);
            DoubleBuffered = true;
            SetStyle(ControlStyles.UserPaint | ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer, true);
        }

        public void AddDivider(int y) => _dividers.Add(y);

        protected override void OnPaint(PaintEventArgs e)
        {
            base.OnPaint(e);
            var g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;

            var r = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
            using var path = CreateRound(r, 6f * _scale);
            using var fill = new SolidBrush(BackColor);
            g.FillPath(fill, path);

            using var border = new Pen(Color.FromArgb(0x28, 0x32, 0x42), 1f);
            g.DrawPath(border, path);

            // 分割细线
            int margin = (int)(14 * _scale);
            using var sep = new Pen(Color.FromArgb(0x1E, 0x26, 0x33), 1f);
            foreach (var dy in _dividers)
                g.DrawLine(sep, margin, dy, Width - margin, dy);
        }
    }

    private sealed class ModernInput : Panel
    {
        private readonly TextBox _inner;
        private readonly ToolTip _tip = new();
        private readonly float _scale;
        private bool _focused;

        public string Value
        {
            get => _inner.Text;
            set
            {
                _inner.Text = value ?? "";
                _tip.SetToolTip(_inner, _inner.Text);
            }
        }

        public ModernInput(string text, int width, int height, float scale)
        {
            _scale = scale;
            Width = width;
            Height = height;
            DoubleBuffered = true;
            BackColor = Theme.InputBg;

            int pad = (int)(8 * scale);
            _inner = new TextBox
            {
                Text = text,
                BorderStyle = BorderStyle.None,
                BackColor = Theme.InputBg,
                ForeColor = Theme.Text,
                Font = Theme.Ui,
                Left = pad,
                Width = width - pad * 2
            };

            // 垂直居中
            _inner.Top = Math.Max(0, (height - _inner.PreferredHeight) / 2);

            _tip.SetToolTip(_inner, text);
            _inner.GotFocus += (_, _) => { _focused = true; Invalidate(); };
            _inner.LostFocus += (_, _) => { _focused = false; Invalidate(); };
            _inner.TextChanged += (_, _) => _tip.SetToolTip(_inner, _inner.Text);

            Controls.Add(_inner);
        }

        protected override void OnEnabledChanged(EventArgs e)
        {
            base.OnEnabledChanged(e);
            _inner.Enabled = Enabled;
            _inner.ForeColor = Enabled ? Theme.Text : Theme.Dim;
            Invalidate();
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            base.OnPaint(e);
            var g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;

            var r = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
            using var path = CreateRound(r, 5f * _scale);
            using var fill = new SolidBrush(Theme.InputBg);
            g.FillPath(fill, path);

            Color borderCol = !Enabled
                ? Color.FromArgb(0x1F, 0x25, 0x30)
                : (_focused ? Theme.Accent : Color.FromArgb(0x2B, 0x36, 0x46));

            using var pen = new Pen(borderCol, _focused && Enabled ? 1.4f : 1f);
            g.DrawPath(pen, path);
        }
    }

    private sealed class HotkeyPill : Label
    {
        public bool IsRecording { get; set; }
        private readonly float _scale;

        public HotkeyPill(string text, int width, int height, float scale)
        {
            _scale = scale;
            Text = text;
            Width = width;
            Height = height;
            TextAlign = ContentAlignment.MiddleCenter;
            Font = Theme.UiBold;
            ForeColor = Theme.AccentHi;
            DoubleBuffered = true;
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;

            var r = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
            using var path = CreateRound(r, 5f * _scale);
            using var bg = new SolidBrush(Color.FromArgb(0x10, 0x14, 0x1C));
            g.FillPath(bg, path);

            using var border = new Pen(IsRecording ? Theme.Accent : Color.FromArgb(0x2B, 0x36, 0x46), IsRecording ? 1.5f : 1f);
            g.DrawPath(border, path);

            var color = IsRecording ? Color.White : Theme.AccentHi;
            TextRenderer.DrawText(g, Text, Font, ClientRectangle, color,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix);
        }
    }

    private sealed class ModernButton : Button
    {
        private readonly bool _accent;
        private readonly float _scale;
        private bool _hover;
        private bool _down;

        public ModernButton(string text, int width, int height, bool accent, float scale)
        {
            _accent = accent;
            _scale = scale;
            Text = text;
            Width = width;
            Height = height;
            FlatStyle = FlatStyle.Flat;
            FlatAppearance.BorderSize = 0;
            Cursor = Cursors.Hand;
            Font = Theme.UiBold;
            DoubleBuffered = true;
        }

        protected override void OnMouseEnter(EventArgs e) { base.OnMouseEnter(e); _hover = true; Invalidate(); }
        protected override void OnMouseLeave(EventArgs e) { base.OnMouseLeave(e); _hover = false; _down = false; Invalidate(); }
        protected override void OnMouseDown(MouseEventArgs mevent) { base.OnMouseDown(mevent); _down = true; Invalidate(); }
        protected override void OnMouseUp(MouseEventArgs mevent) { base.OnMouseUp(mevent); _down = false; Invalidate(); }

        protected override void OnPaint(PaintEventArgs pevent)
        {
            var g = pevent.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;

            var r = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
            using var path = CreateRound(r, 5f * _scale);

            Color bg;
            Color textCol;
            Color borderCol;

            if (_accent)
            {
                bg = _down ? Color.FromArgb(0x22, 0xB8, 0x88) : _hover ? Theme.AccentHi : Theme.Accent;
                textCol = Color.FromArgb(0x06, 0x12, 0x10);
                borderCol = bg;
            }
            else
            {
                bg = _down ? Color.FromArgb(0x16, 0x1C, 0x26) : _hover ? Color.FromArgb(0x28, 0x33, 0x43) : Color.FromArgb(0x1F, 0x27, 0x33);
                textCol = _hover ? Color.White : Theme.Text;
                borderCol = Color.FromArgb(0x30, 0x3C, 0x4D);
            }

            using var brush = new SolidBrush(bg);
            g.FillPath(brush, path);
            using var pen = new Pen(borderCol, 1f);
            g.DrawPath(pen, path);

            TextRenderer.DrawText(g, Text, Font, ClientRectangle, textCol,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix);
        }
    }

    private sealed class LangSwitch : Panel
    {
        public bool IsEnglish { get; private set; }
        private readonly float _scale;

        public LangSwitch(bool isEnglish, int width, int height, float scale)
        {
            IsEnglish = isEnglish;
            Width = width;
            Height = height;
            _scale = scale;
            DoubleBuffered = true;
            Cursor = Cursors.Hand;
        }

        protected override void OnMouseClick(MouseEventArgs e)
        {
            if (!Enabled) return;
            base.OnMouseClick(e);
            IsEnglish = e.X >= Width / 2;
            Invalidate();
        }

        protected override void OnEnabledChanged(EventArgs e)
        {
            base.OnEnabledChanged(e);
            Invalidate();
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            base.OnPaint(e);
            var g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;

            var r = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
            using var outer = CreateRound(r, 5f * _scale);
            using var fill = new SolidBrush(Color.FromArgb(0x0F, 0x13, 0x1A));
            g.FillPath(fill, outer);

            using var border = new Pen(Color.FromArgb(0x2B, 0x36, 0x46), 1f);
            g.DrawPath(border, outer);

            int half = Width / 2;
            var pillRect = IsEnglish
                ? new RectangleF(half + 2, 2, half - 4, Height - 4)
                : new RectangleF(2, 2, half - 4, Height - 4);

            using var pillPath = CreateRound(pillRect, 4f * _scale);
            Color pillCol = Enabled ? Theme.Accent : Color.FromArgb(0x25, 0x30, 0x3E);
            using var pillBrush = new SolidBrush(pillCol);
            g.FillPath(pillBrush, pillPath);

            Color activeText = Enabled ? Color.FromArgb(0x06, 0x12, 0x10) : Theme.Dim;
            var zhColor = !IsEnglish ? activeText : Theme.Dim;
            var enColor = IsEnglish ? activeText : Theme.Dim;

            var zhRect = new Rectangle(0, 0, half, Height);
            var enRect = new Rectangle(half, 0, half, Height);

            TextRenderer.DrawText(g, "中文", Theme.UiBold, zhRect, zhColor,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
            TextRenderer.DrawText(g, "英文", Theme.UiBold, enRect, enColor,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
        }
    }

    private sealed class ModernCombo : ComboBox
    {
        public ModernCombo(IEnumerable<string> items, int width, int height, float scale)
        {
            Width = width;
            Height = height;
            DropDownStyle = ComboBoxStyle.DropDownList;
            DrawMode = DrawMode.OwnerDrawFixed;
            ItemHeight = (int)(22 * scale);
            DropDownWidth = width;
            BackColor = Theme.InputBg;
            ForeColor = Theme.Text;
            FlatStyle = FlatStyle.Flat;
            Font = Theme.Ui;

            foreach (var it in items)
                Items.Add(it);

            DrawItem += (_, e) =>
            {
                if (e.Index < 0) return;
                bool selected = (e.State & DrawItemState.Selected) != 0;
                using var bg = new SolidBrush(selected ? Theme.AccentDark : Theme.InputBg);
                e.Graphics.FillRectangle(bg, e.Bounds);

                var color = selected ? Theme.AccentHi : Theme.Text;
                TextRenderer.DrawText(e.Graphics, Items[e.Index]!.ToString(), Theme.Ui,
                    e.Bounds, color,
                    TextFormatFlags.VerticalCenter | TextFormatFlags.Left | TextFormatFlags.NoPrefix);
            };
        }
    }

    private sealed class ModernCheck : CheckBox
    {
        public ModernCheck(string text, bool on, float scale)
        {
            Text = text;
            Checked = on;
            AutoSize = false;
            Font = Theme.Ui;
            ForeColor = Theme.Text;
            FlatStyle = FlatStyle.Flat;
            Cursor = Cursors.Hand;
            FlatAppearance.BorderColor = Theme.Border;
        }
    }

    private static GraphicsPath CreateRound(RectangleF r, float radius)
    {
        var path = new GraphicsPath();
        if (r.Width < 1 || r.Height < 1)
        {
            path.AddRectangle(r);
            return path;
        }
        float d = Math.Min(radius * 2, Math.Min(r.Width, r.Height));
        if (d < 1)
        {
            path.AddRectangle(r);
            return path;
        }
        path.AddArc(r.X, r.Y, d, d, 180, 90);
        path.AddArc(r.Right - d, r.Y, d, d, 270, 90);
        path.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
        path.AddArc(r.X, r.Bottom - d, d, d, 90, 90);
        path.CloseFigure();
        return path;
    }
}

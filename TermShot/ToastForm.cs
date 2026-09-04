using System.Drawing.Drawing2D;

namespace TermShot;

internal static class Toast
{
    private static ToastForm? _current;
    private static readonly object _lock = new();

    public static void Show(string title, string text)
    {
        if (Application.OpenForms.Count > 0)
        {
            var main = Application.OpenForms[0];
            if (main != null && main.InvokeRequired)
            {
                main.BeginInvoke(new Action(() => ShowInternal(title, text)));
                return;
            }
        }
        ShowInternal(title, text);
    }

    private static void ShowInternal(string title, string text)
    {
        lock (_lock)
        {
            if (_current != null && !_current.IsDisposed)
            {
                _current.UpdateContent(title, text);
                return;
            }

            var form = new ToastForm(title, text);
            _current = form;
            form.FormClosed += (_, _) =>
            {
                lock (_lock)
                {
                    if (_current == form)
                        _current = null;
                }
            };
            form.Show();
        }
    }

    public static void Close()
    {
        lock (_lock)
        {
            if (_current != null && !_current.IsDisposed)
            {
                _current.Close();
                _current = null;
            }
        }
    }
}

internal sealed class ToastForm : Form
{
    private readonly System.Windows.Forms.Timer _timer;
    private readonly TextBox _contentBox;
    private readonly Label _titleLabel;
    private readonly Label _closeLabel;
    private readonly Label _tipLabel;
    private int _remainingMs;
    private bool _mouseInside;
    private float _scale = 1.0f;

    private int S(float px) => (int)Math.Round(px * _scale);

    public ToastForm(string title, string text)
    {
        AutoScaleMode = AutoScaleMode.None;
        FormBorderStyle = FormBorderStyle.None;
        ShowInTaskbar = false;
        StartPosition = FormStartPosition.Manual;
        TopMost = true;
        DoubleBuffered = true;
        BackColor = Color.FromArgb(0x14, 0x18, 0x22);
        ForeColor = Theme.Text;
        Font = Theme.Ui;
        Opacity = 0.0;

        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer, true);

        _titleLabel = new Label
        {
            AutoSize = false,
            Font = Theme.UiBold,
            ForeColor = Theme.AccentHi,
            TextAlign = ContentAlignment.MiddleLeft,
            BackColor = Color.Transparent
        };
        _titleLabel.MouseEnter += (_, _) => OnAreaEnter();
        _titleLabel.MouseLeave += (_, _) => OnAreaLeave();

        _closeLabel = new Label
        {
            Text = "✕",
            Font = new Font("Segoe UI", 9f, FontStyle.Bold),
            ForeColor = Theme.Dim,
            TextAlign = ContentAlignment.MiddleCenter,
            Cursor = Cursors.Hand,
            BackColor = Color.Transparent
        };
        _closeLabel.MouseEnter += (_, _) =>
        {
            _closeLabel.ForeColor = Color.White;
            OnAreaEnter();
        };
        _closeLabel.MouseLeave += (_, _) =>
        {
            _closeLabel.ForeColor = Theme.Dim;
            OnAreaLeave();
        };
        _closeLabel.Click += (_, _) => Close();

        _contentBox = new TextBox
        {
            Multiline = true,
            ReadOnly = true,
            BorderStyle = BorderStyle.None,
            BackColor = Color.FromArgb(0x14, 0x18, 0x22),
            ForeColor = Theme.Text,
            Font = Theme.Ui,
            ScrollBars = ScrollBars.None
        };
        _contentBox.MouseEnter += (_, _) => OnAreaEnter();
        _contentBox.MouseLeave += (_, _) => OnAreaLeave();

        _tipLabel = new Label
        {
            AutoSize = false,
            Font = Theme.UiSmall,
            ForeColor = Theme.Dim,
            TextAlign = ContentAlignment.MiddleRight,
            BackColor = Color.Transparent,
            Text = "已复制到剪贴板"
        };
        _tipLabel.MouseEnter += (_, _) => OnAreaEnter();
        _tipLabel.MouseLeave += (_, _) => OnAreaLeave();

        Controls.Add(_titleLabel);
        Controls.Add(_closeLabel);
        Controls.Add(_contentBox);
        Controls.Add(_tipLabel);

        _timer = new System.Windows.Forms.Timer { Interval = 30 };
        _timer.Tick += OnTick;

        UpdateContent(title, text);
    }

    protected override bool ShowWithoutActivation => true;

    protected override CreateParams CreateParams
    {
        get
        {
            var cp = base.CreateParams;
            cp.ExStyle |= NativeMethods.WS_EX_TOPMOST | NativeMethods.WS_EX_TOOLWINDOW
                        | NativeMethods.WS_EX_NOACTIVATE;
            return cp;
        }
    }

    public void UpdateContent(string title, string text)
    {
        // 获取当前鼠标所在屏幕并计算实际 DPI 缩放
        var screen = Screen.FromPoint(Cursor.Position) ?? Screen.PrimaryScreen;
        var work = screen?.WorkingArea ?? Screen.GetWorkingArea(Cursor.Position);
        _scale = Math.Max(1.0f, DeviceDpi / 96f);

        _titleLabel.Text = title;
        _contentBox.Text = text;

        // 根据内容量计算展示时长：短信息 2.8 秒，长文本多给时间阅读，上限 8 秒
        int charCount = text.Length;
        _remainingMs = charCount > 120 ? 8000 : (charCount > 40 ? 5500 : 3000);

        // -------------------------------------------------------------
        // 自适应宽高计算：根据文字实际内容动态撑大
        // -------------------------------------------------------------
        int minW = S(320);
        int maxW = Math.Min(S(520), work.Width - S(40));
        int padX = S(16);
        int padY = S(12);
        int headerH = S(26);
        int closeW = S(24);

        // 文本测量：先按理想内容宽度测量
        int targetContentW = S(360);
        // 如果文本单行很长或整体字数很多，自适应放宽宽度
        if (charCount > 80 || text.Contains('\n'))
            targetContentW = S(440);

        int candidateW = Math.Clamp(targetContentW + padX * 2, minW, maxW);
        int innerContentW = candidateW - padX * 2;

        var flags = TextFormatFlags.WordBreak | TextFormatFlags.TextBoxControl;
        var textSize = TextRenderer.MeasureText(text, Theme.Ui, new Size(innerContentW, int.MaxValue), flags);

        // 垂直高度自适应
        int minH = S(80);
        int maxH = Math.Min(S(420), work.Height - S(80));

        bool needTip = charCount >= 30;
        int tipH = needTip ? S(20) : 0;
        _tipLabel.Visible = needTip;

        int idealH = padY + headerH + S(6) + textSize.Height + S(6) + tipH + padY;
        int actualH = Math.Clamp(idealH, minH, maxH);
        int actualW = candidateW;

        // 设置控件尺寸与位置
        _titleLabel.Bounds = new Rectangle(padX, padY, actualW - padX * 2 - closeW, headerH);
        _closeLabel.Bounds = new Rectangle(actualW - padX - closeW, padY, closeW, headerH);

        int contentTop = padY + headerH + S(6);
        int contentH = actualH - contentTop - (needTip ? tipH + padY : padY);

        _contentBox.Bounds = new Rectangle(padX, contentTop, innerContentW, contentH);
        _contentBox.ScrollBars = textSize.Height > contentH ? ScrollBars.Vertical : ScrollBars.None;

        if (needTip)
        {
            _tipLabel.Bounds = new Rectangle(padX, actualH - padY - tipH, innerContentW, tipH);
        }

        // 定位到屏幕右下角（留出工作区边距）
        int margin = S(16);
        int posX = work.Right - actualW - margin;
        int posY = work.Bottom - actualH - margin;

        Bounds = new Rectangle(posX, posY, actualW, actualH);
        Invalidate();

        if (!_timer.Enabled)
        {
            Opacity = 0.05;
            _timer.Start();
        }
    }

    private void OnAreaEnter()
    {
        _mouseInside = true;
        // 鼠标进入时，如果正在淡出则恢复为完全不透明
        if (Opacity < 1.0) Opacity = 1.0;
    }

    private void OnAreaLeave()
    {
        // 延迟检查鼠标是否真正离开了窗体范围
        var clientPt = PointToClient(Cursor.Position);
        if (!ClientRectangle.Contains(clientPt))
        {
            _mouseInside = false;
            // 鼠标移出后，若剩余时间过短，补足 2 秒给用户反应时间
            if (_remainingMs < 2000)
                _remainingMs = 2000;
        }
    }

    protected override void OnMouseEnter(EventArgs e)
    {
        base.OnMouseEnter(e);
        OnAreaEnter();
    }

    protected override void OnMouseLeave(EventArgs e)
    {
        base.OnMouseLeave(e);
        OnAreaLeave();
    }

    private void OnTick(object? sender, EventArgs e)
    {
        // 鼠标停留时暂停倒计时
        if (_mouseInside)
        {
            if (Opacity < 0.98)
                Opacity = Math.Min(0.98, Opacity + 0.15);
            return;
        }

        // 淡入动画
        if (Opacity < 0.98 && _remainingMs > 400)
        {
            Opacity = Math.Min(0.98, Opacity + 0.15);
        }

        _remainingMs -= _timer.Interval;

        // 淡出动画（最后 350ms）
        if (_remainingMs <= 350)
        {
            Opacity = Math.Max(0.0, Opacity - 0.09);
            if (Opacity <= 0.05 || _remainingMs <= 0)
            {
                _timer.Stop();
                Close();
            }
        }
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        var g = e.Graphics;
        g.SmoothingMode = SmoothingMode.AntiAlias;

        var r = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
        using var path = CreateRound(r, 8f * _scale);

        // 卡片背景填充
        using var fill = new SolidBrush(BackColor);
        g.FillPath(fill, path);

        // 边框绘制
        using var borderPen = new Pen(Color.FromArgb(0x28, 0x34, 0x46), 1f);
        g.DrawPath(borderPen, path);

        // 顶部微光装饰线
        int padX = S(16);
        using var accentPen = new Pen(Theme.Accent, 2f);
        g.DrawLine(accentPen, padX, 1, padX + S(36), 1);
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            _timer.Stop();
            _timer.Dispose();
        }
        base.Dispose(disposing);
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

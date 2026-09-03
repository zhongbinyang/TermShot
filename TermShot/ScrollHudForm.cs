using System.Drawing.Drawing2D;

namespace TermShot;

internal enum ScrollHudDecision
{
    Finish,
    Cancel
}

internal sealed class ScrollHudForm : Form
{
    private readonly Rectangle _region;
    private readonly Rectangle _virtual;
    private Rectangle _hole;
    private Rectangle _bar;
    private Rectangle _doneBtn;
    private Rectangle _cancelBtn;
    private int _height;
    private int _hover = -1;

    public ScrollHudDecision? Decision { get; private set; }

    public ScrollHudForm(Rectangle screenRegion)
    {
        _region = screenRegion;
        _height = screenRegion.Height;
        _virtual = NativeMethods.GetVirtualScreen();

        AutoScaleMode = AutoScaleMode.None;
        FormBorderStyle = FormBorderStyle.None;
        ShowInTaskbar = false;
        StartPosition = FormStartPosition.Manual;
        TopMost = true;
        KeyPreview = true;
        DoubleBuffered = true;
        Bounds = _virtual;
        BackColor = Theme.WinBg;
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer, true);
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

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        PlaceOnVirtualScreen();
        LayoutChrome();
    }

    protected override void OnShown(EventArgs e)
    {
        base.OnShown(e);
        PlaceOnVirtualScreen();
        LayoutChrome();
    }

    private void PlaceOnVirtualScreen()
    {
        NativeMethods.SetWindowPos(Handle, new IntPtr(NativeMethods.HWND_TOPMOST),
            _virtual.X, _virtual.Y, _virtual.Width, _virtual.Height,
            NativeMethods.SWP_SHOWWINDOW | NativeMethods.SWP_NOACTIVATE);
    }

    public void SetHeight(int height)
    {
        if (height == _height) return;
        _height = height;
        Invalidate();
    }

    private Rectangle ScreenToClientRect(Rectangle screen)
    {
        int cw = Math.Max(1, ClientSize.Width);
        int ch = Math.Max(1, ClientSize.Height);
        int vw = Math.Max(1, _virtual.Width);
        int vh = Math.Max(1, _virtual.Height);
        int x1 = (screen.X - _virtual.X) * cw / vw;
        int y1 = (screen.Y - _virtual.Y) * ch / vh;
        int x2 = (screen.Right - _virtual.X) * cw / vw;
        int y2 = (screen.Bottom - _virtual.Y) * ch / vh;
        return Rectangle.FromLTRB(x1, y1, Math.Max(x1 + 1, x2), Math.Max(y1 + 1, y2));
    }

    private void LayoutChrome()
    {
        _hole = ScreenToClientRect(_region);
        float scale = DeviceDpi > 0 ? DeviceDpi / 96f : 1f;
        int barH = Math.Max(32, (int)Math.Round(36 * scale));
        int pad = Math.Max(6, (int)Math.Round(8 * scale));
        int btnW = Math.Max(56, (int)Math.Round(68 * scale));
        int gap = Math.Max(4, (int)Math.Round(6 * scale));
        var text = StatusText();
        var size = TextRenderer.MeasureText(text, Theme.Ui, new Size(int.MaxValue, int.MaxValue),
            TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
        int barW = Math.Max(size.Width + pad * 3 + btnW * 2 + gap, (int)Math.Round(360 * scale));

        int x = _hole.X + (_hole.Width - barW) / 2;
        int y = _hole.Bottom + pad;
        if (y + barH > ClientSize.Height - 4)
            y = _hole.Y - barH - pad;
        x = Math.Clamp(x, 4, Math.Max(4, ClientSize.Width - barW - 4));
        y = Math.Clamp(y, 4, Math.Max(4, ClientSize.Height - barH - 4));

        _bar = new Rectangle(x, y, barW, barH);
        _doneBtn = new Rectangle(_bar.Right - pad - btnW, _bar.Y + 4, btnW, barH - 8);
        _cancelBtn = new Rectangle(_doneBtn.X - gap - btnW, _doneBtn.Y, btnW, _doneBtn.Height);
        ApplyWindowRegion();
    }

    private void ApplyWindowRegion()
    {
        using var rgn = new Region(Rectangle.Inflate(_hole, 4, 4));
        rgn.Exclude(_hole);
        rgn.Union(_bar);
        Region?.Dispose();
        Region = rgn.Clone();
    }

    private string StatusText() =>
        $"用滚轮向下滚  高度 {_height} px  ·  Enter 完成  ·  Esc 取消";

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.PixelOffsetMode = PixelOffsetMode.HighQuality;

        g.SmoothingMode = SmoothingMode.None;
        using (var outer = new Pen(Color.FromArgb(220, 8, 10, 14), 3f))
        using (var inner = new Pen(Theme.Accent, 1f))
        {
            DrawFrame(g, outer, _hole, 2);
            DrawFrame(g, inner, _hole, 1);
        }
        g.SmoothingMode = SmoothingMode.AntiAlias;

        var box = new RectangleF(_bar.X, _bar.Y, _bar.Width, _bar.Height);
        using (var shadow = new SolidBrush(Color.FromArgb(70, 0, 0, 0)))
            FillRound(g, shadow, box with { Y = box.Y + 2, Height = box.Height + 1 }, 8);
        using (var bg = new SolidBrush(Color.FromArgb(242, 18, 22, 30)))
            FillRound(g, bg, box, 8);
        using (var border = new Pen(Color.FromArgb(90, Theme.Accent), 1f))
            DrawRound(g, border, box, 8);

        TextRenderer.DrawText(g, StatusText(), Theme.Ui,
            new Point(_bar.X + 10, _bar.Y + (_bar.Height - Theme.Ui.Height) / 2),
            Theme.Text, TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);

        PaintBtn(g, _doneBtn, "完成", _hover == 0, Theme.AccentHi);
        PaintBtn(g, _cancelBtn, "取消", _hover == 1, Theme.Text);
    }

    private static void PaintBtn(Graphics g, Rectangle r, string text, bool hover, Color fg)
    {
        if (hover)
        {
            using var fill = new SolidBrush(Color.FromArgb(180, Theme.AccentDark));
            FillRound(g, fill, r, 6);
        }
        var size = TextRenderer.MeasureText(text, Theme.UiBold, new Size(int.MaxValue, int.MaxValue),
            TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
        TextRenderer.DrawText(g, text, Theme.UiBold,
            new Point(r.X + (r.Width - size.Width) / 2, r.Y + (r.Height - size.Height) / 2),
            fg, TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        int h = _doneBtn.Contains(e.Location) ? 0 : _cancelBtn.Contains(e.Location) ? 1 : -1;
        Cursor = h >= 0 ? Cursors.Hand : Cursors.Default;
        if (h != _hover)
        {
            _hover = h;
            Invalidate();
        }
        base.OnMouseMove(e);
    }

    protected override void OnMouseDown(MouseEventArgs e)
    {
        if (e.Button == MouseButtons.Left)
        {
            if (_doneBtn.Contains(e.Location))
                Decision = ScrollHudDecision.Finish;
            else if (_cancelBtn.Contains(e.Location))
                Decision = ScrollHudDecision.Cancel;
        }
        else if (e.Button == MouseButtons.Right)
            Decision = ScrollHudDecision.Cancel;
        base.OnMouseDown(e);
    }

    private static void DrawFrame(Graphics g, Pen pen, Rectangle hole, int outset)
    {
        int x = hole.X - outset;
        int y = hole.Y - outset;
        int w = hole.Width + outset * 2 - 1;
        int h = hole.Height + outset * 2 - 1;
        g.DrawRectangle(pen, x, y, w, h);
    }

    private static void FillRound(Graphics g, Brush brush, RectangleF r, float radius)
    {
        using var path = RoundPath(r, radius);
        g.FillPath(brush, path);
    }

    private static void DrawRound(Graphics g, Pen pen, RectangleF r, float radius)
    {
        using var path = RoundPath(r, radius);
        g.DrawPath(pen, path);
    }

    private static GraphicsPath RoundPath(RectangleF r, float radius)
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

using System.Drawing.Drawing2D;

namespace TermShot;

internal sealed class PastePinForm : Form
{
    private const float MinZoom = 0.08f;
    private const float MaxZoom = 8f;
    private const int DragThreshold = 4;

    private readonly Bitmap _bmp;
    private readonly AppSettings _settings;
    private readonly ContextMenuStrip _menu;
    private readonly ToolStripMenuItem _copyTextItem;
    private readonly ToolStripMenuItem _copyAllTextItem;
    private readonly bool _centerOnPoint;
    private readonly CancellationTokenSource _ocrCts = new();

    private float _zoom = 1f;
    private DragKind _drag;
    private Point _dragOffset;
    private Point _dragStart;
    private bool _owned = true;
    private OcrPage? _page;
    private bool _ocrFailed;
    private int _selFrom = -1;
    private int _selTo = -1;
    private int _hoverGlyph = -1;
    private bool _hoverChip;
    private Rectangle _copyChip;

    private enum DragKind { None, Move, Select }

    public PastePinForm(Bitmap bmp, Point preferredTopLeft, AppSettings settings, bool centerOnPoint = false)
    {
        _bmp = bmp;
        _settings = settings;
        _centerOnPoint = centerOnPoint;

        AutoScaleMode = AutoScaleMode.None;
        FormBorderStyle = FormBorderStyle.None;
        ShowInTaskbar = false;
        TopMost = true;
        KeyPreview = true;
        StartPosition = FormStartPosition.Manual;
        BackColor = Color.Black;
        DoubleBuffered = true;
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer, true);
        Cursor = Cursors.SizeAll;
        MinimumSize = new Size(24, 24);

        _copyTextItem = Item("复制文字", CopySelectedText);
        _copyAllTextItem = Item("复制全部文字", CopyAllText);
        _menu = BuildMenu();
        ContextMenuStrip = _menu;
        _menu.Opening += OnMenuOpening;

        KeyDown += OnKeyDown;
        Place(preferredTopLeft);
    }

    protected override CreateParams CreateParams
    {
        get
        {
            var cp = base.CreateParams;
            cp.ExStyle |= NativeMethods.WS_EX_TOPMOST | NativeMethods.WS_EX_TOOLWINDOW;
            cp.ClassStyle |= NativeMethods.CS_DROPSHADOW;
            return cp;
        }
    }

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        NativeMethods.SetWindowPos(Handle, new IntPtr(NativeMethods.HWND_TOPMOST),
            Left, Top, Width, Height, NativeMethods.SWP_SHOWWINDOW);
    }

    protected override void OnShown(EventArgs e)
    {
        base.OnShown(e);
        StartOcr();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        g.InterpolationMode = _zoom < 0.999f
            ? InterpolationMode.HighQualityBicubic
            : InterpolationMode.NearestNeighbor;
        g.PixelOffsetMode = PixelOffsetMode.Half;
        g.CompositingMode = CompositingMode.SourceCopy;
        g.DrawImage(_bmp, ClientRectangle);
        g.CompositingMode = CompositingMode.SourceOver;
        PaintTextOverlay(g);
        g.SmoothingMode = SmoothingMode.None;
        g.PixelOffsetMode = PixelOffsetMode.None;
        using var border = new Pen(ContainsFocus ? Theme.Accent : Theme.Border, 1f)
        {
            Alignment = PenAlignment.Inset
        };
        g.DrawRectangle(border, 0, 0, ClientSize.Width - 1, ClientSize.Height - 1);
    }

    private void PaintTextOverlay(Graphics g)
    {
        if (_page is not { HasText: true })
            return;

        g.SmoothingMode = SmoothingMode.AntiAlias;
        int from = Math.Min(_selFrom, _selTo);
        int to = Math.Max(_selFrom, _selTo);
        bool hasSel = _selFrom >= 0;

        if (hasSel)
        {
            using var fill = new SolidBrush(Color.FromArgb(110, 45, 140, 230));
            for (int i = from; i <= to; i++)
                FillGlyph(g, fill, _page.Glyphs[i].Bounds);
        }
        else if (_hoverGlyph >= 0)
        {
            using var fill = new SolidBrush(Color.FromArgb(55, 45, 140, 230));
            FillGlyph(g, fill, _page.Glyphs[_hoverGlyph].Bounds);
        }

        if (hasSel)
        {
            _copyChip = LayoutCopyChip(from, to);
            PaintCopyChip(g, _copyChip, _hoverChip);
        }
        else
        {
            _copyChip = Rectangle.Empty;
        }

        g.SmoothingMode = SmoothingMode.None;
    }

    private void FillGlyph(Graphics g, Brush brush, RectangleF bmpBounds)
    {
        var r = ToClient(bmpBounds);
        r.Inflate(1.2f, 1.4f);
        if (r.Width < 1 || r.Height < 1) return;
        using var path = RoundPath(r, Math.Min(3f, r.Height * 0.25f));
        g.FillPath(brush, path);
    }

    private void PaintCopyChip(Graphics g, Rectangle chip, bool hover)
    {
        if (chip.Width < 8) return;
        var box = new RectangleF(chip.X, chip.Y, chip.Width, chip.Height);
        using (var shadow = new SolidBrush(Color.FromArgb(70, 0, 0, 0)))
            FillRound(g, shadow, box with { Y = box.Y + 1.5f }, 5);
        using (var bg = new SolidBrush(Color.FromArgb(hover ? 250 : 236, 16, 20, 28)))
            FillRound(g, bg, box, 5);
        using (var border = new Pen(Color.FromArgb(160, Theme.Accent), 1f))
            DrawRound(g, border, box, 5);
        TextRenderer.DrawText(g, "复制", Theme.UiBold, chip,
            hover ? Theme.AccentHi : Theme.Text,
            TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter |
            TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
    }

    private Rectangle LayoutCopyChip(int from, int to)
    {
        if (_page is null) return Rectangle.Empty;
        var union = RectangleF.Empty;
        for (int i = from; i <= to; i++)
        {
            var r = ToClient(_page.Glyphs[i].Bounds);
            union = union.IsEmpty ? r : RectangleF.Union(union, r);
        }

        int w = 56, h = 26;
        int x = (int)Math.Round(union.X + (union.Width - w) / 2f);
        int y = (int)Math.Round(union.Bottom + 7);
        if (y + h > ClientSize.Height - 3)
            y = (int)Math.Round(union.Y - h - 7);
        x = Math.Clamp(x, 3, Math.Max(3, ClientSize.Width - w - 3));
        y = Math.Clamp(y, 3, Math.Max(3, ClientSize.Height - h - 3));
        return new Rectangle(x, y, w, h);
    }

    protected override void OnActivated(EventArgs e)
    {
        base.OnActivated(e);
        Invalidate();
    }

    protected override void OnDeactivate(EventArgs e)
    {
        base.OnDeactivate(e);
        Invalidate();
    }

    protected override void OnMouseDown(MouseEventArgs e)
    {
        if (e.Button == MouseButtons.Left)
        {
            if (_selFrom >= 0 && _copyChip.Contains(e.Location))
            {
                CopySelectedText();
                return;
            }

            int glyph = HitGlyph(e.Location);
            if (glyph >= 0)
            {
                _drag = DragKind.Select;
                _dragStart = e.Location;
                _selFrom = _selTo = glyph;
                Capture = true;
                Invalidate();
                return;
            }

            ClearSelection();
            _drag = DragKind.Move;
            _dragOffset = e.Location;
            _dragStart = e.Location;
            Capture = true;
        }
        base.OnMouseDown(e);
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        if (_drag == DragKind.Move)
        {
            var screen = PointToScreen(e.Location);
            Location = new Point(screen.X - _dragOffset.X, screen.Y - _dragOffset.Y);
        }
        else if (_drag == DragKind.Select)
        {
            int glyph = HitOrNearest(e.Location);
            if (glyph >= 0 && glyph != _selTo)
            {
                _selTo = glyph;
                Invalidate();
            }
        }
        else
        {
            bool chip = _selFrom >= 0 && _copyChip.Contains(e.Location);
            int hover = chip ? -1 : HitGlyph(e.Location);
            Cursor = chip ? Cursors.Hand : hover >= 0 ? Cursors.IBeam : Cursors.SizeAll;
            if (chip != _hoverChip || hover != _hoverGlyph)
            {
                _hoverChip = chip;
                _hoverGlyph = hover;
                Invalidate();
            }
        }
        base.OnMouseMove(e);
    }

    protected override void OnMouseUp(MouseEventArgs e)
    {
        if (e.Button == MouseButtons.Left && _drag != DragKind.None)
        {
            if (_drag == DragKind.Select
                && Math.Abs(e.X - _dragStart.X) < DragThreshold
                && Math.Abs(e.Y - _dragStart.Y) < DragThreshold)
            {
                _selTo = _selFrom;
                Invalidate();
            }
            _drag = DragKind.None;
            Capture = false;
        }
        base.OnMouseUp(e);
    }

    protected override void OnMouseDoubleClick(MouseEventArgs e)
    {
        if (e.Button != MouseButtons.Left)
        {
            base.OnMouseDoubleClick(e);
            return;
        }

        int glyph = HitGlyph(e.Location);
        if (glyph >= 0 && _page != null)
        {
            _selFrom = _page.LineStart(glyph);
            _selTo = _page.LineEnd(glyph);
            Invalidate();
            return;
        }

        CopyImage();
        base.OnMouseDoubleClick(e);
    }

    protected override void OnMouseLeave(EventArgs e)
    {
        if (_drag == DragKind.None && (_hoverGlyph >= 0 || _hoverChip))
        {
            _hoverGlyph = -1;
            _hoverChip = false;
            Invalidate();
        }
        base.OnMouseLeave(e);
    }

    protected override void OnMouseWheel(MouseEventArgs e)
    {
        if (ModifierKeys.HasFlag(Keys.Shift))
        {
            Opacity = Math.Clamp(Opacity + (e.Delta > 0 ? 0.08 : -0.08), 0.2, 1.0);
            return;
        }

        float factor = e.Delta > 0 ? 1.12f : 1f / 1.12f;
        ApplyZoom(_zoom * factor, PointToScreen(e.Location));
        base.OnMouseWheel(e);
    }

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        e.SuppressKeyPress = true;
        if (e.Control && e.KeyCode == Keys.A && _page is { HasText: true })
        {
            _selFrom = 0;
            _selTo = _page.Glyphs.Count - 1;
            Invalidate();
        }
        else if (e.Control && e.KeyCode == Keys.C)
        {
            if (_selFrom >= 0)
                CopySelectedText();
            else
                CopyImage();
        }
        else if (e.Control && e.KeyCode == Keys.S)
            SaveImage();
        else if (e.KeyCode == Keys.D0 && e.Control)
            ApplyZoom(FitZoom(), null);
        else if (e.KeyCode == Keys.Escape)
        {
            if (_selFrom >= 0)
                ClearSelection();
            else
                Close();
        }
        else if (e.KeyCode == Keys.Delete)
            Close();
    }

    private void StartOcr()
    {
        var clone = new Bitmap(_bmp);
        var ct = _ocrCts.Token;
        _ = Task.Run(async () =>
        {
            OcrPage? page = null;
            bool failed = false;
            try
            {
                page = await OcrService.RecognizeAsync(clone, ct).ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch
            {
                failed = true;
            }
            finally
            {
                clone.Dispose();
            }

            if (ct.IsCancellationRequested || IsDisposed || !IsHandleCreated)
                return;

            try
            {
                BeginInvoke(() =>
                {
                    if (IsDisposed) return;
                    _page = page;
                    _ocrFailed = failed || page is null;
                    Invalidate();
                });
            }
            catch (ObjectDisposedException) { }
            catch (InvalidOperationException) { }
        }, ct);
    }

    private int HitGlyph(Point client)
    {
        if (_page is not { HasText: true }) return -1;
        for (int i = 0; i < _page.Glyphs.Count; i++)
        {
            var r = ToClient(_page.Glyphs[i].Bounds);
            r.Inflate(3, 2);
            if (r.Contains(client))
                return i;
        }
        return -1;
    }

    private int HitOrNearest(Point client)
    {
        int hit = HitGlyph(client);
        if (hit >= 0) return hit;
        if (_page is not { HasText: true }) return -1;

        float best = float.MaxValue;
        int idx = -1;
        for (int i = 0; i < _page.Glyphs.Count; i++)
        {
            var r = ToClient(_page.Glyphs[i].Bounds);
            float cx = r.X + r.Width / 2f;
            float cy = r.Y + r.Height / 2f;
            float dx = client.X - cx;
            float dy = client.Y - cy;
            float d = dx * dx + dy * dy;
            if (d < best)
            {
                best = d;
                idx = i;
            }
        }
        return idx;
    }

    private RectangleF ToClient(RectangleF bmp)
    {
        if (_bmp.Width <= 0 || _bmp.Height <= 0) return bmp;
        float sx = ClientSize.Width / (float)_bmp.Width;
        float sy = ClientSize.Height / (float)_bmp.Height;
        return new RectangleF(bmp.X * sx, bmp.Y * sy, bmp.Width * sx, bmp.Height * sy);
    }

    private void ClearSelection()
    {
        if (_selFrom < 0 && _hoverGlyph < 0) return;
        _selFrom = _selTo = -1;
        _hoverChip = false;
        Invalidate();
    }

    private void Place(Point preferredTopLeft)
    {
        var working = Screen.FromPoint(preferredTopLeft).WorkingArea;
        _zoom = FitZoom(working);
        var size = ZoomedSize();
        int x = _centerOnPoint
            ? preferredTopLeft.X - size.Width / 2
            : preferredTopLeft.X;
        int y = _centerOnPoint
            ? preferredTopLeft.Y - size.Height / 2
            : preferredTopLeft.Y;
        if (x + size.Width > working.Right)
            x = working.Right - size.Width;
        if (y + size.Height > working.Bottom)
            y = working.Bottom - size.Height;
        x = Math.Max(working.Left, x);
        y = Math.Max(working.Top, y);
        Bounds = new Rectangle(x, y, size.Width, size.Height);
    }

    private float FitZoom(Rectangle? working = null)
    {
        var area = working ?? Screen.FromPoint(Location).WorkingArea;
        float maxW = Math.Max(48, area.Width * 0.88f);
        float maxH = Math.Max(48, area.Height * 0.88f);
        float fit = Math.Min(1f, Math.Min(maxW / _bmp.Width, maxH / _bmp.Height));
        return Math.Clamp(fit, MinZoom, MaxZoom);
    }

    private Size ZoomedSize()
    {
        int w = Math.Max(24, (int)Math.Round(_bmp.Width * _zoom));
        int h = Math.Max(24, (int)Math.Round(_bmp.Height * _zoom));
        return new Size(w, h);
    }

    private void ApplyZoom(float zoom, Point? anchorScreen)
    {
        float next = Math.Clamp(zoom, MinZoom, MaxZoom);
        if (Math.Abs(next - _zoom) < 0.0001f) return;

        var old = Bounds;
        Point anchor = anchorScreen ?? new Point(old.X + old.Width / 2, old.Y + old.Height / 2);
        float rx = old.Width > 0 ? (anchor.X - old.X) / (float)old.Width : 0.5f;
        float ry = old.Height > 0 ? (anchor.Y - old.Y) / (float)old.Height : 0.5f;

        _zoom = next;
        var size = ZoomedSize();
        int x = (int)Math.Round(anchor.X - rx * size.Width);
        int y = (int)Math.Round(anchor.Y - ry * size.Height);
        Bounds = new Rectangle(x, y, size.Width, size.Height);
        Invalidate();
    }

    private ContextMenuStrip BuildMenu()
    {
        var menu = new ContextMenuStrip
        {
            Renderer = new DarkMenuRenderer(),
            Font = Theme.Ui,
            ShowImageMargin = false,
            BackColor = Theme.WinBg,
            ForeColor = Theme.Text
        };
        menu.Items.Add(_copyTextItem);
        menu.Items.Add(_copyAllTextItem);
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(Item("复制图片", CopyImage));
        menu.Items.Add(Item("保存图片", SaveImage));
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(Item("关闭", Close));
        return menu;
    }

    private void OnMenuOpening(object? sender, EventArgs e)
    {
        bool hasSel = _selFrom >= 0 && _page is { HasText: true };
        bool hasAny = _page is { HasText: true };
        _copyTextItem.Enabled = hasSel;
        _copyAllTextItem.Enabled = hasAny;
        _copyAllTextItem.Text = _ocrFailed
            ? "未识别到文字"
            : _page is null
                ? "正在识别文字…"
                : "复制全部文字";
    }

    private static ToolStripMenuItem Item(string text, Action action)
    {
        var item = new ToolStripMenuItem(text);
        item.Click += (_, _) => action();
        item.ForeColor = Theme.Text;
        item.BackColor = Theme.WinBg;
        return item;
    }

    private void CopySelectedText()
    {
        if (_page is null || _selFrom < 0) return;
        var text = _page.Slice(_selFrom, _selTo);
        if (CaptureService.TrySetClipboardText(text))
            Flash();
    }

    private void CopyAllText()
    {
        if (_page is not { HasText: true }) return;
        var text = string.IsNullOrWhiteSpace(_page.FullText) ? _page.Slice(0, _page.Glyphs.Count - 1) : _page.FullText;
        if (CaptureService.TrySetClipboardText(text))
            Flash();
    }

    private void CopyImage()
    {
        if (!CaptureService.TrySetClipboardImage(_bmp))
            return;
        Flash();
    }

    private void SaveImage()
    {
        CaptureService.TrySavePng(_bmp, _settings, out _, out _);
    }

    private void Flash()
    {
        var prev = Opacity;
        Opacity = Math.Max(0.35, prev * 0.55);
        var t = new System.Windows.Forms.Timer { Interval = 90 };
        t.Tick += (_, _) =>
        {
            t.Stop();
            t.Dispose();
            if (!IsDisposed) Opacity = prev;
        };
        t.Start();
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

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            _ocrCts.Cancel();
            _ocrCts.Dispose();
            _menu.Opening -= OnMenuOpening;
            _menu.Dispose();
            if (_owned)
            {
                _bmp.Dispose();
                _owned = false;
            }
        }
        base.Dispose(disposing);
    }
}

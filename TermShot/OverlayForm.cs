using System.Drawing.Drawing2D;

namespace TermShot;

internal sealed class OverlayForm : Form
{
    private const int DragThreshold = 4;
    private const int MagSource = 13;
    private const int MagZoom = 10;

    private readonly Bitmap _shot;
    private readonly Rectangle _virtual;
    private readonly List<WindowInfo> _windows;
    private readonly IntPtr _previousForeground;
    private readonly bool _askAfterSelect;
    private readonly bool _scrollAfterSelect;
    private readonly ActionToolbar _toolbar = new();
    private readonly AnnotationSession _ann = new();

    private Point _cursorBmp;
    private Point _dragStartBmp;
    private bool _dragging;
    private bool _dragConfirmed;
    private bool _awaitingAction;
    private Rectangle? _lockedBmpRect;
    private WindowInfo? _hover;
    private System.Windows.Forms.Timer? _toolbarAnim;
    private float _toolbarAlpha;
    private TextBox? _textBox;
    private PointF _textAt;

    public Rectangle? SelectedScreenRect { get; private set; }
    public PostCaptureAction? ChosenAction { get; private set; }
    public bool ScrollCapture { get; private set; }
    public AnnotationSession Annotations => _ann;

    public OverlayForm(Bitmap shot, List<WindowInfo> windows, IntPtr previousForeground,
        bool askAfterSelect = false, bool scrollAfterSelect = false)
    {
        _shot = shot;
        _windows = windows;
        _previousForeground = previousForeground;
        _askAfterSelect = askAfterSelect && !scrollAfterSelect;
        _scrollAfterSelect = scrollAfterSelect;
        _virtual = NativeMethods.GetVirtualScreen();
        _toolbar.ShowScroll = true;

        AutoScaleMode = AutoScaleMode.None;
        FormBorderStyle = FormBorderStyle.None;
        ShowInTaskbar = false;
        StartPosition = FormStartPosition.Manual;
        TopMost = true;
        KeyPreview = true;
        DoubleBuffered = true;
        Cursor = Cursors.Cross;
        UseWaitCursor = false;
        Bounds = _virtual;
        BackColor = Color.Black;
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);

        _cursorBmp = ScreenToBmp(Cursor.Position);
        _hover = WindowEnumerator.HitTest(_windows, BmpToScreen(_cursorBmp));
        _ann.Attach(_shot);
        FormClosed += (_, _) => EndTextInput(commit: false);
    }

    protected override CreateParams CreateParams
    {
        get
        {
            var cp = base.CreateParams;
            cp.ExStyle |= NativeMethods.WS_EX_TOPMOST | NativeMethods.WS_EX_TOOLWINDOW;
            return cp;
        }
    }

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        NativeMethods.SetWindowPos(Handle, new IntPtr(NativeMethods.HWND_TOPMOST),
            _virtual.X, _virtual.Y, _virtual.Width, _virtual.Height, NativeMethods.SWP_SHOWWINDOW);
        NativeMethods.ApplyCursor(Cursor);
    }

    protected override void OnShown(EventArgs e)
    {
        base.OnShown(e);
        TopMost = true;
        NativeMethods.SetWindowPos(Handle, new IntPtr(NativeMethods.HWND_TOPMOST),
            _virtual.X, _virtual.Y, _virtual.Width, _virtual.Height, NativeMethods.SWP_SHOWWINDOW);
        NativeMethods.SetForegroundWindow(Handle);
        Activate();
        Focus();
        Capture = true;
        Cursor = _awaitingAction ? Cursors.Default : Cursors.Cross;
        NativeMethods.ApplyCursor(Cursor);
        Cursor.Current = Cursor;
    }

    protected override void WndProc(ref Message m)
    {
        if (m.Msg == NativeMethods.WM_SETCURSOR
            && (m.LParam.ToInt64() & 0xFFFF) == NativeMethods.HTCLIENT)
        {
            NativeMethods.ApplyCursor(Cursor);
            m.Result = 1;
            return;
        }
        base.WndProc(ref m);
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
            _ann.Dispose();
        base.Dispose(disposing);
    }

    protected override bool ProcessDialogKey(Keys keyData)
    {
        var key = keyData & Keys.KeyCode;
        if (key == Keys.Escape)
        {
            if (_textBox != null)
            {
                EndTextInput(commit: false);
                Invalidate();
                return true;
            }
            if (_awaitingAction && _ann.HasDraft)
            {
                _ann.CancelDraft();
                Invalidate();
                return true;
            }
            Cancel();
            return true;
        }
        if (_textBox != null)
            return false;
        if (key == Keys.R)
        {
            TryBeginScroll();
            return true;
        }
        if (_awaitingAction)
        {
            if (key == Keys.Tab)
            {
                _ann.CycleTab();
                Invalidate();
                return true;
            }
            if (key == Keys.A)
            {
                _ann.Toggle(AnnotKind.Arrow);
                Invalidate();
                return true;
            }
            if (key == Keys.B)
            {
                _ann.Toggle(AnnotKind.Pencil);
                Invalidate();
                return true;
            }
            if (key == Keys.H)
            {
                _ann.Toggle(AnnotKind.Marker);
                Invalidate();
                return true;
            }
            if (key == Keys.M)
            {
                _ann.Toggle(AnnotKind.Mosaic);
                Invalidate();
                return true;
            }
            if (key == Keys.X)
            {
                _ann.Toggle(AnnotKind.Text);
                Invalidate();
                return true;
            }
            if (key == Keys.E)
            {
                _ann.Toggle(AnnotKind.Eraser);
                Invalidate();
                return true;
            }
            if (key == Keys.Z)
            {
                if (_ann.Undo()) Invalidate();
                return true;
            }
            if (key is Keys.D1 or Keys.D2 or Keys.D3 or Keys.D4)
            {
                _ann.SetWidthIndex(key - Keys.D1);
                Invalidate();
                return true;
            }
            if (key == Keys.T)
            {
                Choose(PostCaptureAction.Pin);
                return true;
            }
            if (key == Keys.S)
            {
                Choose(PostCaptureAction.SaveImage);
                return true;
            }
            if (key == Keys.C)
            {
                Choose(PostCaptureAction.CopyImage);
                return true;
            }
            if (key == Keys.P)
            {
                Choose(PostCaptureAction.CopyPath);
                return true;
            }
            if (key == Keys.O)
            {
                Choose(PostCaptureAction.CopyText);
                return true;
            }
            if (key == Keys.L)
            {
                Choose(PostCaptureAction.Translate);
                return true;
            }
            return true;
        }
        if (key == Keys.Enter)
        {
            if (_dragConfirmed)
                Complete(BmpRectToScreen(NormalizedDrag()));
            else if (_hover is { } w)
                Complete(w.Bounds);
            return true;
        }
        return base.ProcessDialogKey(keyData);
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        _cursorBmp = ClientToBmp(e.Location);
        if (_awaitingAction)
        {
            if (_ann.HasDraft)
            {
                _ann.Move(ClampToSelection(_cursorBmp), snap45: ModifierKeys.HasFlag(Keys.Shift));
                Invalidate();
                return;
            }
            bool dirty = _toolbar.SetHover(e.Location);
            Cursor = CursorForAwait(e.Location);
            if (dirty) Invalidate();
            return;
        }
        if (_dragging)
        {
            var dx = _cursorBmp.X - _dragStartBmp.X;
            var dy = _cursorBmp.Y - _dragStartBmp.Y;
            if (Math.Abs(dx) >= DragThreshold || Math.Abs(dy) >= DragThreshold)
                _dragConfirmed = true;
        }
        else
        {
            _hover = WindowEnumerator.HitTest(_windows, BmpToScreen(_cursorBmp));
        }
        Invalidate();
        base.OnMouseMove(e);
    }

    protected override void OnMouseDown(MouseEventArgs e)
    {
        if (e.Button == MouseButtons.Right)
        {
            if (_awaitingAction && _ann.HasDraft)
            {
                _ann.CancelDraft();
                Invalidate();
                return;
            }
            Cancel();
            return;
        }
        if (e.Button == MouseButtons.Left)
        {
            if (_awaitingAction)
            {
                var hit = _toolbar.Hit(e.Location);
                if (hit is ToolbarResult.Color)
                {
                    int c = _toolbar.HitTestColor(e.Location);
                    if (c >= 0) _ann.ColorIndex = c;
                    Invalidate();
                    return;
                }
                if (hit is ToolbarResult.Width)
                {
                    int w = _toolbar.HitTestWidth(e.Location);
                    if (w >= 0) _ann.SetWidthIndex(w);
                    Invalidate();
                    return;
                }
                if (hit is not ToolbarResult.Miss)
                {
                    if (_textBox != null)
                        EndTextInput(commit: true);
                    _toolbar.PressedIndex = _toolbar.HitTest(e.Location);
                    if (_toolbar.PressedIndex >= 0)
                        Invalidate();
                    return;
                }
                if (_textBox != null)
                {
                    EndTextInput(commit: true);
                    Invalidate();
                }
                _cursorBmp = ClientToBmp(e.Location);
                var at = ClampToSelection(_cursorBmp);
                if (_ann.IsTextTool)
                {
                    BeginTextAt(at);
                    return;
                }
                if (_ann.CanDraw)
                {
                    _ann.Begin(at);
                    Cursor = Cursors.Cross;
                    Invalidate();
                }
                return;
            }
            _dragging = true;
            _dragConfirmed = false;
            _dragStartBmp = ClientToBmp(e.Location);
            _cursorBmp = _dragStartBmp;
            Invalidate();
        }
        base.OnMouseDown(e);
    }

    protected override void OnMouseUp(MouseEventArgs e)
    {
        if (e.Button == MouseButtons.Left && _awaitingAction)
        {
            if (_ann.HasDraft)
            {
                _ann.Move(ClampToSelection(ClientToBmp(e.Location)), snap45: ModifierKeys.HasFlag(Keys.Shift));
                _ann.CommitDraft();
                Invalidate();
                return;
            }
            int hit = _toolbar.HitTest(e.Location);
            int pressed = _toolbar.PressedIndex;
            _toolbar.PressedIndex = -1;
            if (pressed >= 0 && hit == pressed)
                ApplyToolbar(_toolbar.ResultAt(pressed));
            else
                Invalidate();
            return;
        }
        if (e.Button == MouseButtons.Left && _dragging)
        {
            _cursorBmp = ClientToBmp(e.Location);
            if (_dragConfirmed)
            {
                var r = NormalizedDrag();
                if (r.Width >= 1 && r.Height >= 1)
                    Complete(BmpRectToScreen(r));
                else
                    _dragging = false;
            }
            else if (_hover is { } w)
            {
                Complete(w.Bounds);
            }
            else
            {
                _dragging = false;
                _dragConfirmed = false;
            }
            Invalidate();
        }
        base.OnMouseUp(e);
    }

    protected override void OnMouseLeave(EventArgs e)
    {
        // spanning all screens; ignore
        base.OnMouseLeave(e);
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        try
        {
            PaintOverlay(e.Graphics);
        }
        catch
        {
            // 单帧绘制失败不应弹出 JIT 对话框或拆掉框选
        }
    }

    private void PaintOverlay(Graphics g)
    {
        if (ClientSize.Width <= 0 || ClientSize.Height <= 0 || _shot.Width <= 0 || _shot.Height <= 0)
            return;

        g.ResetTransform();
        g.PageUnit = GraphicsUnit.Pixel;
        g.InterpolationMode = InterpolationMode.NearestNeighbor;
        g.PixelOffsetMode = PixelOffsetMode.Half;
        g.SmoothingMode = SmoothingMode.None;
        g.CompositingMode = CompositingMode.SourceCopy;
        g.DrawImage(_shot, ClientRectangle);
        g.CompositingMode = CompositingMode.SourceOver;

        using (var veil = new SolidBrush(Theme.Veil))
            g.FillRectangle(veil, ClientRectangle);

        var highlightBmp = CurrentHighlightBmp();
        if (highlightBmp is { } hr && hr.Width > 0 && hr.Height > 0)
        {
            var clientHr = BmpToClient(hr);
            if (clientHr.Width > 0 && clientHr.Height > 0)
            {
                g.SetClip(clientHr);
                g.CompositingMode = CompositingMode.SourceCopy;
                g.DrawImage(_shot, clientHr, hr, GraphicsUnit.Pixel);
                g.CompositingMode = CompositingMode.SourceOver;
                g.ResetClip();
                DrawSelectionChrome(g, clientHr, hr);
                DrawAnnotations(g, hr, clientHr);
            }
        }

        DrawCrosshair(g);
        DrawHint(g);
        DrawMagnifier(g);
        DrawActionToolbar(g);
    }

    private Rectangle? CurrentHighlightBmp()
    {
        if (_lockedBmpRect is { } locked)
            return locked;
        if (_dragConfirmed)
            return NormalizedDrag();
        if (_hover is { } w)
            return ScreenRectToBmp(w.Bounds);
        return null;
    }

    private void DrawCrosshair(Graphics g)
    {
        if (_awaitingAction) return;
        var c = BmpToClient(_cursorBmp);
        using var shadow = new Pen(Color.FromArgb(180, 0, 0, 0), 1f);
        using var pen = new Pen(Color.FromArgb(230, Theme.Accent), 1f);
        g.DrawLine(shadow, 0, c.Y + 1, ClientSize.Width, c.Y + 1);
        g.DrawLine(shadow, c.X + 1, 0, c.X + 1, ClientSize.Height);
        g.DrawLine(pen, 0, c.Y, ClientSize.Width, c.Y);
        g.DrawLine(pen, c.X, 0, c.X, ClientSize.Height);
    }

    private void DrawSelectionChrome(Graphics g, Rectangle clientRect, Rectangle bmpRect)
    {
        using var outer = new Pen(Color.FromArgb(220, 8, 10, 14), 3f);
        using var inner = new Pen(Theme.Accent, 1f);
        g.DrawRectangle(outer, clientRect.X, clientRect.Y, clientRect.Width, clientRect.Height);
        g.DrawRectangle(inner, clientRect.X, clientRect.Y, clientRect.Width, clientRect.Height);

        var label = $"{bmpRect.Width} × {bmpRect.Height}";
        if (!_dragConfirmed && _hover is { } w && !string.IsNullOrWhiteSpace(w.Title))
        {
            var title = w.Title.Length > 42 ? w.Title[..42] + "…" : w.Title;
            label = $"{title}  ·  {label}";
        }

        var size = MeasureUi(label, Theme.UiBold);
        int lx = clientRect.X;
        int ly = clientRect.Y - size.Height - 10;
        if (ly < 4) ly = clientRect.Y + 6;
        var box = new RectangleF(lx, ly, size.Width + 14, size.Height + 6);
        using var bg = new SolidBrush(Color.FromArgb(230, 12, 18, 24));
        g.SmoothingMode = SmoothingMode.AntiAlias;
        FillRound(g, bg, box, 4);
        TextRenderer.DrawText(g, label, Theme.UiBold,
            new Point(lx + 7, ly + 3), Theme.Text, TextFlags);
        g.SmoothingMode = SmoothingMode.None;
    }

    private void DrawHint(Graphics g)
    {
        if (_awaitingAction) return;
        var screenPt = BmpToScreen(_cursorBmp);
        var mon = WindowEnumerator.MonitorFromPoint(screenPt);
        var text = _dragConfirmed
            ? "松开完成  ·  R 滚动  ·  Esc / 右键取消"
            : _scrollAfterSelect
                ? "拖动框选滚动区域  ·  单击窗口  ·  Esc / 右键取消"
                : "拖动框选  ·  单击窗口  ·  R 滚动  ·  Esc / 右键取消";
        var size = MeasureUi(text, Theme.Hint);
        var monClient = BmpToClient(ScreenRectToBmp(mon));
        float cx = monClient.X + monClient.Width / 2f;
        float y = monClient.Y + 18;
        var box = new RectangleF(cx - (size.Width + 28) / 2f, y, size.Width + 28, size.Height + 10);
        using var bg = new SolidBrush(Color.FromArgb(200, 10, 14, 20));
        FillRound(g, bg, box, 8);
        TextRenderer.DrawText(g, text, Theme.Hint,
            new Point((int)box.X + 14, (int)box.Y + 5), Theme.Text, TextFlags);
    }

    private void DrawMagnifier(Graphics g)
    {
        if (_awaitingAction) return;
        int src = MagSource;
        int zoom = MagZoom;
        int mag = src * zoom;
        int infoH = 44;
        int pad = 3;

        var screenPt = BmpToScreen(_cursorBmp);
        var mon = WindowEnumerator.MonitorFromPoint(screenPt);
        var monBmp = ScreenRectToBmp(mon);

        int offset = 28;
        int totalW = mag + pad * 2;
        int totalH = mag + pad * 2 + infoH;
        int bx = _cursorBmp.X + offset;
        int by = _cursorBmp.Y + offset;
        if (bx + totalW > monBmp.Right - 8) bx = _cursorBmp.X - offset - totalW;
        if (by + totalH > monBmp.Bottom - 8) by = _cursorBmp.Y - offset - totalH;
        bx = Math.Clamp(bx, monBmp.Left + 8, Math.Max(monBmp.Left + 8, monBmp.Right - totalW - 8));
        by = Math.Clamp(by, monBmp.Top + 8, Math.Max(monBmp.Top + 8, monBmp.Bottom - totalH - 8));

        float sx = ClientSize.Width / (float)_shot.Width;
        float sy = ClientSize.Height / (float)_shot.Height;
        var box = new RectangleF(bx * sx, by * sy, totalW * sx, totalH * sy);

        using var bg = new SolidBrush(Theme.MagBg);
        using var border = new Pen(Theme.MagBorder, 1.5f);
        FillRound(g, bg, box, 6);
        using (var path = RoundPath(box, 6))
            g.DrawPath(border, path);

        int half = src / 2;
        var srcRect = new Rectangle(_cursorBmp.X - half, _cursorBmp.Y - half, src, src);
        var dest = new RectangleF(box.X + pad * sx, box.Y + pad * sy, mag * sx, mag * sy);

        g.InterpolationMode = InterpolationMode.NearestNeighbor;
        g.PixelOffsetMode = PixelOffsetMode.Half;
        g.SetClip(dest);
        g.DrawImage(_shot, dest, srcRect, GraphicsUnit.Pixel);
        g.ResetClip();

        using (var grid = new Pen(Color.FromArgb(40, 255, 255, 255)))
        {
            float cw = dest.Width / src;
            float ch = dest.Height / src;
            for (int i = 1; i < src; i++)
            {
                g.DrawLine(grid, dest.X + i * cw, dest.Y, dest.X + i * cw, dest.Y + dest.Height);
                g.DrawLine(grid, dest.X, dest.Y + i * ch, dest.X + dest.Width, dest.Y + i * ch);
            }
        }

        var center = new RectangleF(dest.X + (src / 2) * (dest.Width / src),
            dest.Y + (src / 2) * (dest.Height / src), dest.Width / src, dest.Height / src);
        using var centerPen = new Pen(Theme.Accent, 1.5f);
        g.DrawRectangle(centerPen, center.X, center.Y, center.Width, center.Height);

        var color = PeekColor(_cursorBmp);
        var hex = $"#{color.R:X2}{color.G:X2}{color.B:X2}";
        var rgb = $"{color.R,3} {color.G,3} {color.B,3}";
        var coord = $"{BmpToScreen(_cursorBmp).X}, {BmpToScreen(_cursorBmp).Y}";
        float ty = dest.Bottom + 4 * sy;
        float tx0 = dest.X + 4;
        TextRenderer.DrawText(g, hex, Theme.Mono, new Point((int)tx0, (int)ty), Theme.Text, TextFlags);
        TextRenderer.DrawText(g, rgb, Theme.Mono, new Point((int)(tx0 + 72 * sx), (int)ty), Theme.Dim, TextFlags);
        TextRenderer.DrawText(g, coord, Theme.Mono, new Point((int)tx0, (int)(ty + 16 * sy)), Theme.Dim, TextFlags);
    }

    private Color PeekColor(Point bmpPt)
    {
        int x = Math.Clamp(bmpPt.X, 0, _shot.Width - 1);
        int y = Math.Clamp(bmpPt.Y, 0, _shot.Height - 1);
        return _shot.GetPixel(x, y);
    }

    private Rectangle NormalizedDrag()
    {
        int x1 = Math.Min(_dragStartBmp.X, _cursorBmp.X);
        int y1 = Math.Min(_dragStartBmp.Y, _cursorBmp.Y);
        int x2 = Math.Max(_dragStartBmp.X, _cursorBmp.X);
        int y2 = Math.Max(_dragStartBmp.Y, _cursorBmp.Y);
        x1 = Math.Clamp(x1, 0, _shot.Width - 1);
        y1 = Math.Clamp(y1, 0, _shot.Height - 1);
        x2 = Math.Clamp(x2, 0, _shot.Width);
        y2 = Math.Clamp(y2, 0, _shot.Height);
        return Rectangle.FromLTRB(x1, y1, Math.Max(x1 + 1, x2), Math.Max(y1 + 1, y2));
    }

    private const TextFormatFlags TextFlags =
        TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding | TextFormatFlags.PreserveGraphicsClipping;

    private static Size MeasureUi(string text, Font font) =>
        TextRenderer.MeasureText(text, font, new Size(int.MaxValue, int.MaxValue), TextFlags);

    private Point BmpToClient(Point bmp)
    {
        if (_shot.Width <= 0 || _shot.Height <= 0 || ClientSize.Width <= 0 || ClientSize.Height <= 0)
            return bmp;
        return new Point(
            bmp.X * ClientSize.Width / _shot.Width,
            bmp.Y * ClientSize.Height / _shot.Height);
    }

    private Rectangle BmpToClient(Rectangle bmp)
    {
        var a = BmpToClient(bmp.Location);
        var b = BmpToClient(new Point(bmp.Right, bmp.Bottom));
        return Rectangle.FromLTRB(a.X, a.Y, Math.Max(a.X + 1, b.X), Math.Max(a.Y + 1, b.Y));
    }

    private Point ClientToBmp(Point client)
    {
        if (ClientSize.Width <= 0 || ClientSize.Height <= 0) return client;
        return new Point(
            Math.Clamp(client.X * _shot.Width / ClientSize.Width, 0, _shot.Width - 1),
            Math.Clamp(client.Y * _shot.Height / ClientSize.Height, 0, _shot.Height - 1));
    }

    private Point ScreenToBmp(Point screen) => new(screen.X - _virtual.X, screen.Y - _virtual.Y);

    private Point BmpToScreen(Point bmp) => new(bmp.X + _virtual.X, bmp.Y + _virtual.Y);

    private Rectangle ScreenRectToBmp(Rectangle screen)
    {
        var r = new Rectangle(screen.X - _virtual.X, screen.Y - _virtual.Y, screen.Width, screen.Height);
        r.Intersect(new Rectangle(0, 0, _shot.Width, _shot.Height));
        return r;
    }

    private Rectangle BmpRectToScreen(Rectangle bmp) =>
        new(bmp.X + _virtual.X, bmp.Y + _virtual.Y, bmp.Width, bmp.Height);

    private void Complete(Rectangle screenRect)
    {
        screenRect.Intersect(_virtual);
        if (screenRect.Width < 1 || screenRect.Height < 1)
            return;
        SelectedScreenRect = screenRect;
        if (_scrollAfterSelect)
        {
            ScrollCapture = true;
            RestoreForeground();
            DialogResult = DialogResult.OK;
            Close();
            return;
        }
        if (_askAfterSelect)
        {
            EnterAskMode(screenRect);
            return;
        }
        RestoreForeground();
        DialogResult = DialogResult.OK;
        Close();
    }

    private void EnterAskMode(Rectangle screenRect)
    {
        _awaitingAction = true;
        _dragging = false;
        _dragConfirmed = false;
        _lockedBmpRect = ScreenRectToBmp(screenRect);
        _ann.WidthIndex = 1;
        _ann.Select(AnnotKind.Arrow);
        Capture = true;
        Cursor = Cursors.Cross;
        _toolbarAlpha = 0f;
        _toolbarAnim?.Stop();
        _toolbarAnim?.Dispose();
        _toolbarAnim = new System.Windows.Forms.Timer { Interval = 16 };
        _toolbarAnim.Tick += (_, _) =>
        {
            _toolbarAlpha = Math.Min(1f, _toolbarAlpha + 0.16f);
            Invalidate();
            if (_toolbarAlpha >= 1f)
            {
                _toolbarAnim?.Stop();
                _toolbarAnim?.Dispose();
                _toolbarAnim = null;
            }
        };
        _toolbarAnim.Start();
        Invalidate();
    }

    private void DrawActionToolbar(Graphics g)
    {
        if (!_awaitingAction || _lockedBmpRect is not { } locked)
            return;
        var sel = BmpToClient(locked);
        var center = BmpToScreen(new Point(locked.X + locked.Width / 2, locked.Y + locked.Height / 2));
        var mon = WindowEnumerator.MonitorFromPoint(center);
        var confine = BmpToClient(ScreenRectToBmp(mon));
        if (confine.Width < 8 || confine.Height < 8)
            confine = ClientRectangle;
        float scale = DeviceDpi / 96f;
        SyncToolbar();
        _toolbar.Relayout(sel, confine, scale);
        _toolbar.Paint(g, scale, _toolbarAlpha);
    }

    private void SyncToolbar()
    {
        _toolbar.Tool = _ann.Tool;
        _toolbar.ShapeKind = _ann.ShapeKind;
        _toolbar.StrokeKind = _ann.StrokeKind;
        _toolbar.ShowPalette = _ann.ShowPalette;
        _toolbar.ShowColor = _ann.ShowColor;
        _toolbar.UndoEnabled = _ann.HasMarks || _ann.HasDraft || _textBox != null;
        _toolbar.ColorIndex = _ann.ColorIndex;
        _toolbar.WidthIndex = _ann.WidthIndex;
    }

    private void DrawAnnotations(Graphics g, Rectangle bmpRect, Rectangle clientRect)
    {
        if (!_awaitingAction) return;
        var state = g.Save();
        g.SetClip(clientRect);
        g.CompositingMode = CompositingMode.SourceOver;
        _ann.Paint(g, bmpRect, clientRect);
        g.Restore(state);
    }

    private PointF ClampToSelection(Point bmp)
    {
        if (_lockedBmpRect is not { } r)
            return bmp;
        return new PointF(
            Math.Clamp(bmp.X, r.Left, Math.Max(r.Left, r.Right - 1)),
            Math.Clamp(bmp.Y, r.Top, Math.Max(r.Top, r.Bottom - 1)));
    }

    private Cursor CursorForAwait(Point client)
    {
        var hit = _toolbar.Hit(client);
        if (hit is ToolbarResult.Color or ToolbarResult.Width || _toolbar.HoverIndex >= 0)
            return Cursors.Hand;
        if (hit is ToolbarResult.Chrome)
            return Cursors.Default;
        return _ann.ToolActive ? Cursors.Cross : Cursors.Default;
    }

    private void ApplyToolbar(ToolbarResult result)
    {
        switch (result)
        {
            case ToolbarResult.Shape:
                EndTextInput(commit: true);
                _ann.ToggleShape();
                Invalidate();
                break;
            case ToolbarResult.Stroke:
                EndTextInput(commit: true);
                _ann.ToggleStroke();
                Invalidate();
                break;
            case ToolbarResult.Pencil:
                EndTextInput(commit: true);
                _ann.Toggle(AnnotKind.Pencil);
                Invalidate();
                break;
            case ToolbarResult.Marker:
                EndTextInput(commit: true);
                _ann.Toggle(AnnotKind.Marker);
                Invalidate();
                break;
            case ToolbarResult.Mosaic:
                EndTextInput(commit: true);
                _ann.Toggle(AnnotKind.Mosaic);
                Invalidate();
                break;
            case ToolbarResult.AnnotText:
                EndTextInput(commit: true);
                _ann.Toggle(AnnotKind.Text);
                Invalidate();
                break;
            case ToolbarResult.Eraser:
                EndTextInput(commit: true);
                _ann.Toggle(AnnotKind.Eraser);
                Invalidate();
                break;
            case ToolbarResult.Undo:
                EndTextInput(commit: false);
                if (_ann.Undo()) Invalidate();
                break;
            case ToolbarResult.Close:
                Cancel();
                break;
            case ToolbarResult.Scroll:
                ChooseScroll();
                break;
            default:
                if (ActionToolbar.ToAction(result) is { } action)
                    Choose(action);
                break;
        }
    }

    private void Choose(PostCaptureAction action)
    {
        EndTextInput(commit: true);
        _ann.CommitDraft();
        ChosenAction = action;
        RestoreForeground();
        DialogResult = DialogResult.OK;
        Close();
    }

    private void TryBeginScroll()
    {
        if (_awaitingAction)
        {
            ChooseScroll();
            return;
        }
        if (_dragConfirmed)
        {
            ChooseScroll(BmpRectToScreen(NormalizedDrag()));
            return;
        }
        if (_hover is { } w)
            ChooseScroll(w.Bounds);
    }

    private void ChooseScroll(Rectangle? screenRect = null)
    {
        EndTextInput(commit: false);
        var rect = screenRect ?? SelectedScreenRect;
        if (rect is not { } r)
            return;
        r.Intersect(_virtual);
        if (r.Width < 1 || r.Height < 1)
            return;
        SelectedScreenRect = r;
        ScrollCapture = true;
        ChosenAction = null;
        RestoreForeground();
        DialogResult = DialogResult.OK;
        Close();
    }

    private void Cancel()
    {
        EndTextInput(commit: false);
        SelectedScreenRect = null;
        ChosenAction = null;
        ScrollCapture = false;
        RestoreForeground();
        DialogResult = DialogResult.Cancel;
        Close();
    }

    private void BeginTextAt(PointF bmpPt)
    {
        EndTextInput(commit: true);
        _textAt = bmpPt;
        var client = BmpToClient(new Point((int)Math.Round(bmpPt.X), (int)Math.Round(bmpPt.Y)));
        float px = Math.Clamp(_ann.Width * 3.4f, 12f, 26f);
        _textBox = new TextBox
        {
            BorderStyle = BorderStyle.FixedSingle,
            BackColor = Color.FromArgb(0x12, 0x18, 0x22),
            ForeColor = _ann.Color,
            Font = new Font("Segoe UI", px, FontStyle.Bold, GraphicsUnit.Pixel),
            Width = Math.Max(160, (int)(220 * DeviceDpi / 96f)),
            ImeMode = ImeMode.On
        };
        _textBox.Left = Math.Clamp(client.X, 8, Math.Max(8, ClientSize.Width - _textBox.Width - 8));
        _textBox.Top = Math.Clamp(client.Y, 8, Math.Max(8, ClientSize.Height - _textBox.Height - 8));
        _textBox.KeyDown += (_, e) =>
        {
            if (e.KeyCode == Keys.Enter)
            {
                e.SuppressKeyPress = true;
                EndTextInput(commit: true);
                Invalidate();
            }
            else if (e.KeyCode == Keys.Escape)
            {
                e.SuppressKeyPress = true;
                EndTextInput(commit: false);
                Invalidate();
            }
        };
        Controls.Add(_textBox);
        Capture = false;
        _textBox.BringToFront();
        _textBox.Focus();
    }

    private void EndTextInput(bool commit)
    {
        if (_textBox == null) return;
        var text = _textBox.Text;
        Controls.Remove(_textBox);
        _textBox.Dispose();
        _textBox = null;
        if (commit && !string.IsNullOrWhiteSpace(text))
            _ann.AddText(_textAt, text);
        if (!IsDisposed)
            Capture = true;
    }

    private void RestoreForeground()
    {
        _toolbarAnim?.Stop();
        _toolbarAnim?.Dispose();
        _toolbarAnim = null;
        Capture = false;
        if (_previousForeground != IntPtr.Zero && NativeMethods.IsWindow(_previousForeground))
            NativeMethods.SetForegroundWindow(_previousForeground);
    }

    private static void FillRound(Graphics g, Brush brush, RectangleF r, float radius)
    {
        if (r.Width < 1 || r.Height < 1) return;
        using var path = RoundPath(r, radius);
        g.FillPath(brush, path);
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

using System.Drawing.Drawing2D;

namespace TermShot;

internal sealed class ActionToolbar
{
    public const int AnnotCount = 8;
    public const int MaxCount = 16;

    private readonly Rectangle[] _buttons = new Rectangle[MaxCount];
    private readonly Rectangle[] _swatches = new Rectangle[AnnotationSession.Colors.Length];
    private readonly Rectangle[] _widths = new Rectangle[AnnotationSession.Widths.Length];

    public Rectangle Bounds { get; private set; }
    public int HoverIndex { get; private set; } = -1;
    public int PressedIndex { get; set; } = -1;
    public int HoverColor { get; private set; } = -1;
    public int HoverWidth { get; private set; } = -1;
    public bool ShowPalette { get; set; }
    public bool ShowColor { get; set; } = true;
    public bool UndoEnabled { get; set; }
    public bool ShowScroll { get; set; }
    public int ColorIndex { get; set; }
    public int WidthIndex { get; set; } = 1;
    public AnnotKind Tool { get; set; } = AnnotKind.Arrow;
    public AnnotKind ShapeKind { get; set; } = AnnotKind.Rect;
    public AnnotKind StrokeKind { get; set; } = AnnotKind.Arrow;
    public int VisibleCount => AnnotCount + (ShowScroll ? 8 : 7);

    public void Relayout(Rectangle selection, Rectangle confine, float scale)
    {
        int n = VisibleCount;
        int pad = Sc(6, scale);
        int btn = Sc(32, scale);
        int gap = Sc(2, scale);
        int sep = Sc(8, scale);
        int pal = ShowPalette ? Sc(30, scale) : 0;
        int w = pad * 2 + btn * n + gap * (n - 3) + sep * 2;
        int h = pad * 2 + btn + pal;
        int margin = Sc(8, scale);

        int x = selection.X + (selection.Width - w) / 2;
        int y = selection.Bottom + margin;
        if (y + h > confine.Bottom - 2)
            y = selection.Y - h - margin;
        if (y < confine.Top + 2)
            y = Math.Clamp(selection.Bottom - h - margin, confine.Top + 2, Math.Max(confine.Top + 2, confine.Bottom - h - 2));

        int minX = confine.Left + 2;
        int maxX = Math.Max(minX, confine.Right - w - 2);
        x = Math.Clamp(x, minX, maxX);
        int minY = confine.Top + 2;
        int maxY = Math.Max(minY, confine.Bottom - h - 2);
        y = Math.Clamp(y, minY, maxY);

        Bounds = new Rectangle(x, y, w, h);

        int bx = x + pad;
        int by = y + pad;
        for (int i = 0; i < n; i++)
        {
            _buttons[i] = new Rectangle(bx, by, btn, btn);
            bx += btn + (i == AnnotCount - 1 || i == n - 2 ? sep : gap);
        }
        for (int i = n; i < _buttons.Length; i++)
            _buttons[i] = Rectangle.Empty;

        if (ShowPalette)
        {
            int dot = Sc(12, scale);
            int dg = Sc(7, scale);
            int cn = ShowColor ? AnnotationSession.Colors.Length : 0;
            int chipW = Sc(22, scale);
            int chipH = Sc(16, scale);
            int wg = Sc(5, scale);
            int wn = AnnotationSession.Widths.Length;
            int palSep = cn > 0 ? Sc(12, scale) : 0;
            int rowW = cn * dot + Math.Max(0, cn - 1) * dg + palSep + wn * chipW + (wn - 1) * wg;
            int sx = x + (w - rowW) / 2;
            int sy = by + btn + Sc(8, scale);
            Array.Clear(_swatches);
            for (int i = 0; i < cn; i++)
            {
                _swatches[i] = new Rectangle(sx, sy + (chipH - dot) / 2, dot, dot);
                sx += dot + dg;
            }
            if (cn > 0)
                sx += palSep - dg;
            for (int i = 0; i < wn; i++)
            {
                _widths[i] = new Rectangle(sx, sy, chipW, chipH);
                sx += chipW + wg;
            }
        }
        else
        {
            Array.Clear(_swatches);
            Array.Clear(_widths);
        }
    }

    public int HitTest(Point p)
    {
        for (int i = 0; i < VisibleCount; i++)
        {
            if (_buttons[i].Contains(p))
                return i;
        }
        return Bounds.Contains(p) ? -2 : -1;
    }

    public int HitTestColor(Point p)
    {
        if (!ShowPalette || !ShowColor) return -1;
        for (int i = 0; i < _swatches.Length; i++)
        {
            if (_swatches[i].Contains(p))
                return i;
        }
        return -1;
    }

    public int HitTestWidth(Point p)
    {
        if (!ShowPalette) return -1;
        for (int i = 0; i < _widths.Length; i++)
        {
            if (_widths[i].Contains(p))
                return i;
        }
        return -1;
    }

    public ToolbarResult Hit(Point p)
    {
        int b = HitTest(p);
        if (b >= 0) return ResultAt(b);
        if (HitTestColor(p) >= 0) return ToolbarResult.Color;
        if (HitTestWidth(p) >= 0) return ToolbarResult.Width;
        if (Bounds.Contains(p)) return ToolbarResult.Chrome;
        return ToolbarResult.Miss;
    }

    public ToolbarResult ResultAt(int index)
    {
        if (index < 0) return ToolbarResult.Miss;
        if (index < AnnotCount)
        {
            return index switch
            {
                0 => ToolbarResult.Shape,
                1 => ToolbarResult.Stroke,
                2 => ToolbarResult.Pencil,
                3 => ToolbarResult.Marker,
                4 => ToolbarResult.Mosaic,
                5 => ToolbarResult.AnnotText,
                6 => ToolbarResult.Eraser,
                7 => ToolbarResult.Undo,
                _ => ToolbarResult.Miss
            };
        }

        int a = index - AnnotCount;
        if (ShowScroll)
        {
            return a switch
            {
                0 => ToolbarResult.Scroll,
                1 => ToolbarResult.Pin,
                2 => ToolbarResult.Save,
                3 => ToolbarResult.CopyImage,
                4 => ToolbarResult.CopyPath,
                5 => ToolbarResult.CopyText,
                6 => ToolbarResult.Translate,
                7 => ToolbarResult.Close,
                _ => ToolbarResult.Miss
            };
        }

        return a switch
        {
            0 => ToolbarResult.Pin,
            1 => ToolbarResult.Save,
            2 => ToolbarResult.CopyImage,
            3 => ToolbarResult.CopyPath,
            4 => ToolbarResult.CopyText,
            5 => ToolbarResult.Translate,
            6 => ToolbarResult.Close,
            _ => ToolbarResult.Miss
        };
    }

    public static PostCaptureAction? ToAction(ToolbarResult r) => r switch
    {
        ToolbarResult.Pin => PostCaptureAction.Pin,
        ToolbarResult.Save => PostCaptureAction.SaveImage,
        ToolbarResult.CopyImage => PostCaptureAction.CopyImage,
        ToolbarResult.CopyPath => PostCaptureAction.CopyPath,
        ToolbarResult.CopyText => PostCaptureAction.CopyText,
        ToolbarResult.Translate => PostCaptureAction.Translate,
        _ => null
    };

    public bool SetHover(Point p)
    {
        int h = HitTest(p);
        if (h < 0) h = -1;
        int c = HitTestColor(p);
        int w = HitTestWidth(p);
        if (h == HoverIndex && c == HoverColor && w == HoverWidth) return false;
        HoverIndex = h;
        HoverColor = c;
        HoverWidth = w;
        return true;
    }

    public string TipAt(int i)
    {
        return ResultAt(i) switch
        {
            ToolbarResult.Shape => ShapeKind == AnnotKind.Ellipse
                ? "椭圆  ·  Tab 切方框  ·  Shift 正圆"
                : "方框  ·  Tab 切椭圆  ·  Shift 正方形",
            ToolbarResult.Stroke => StrokeKind == AnnotKind.Line
                ? "直线  ·  Tab 切箭头  ·  Shift 45°"
                : "箭头  ·  A  ·  Tab 切直线  ·  Shift 45°",
            ToolbarResult.Pencil => "铅笔  ·  B",
            ToolbarResult.Marker => "荧光笔  ·  H",
            ToolbarResult.Mosaic => "马赛克  ·  M",
            ToolbarResult.AnnotText => "文字  ·  X",
            ToolbarResult.Eraser => "橡皮  ·  E",
            ToolbarResult.Undo => "撤销  ·  Z",
            ToolbarResult.Scroll => "滚动截图  ·  R",
            ToolbarResult.Pin => "贴到桌面  ·  T",
            ToolbarResult.Save => "保存图片  ·  S",
            ToolbarResult.CopyImage => "复制图片  ·  C",
            ToolbarResult.CopyPath => "复制图片地址  ·  P",
            ToolbarResult.CopyText => "复制文字  ·  O",
            ToolbarResult.Translate => "翻译  ·  L",
            ToolbarResult.Close => "取消  ·  Esc",
            _ => ""
        };
    }

    public void Paint(Graphics g, float scale, float alpha)
    {
        if (Bounds.Width < 8 || Bounds.Height < 8 || alpha <= 0)
            return;

        int a = Math.Clamp((int)(255 * alpha), 0, 255);
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.PixelOffsetMode = PixelOffsetMode.HighQuality;

        var box = new RectangleF(Bounds.X, Bounds.Y, Bounds.Width, Bounds.Height);
        using (var shadow = new SolidBrush(Color.FromArgb((int)(70 * alpha), 0, 0, 0)))
            FillRound(g, shadow, box with { Y = box.Y + 2, Height = box.Height + 1 }, Sc(8, scale));
        using (var bg = new SolidBrush(Color.FromArgb((int)(242 * alpha), 18, 22, 30)))
            FillRound(g, bg, box, Sc(8, scale));
        using (var border = new Pen(Color.FromArgb((int)(90 * alpha), Theme.Accent), 1f))
            DrawRound(g, border, box, Sc(8, scale));

        int n = VisibleCount;
        DrawSep(g, _buttons[AnnotCount - 1], _buttons[AnnotCount], a, scale);
        DrawSep(g, _buttons[n - 2], _buttons[n - 1], a, scale);

        for (int i = 0; i < n; i++)
            PaintButton(g, i, scale, a);

        if (ShowPalette)
            PaintPalette(g, scale, a);

        if (HoverWidth >= 0)
            PaintWidthTip(g, HoverWidth, scale, alpha);
        else if (HoverIndex >= 0 && HoverIndex < n)
            PaintTip(g, HoverIndex, scale, alpha);

        g.SmoothingMode = SmoothingMode.None;
    }

    private void DrawSep(Graphics g, Rectangle left, Rectangle right, int a, float scale)
    {
        if (left.Width < 2 || right.Width < 2) return;
        int x = left.Right + (right.Left - left.Right) / 2;
        using var sep = new Pen(Color.FromArgb((int)(50 * (a / 255f)), 255, 255, 255));
        g.DrawLine(sep, x, Bounds.Y + Sc(10, scale), x, left.Bottom - Sc(2, scale));
    }

    private void PaintPalette(Graphics g, float scale, int a)
    {
        if (ShowColor)
        {
            for (int i = 0; i < AnnotationSession.Colors.Length; i++)
            {
                var r = _swatches[i];
                if (r.Width < 2) continue;
                var c = AnnotationSession.Colors[i];
                bool on = i == ColorIndex || i == HoverColor;
                var rr = on ? Rectangle.Inflate(r, 1, 1) : r;
                using var fill = new SolidBrush(Color.FromArgb(a, c));
                g.FillEllipse(fill, rr);
                using var ring = new Pen(
                    i == ColorIndex ? Color.FromArgb(a, Theme.AccentHi) : Color.FromArgb((int)(a * 0.55), 255, 255, 255),
                    i == ColorIndex ? 2f : 1f);
                g.DrawEllipse(ring, rr);
            }

            if (_swatches[0].Width > 1 && _widths[0].Width > 1)
            {
                int x = (_swatches[^1].Right + _widths[0].Left) / 2;
                int top = _widths[0].Y + 2;
                int bot = _widths[0].Bottom - 2;
                using var sep = new Pen(Color.FromArgb((int)(a * 0.22), 255, 255, 255));
                g.DrawLine(sep, x, top, x, bot);
            }
        }

        for (int i = 0; i < AnnotationSession.Widths.Length; i++)
        {
            var r = _widths[i];
            if (r.Width < 2) continue;
            bool on = i == WidthIndex || i == HoverWidth;
            if (on)
            {
                using var bg = new SolidBrush(Color.FromArgb((int)(a * 0.55), Theme.AccentDark));
                FillRound(g, bg, r, Sc(4, scale));
            }
            float lw = 1.4f + i * 1.7f;
            using var pen = new Pen(
                i == WidthIndex ? Color.FromArgb(a, Theme.AccentHi) : Color.FromArgb((int)(a * 0.88), Theme.Text),
                lw)
            {
                StartCap = LineCap.Round,
                EndCap = LineCap.Round
            };
            float y = r.Y + r.Height / 2f;
            float inset = Sc(3, scale);
            g.DrawLine(pen, r.X + inset, y, r.Right - inset, y);
            if (i == WidthIndex)
            {
                using var ring = new Pen(Color.FromArgb(a, Theme.AccentHi), 1.2f);
                DrawRound(g, ring, r, Sc(4, scale));
            }
        }
    }

    private void PaintWidthTip(Graphics g, int i, float scale, float alpha)
    {
        var name = i >= 0 && i < AnnotationSession.WidthNames.Length ? AnnotationSession.WidthNames[i] : "";
        if (name.Length == 0) return;
        var size = TextRenderer.MeasureText(name, Theme.UiSmall, new Size(int.MaxValue, int.MaxValue),
            TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
        int tw = size.Width + Sc(14, scale);
        int th = size.Height + Sc(8, scale);
        var chip = _widths[i];
        int x = chip.X + (chip.Width - tw) / 2;
        int y = Bounds.Y - th - Sc(6, scale);
        if (y < 2) y = Bounds.Bottom + Sc(6, scale);
        var box = new RectangleF(x, y, tw, th);
        using var bg = new SolidBrush(Color.FromArgb((int)(230 * alpha), 10, 14, 20));
        FillRound(g, bg, box, Sc(4, scale));
        TextRenderer.DrawText(g, name, Theme.UiSmall, new Point(x + Sc(7, scale), y + Sc(4, scale)),
            Color.FromArgb((int)(255 * alpha), Theme.Text),
            TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding | TextFormatFlags.PreserveGraphicsClipping);
    }

    private void PaintButton(Graphics g, int i, float scale, int a)
    {
        var r = _buttons[i];
        if (r.Width < 2) return;
        var kind = ResultAt(i);
        bool hover = HoverIndex == i;
        bool pressed = PressedIndex == i;
        bool checkedOn = IsChecked(kind);
        bool dim = kind == ToolbarResult.Undo && !UndoEnabled;
        bool close = kind == ToolbarResult.Close;
        if (hover || pressed || checkedOn)
        {
            var fill = close
                ? Color.FromArgb((int)(a * 0.55), 80, 22, 28)
                : Color.FromArgb((int)(a * (checkedOn && !hover ? 0.85 : 0.7)), Theme.AccentDark);
            using var brush = new SolidBrush(fill);
            FillRound(g, brush, r, Sc(6, scale));
        }

        var color = close && hover
            ? Color.FromArgb(a, Theme.Danger)
            : dim
                ? Color.FromArgb((int)(a * 0.32), Theme.Text)
                : hover || checkedOn
                    ? Color.FromArgb(a, Theme.AccentHi)
                    : Color.FromArgb((int)(a * 0.92), Theme.Text);
        using var pen = new Pen(color, Math.Max(1.3f, 1.55f * scale))
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round,
            LineJoin = LineJoin.Round
        };
        var icon = IconBox(r);
        switch (kind)
        {
            case ToolbarResult.Shape:
                if (ShapeKind == AnnotKind.Ellipse) DrawEllipseIcon(g, icon, pen);
                else DrawRectIcon(g, icon, pen);
                break;
            case ToolbarResult.Stroke:
                if (StrokeKind == AnnotKind.Line) DrawLineIcon(g, icon, pen);
                else DrawArrowIcon(g, icon, pen, color);
                break;
            case ToolbarResult.Pencil: DrawPencil(g, icon, pen); break;
            case ToolbarResult.Marker: DrawMarker(g, icon, pen, color); break;
            case ToolbarResult.Mosaic: DrawMosaic(g, icon, pen, color); break;
            case ToolbarResult.AnnotText: DrawLetter(g, icon, color); break;
            case ToolbarResult.Eraser: DrawEraser(g, icon, pen); break;
            case ToolbarResult.Undo: DrawUndo(g, icon, pen); break;
            case ToolbarResult.Scroll: DrawScroll(g, icon, pen); break;
            case ToolbarResult.Pin: DrawPin(g, icon, pen); break;
            case ToolbarResult.Save: DrawSave(g, icon, pen); break;
            case ToolbarResult.CopyImage: DrawCopy(g, icon, pen); break;
            case ToolbarResult.CopyPath: DrawPath(g, icon, pen); break;
            case ToolbarResult.CopyText: DrawText(g, icon, pen); break;
            case ToolbarResult.Translate: DrawTranslate(g, icon, pen, color); break;
            default: DrawClose(g, icon, pen); break;
        }
    }

    private bool IsChecked(ToolbarResult kind) => kind switch
    {
        ToolbarResult.Shape => Tool is AnnotKind.Rect or AnnotKind.Ellipse,
        ToolbarResult.Stroke => Tool is AnnotKind.Line or AnnotKind.Arrow,
        ToolbarResult.Pencil => Tool == AnnotKind.Pencil,
        ToolbarResult.Marker => Tool == AnnotKind.Marker,
        ToolbarResult.Mosaic => Tool == AnnotKind.Mosaic,
        ToolbarResult.AnnotText => Tool == AnnotKind.Text,
        ToolbarResult.Eraser => Tool == AnnotKind.Eraser,
        _ => false
    };

    private void PaintTip(Graphics g, int i, float scale, float alpha)
    {
        var text = TipAt(i);
        if (text.Length == 0) return;
        var size = TextRenderer.MeasureText(text, Theme.UiSmall, new Size(int.MaxValue, int.MaxValue),
            TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
        int tw = size.Width + Sc(14, scale);
        int th = size.Height + Sc(8, scale);
        var btn = _buttons[i];
        int x = btn.X + (btn.Width - tw) / 2;
        int y = Bounds.Y - th - Sc(6, scale);
        if (y < 2) y = Bounds.Bottom + Sc(6, scale);
        var box = new RectangleF(x, y, tw, th);
        using var bg = new SolidBrush(Color.FromArgb((int)(230 * alpha), 10, 14, 20));
        FillRound(g, bg, box, Sc(4, scale));
        TextRenderer.DrawText(g, text, Theme.UiSmall, new Point(x + Sc(7, scale), y + Sc(4, scale)),
            Color.FromArgb((int)(255 * alpha), Theme.Text),
            TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding | TextFormatFlags.PreserveGraphicsClipping);
    }

    private static RectangleF IconBox(Rectangle btn)
    {
        float s = Math.Min(btn.Width, btn.Height) * 0.46f;
        return new RectangleF(btn.X + (btn.Width - s) / 2f, btn.Y + (btn.Height - s) / 2f, s, s);
    }

    private static void DrawArrowIcon(Graphics g, RectangleF r, Pen pen, Color fill)
    {
        var from = new PointF(r.X + r.Width * 0.08f, r.Bottom - r.Height * 0.08f);
        var to = new PointF(r.Right - r.Width * 0.06f, r.Y + r.Height * 0.08f);
        AnnotationSession.PaintArrow(g, from, to, fill, Math.Max(1.5f, pen.Width));
    }

    private static void DrawRectIcon(Graphics g, RectangleF r, Pen pen)
    {
        var box = RectangleF.Inflate(r, -r.Width * 0.08f, -r.Height * 0.12f);
        g.DrawRectangle(pen, box.X, box.Y, box.Width, box.Height);
    }

    private static void DrawEllipseIcon(Graphics g, RectangleF r, Pen pen)
    {
        var box = RectangleF.Inflate(r, -r.Width * 0.06f, -r.Height * 0.10f);
        g.DrawEllipse(pen, box);
    }

    private static void DrawLineIcon(Graphics g, RectangleF r, Pen pen)
    {
        g.DrawLine(pen, r.X + r.Width * 0.08f, r.Bottom - r.Height * 0.12f,
            r.Right - r.Width * 0.08f, r.Y + r.Height * 0.12f);
    }

    private static void DrawPencil(Graphics g, RectangleF r, Pen pen)
    {
        var body = new[]
        {
            new PointF(r.X + r.Width * 0.22f, r.Bottom - r.Height * 0.08f),
            new PointF(r.X + r.Width * 0.08f, r.Bottom - r.Height * 0.28f),
            new PointF(r.Right - r.Width * 0.28f, r.Y + r.Height * 0.12f),
            new PointF(r.Right - r.Width * 0.08f, r.Y + r.Height * 0.32f)
        };
        g.DrawPolygon(pen, body);
        g.DrawLine(pen, r.X + r.Width * 0.18f, r.Bottom - r.Height * 0.18f,
            r.X + r.Width * 0.32f, r.Bottom - r.Height * 0.08f);
    }

    private static void DrawMarker(Graphics g, RectangleF r, Pen pen, Color color)
    {
        using var fill = new SolidBrush(Color.FromArgb(90, color));
        using var thick = new Pen(Color.FromArgb(160, color), Math.Max(3.2f, pen.Width * 2.2f))
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round
        };
        g.DrawLine(thick, r.X + r.Width * 0.08f, r.Bottom - r.Height * 0.28f,
            r.Right - r.Width * 0.08f, r.Y + r.Height * 0.28f);
        g.DrawLine(pen, r.X + r.Width * 0.12f, r.Bottom - r.Height * 0.12f,
            r.Right - r.Width * 0.12f, r.Y + r.Height * 0.12f);
        _ = fill;
    }

    private static void DrawMosaic(Graphics g, RectangleF r, Pen pen, Color color)
    {
        float s = r.Width / 2.15f;
        using var a = new SolidBrush(Color.FromArgb(180, color));
        using var b = new SolidBrush(Color.FromArgb(70, color));
        g.FillRectangle(a, r.X, r.Y, s, s);
        g.FillRectangle(b, r.X + s, r.Y, s, s);
        g.FillRectangle(b, r.X, r.Y + s, s, s);
        g.FillRectangle(a, r.X + s, r.Y + s, s, s);
        g.DrawRectangle(pen, r.X, r.Y, s * 2, s * 2);
    }

    private static void DrawLetter(Graphics g, RectangleF r, Color color)
    {
        var hint = g.TextRenderingHint;
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
        float px = Math.Max(9f, r.Height * 0.78f);
        using var font = new Font("Segoe UI", px, FontStyle.Bold, GraphicsUnit.Pixel);
        using var brush = new SolidBrush(color);
        g.DrawString("T", font, brush, r.X + r.Width * 0.08f, r.Y - r.Height * 0.18f);
        g.TextRenderingHint = hint;
    }

    private static void DrawEraser(Graphics g, RectangleF r, Pen pen)
    {
        var body = new[]
        {
            new PointF(r.X + r.Width * 0.12f, r.Y + r.Height * 0.42f),
            new PointF(r.X + r.Width * 0.42f, r.Y + r.Height * 0.08f),
            new PointF(r.Right - r.Width * 0.08f, r.Y + r.Height * 0.38f),
            new PointF(r.Right - r.Width * 0.38f, r.Bottom - r.Height * 0.08f)
        };
        g.DrawPolygon(pen, body);
        g.DrawLine(pen, body[0], body[3]);
    }

    private static void DrawPin(Graphics g, RectangleF r, Pen pen)
    {
        var frame = new RectangleF(r.X + r.Width * 0.08f, r.Y + r.Height * 0.18f, r.Width * 0.84f, r.Height * 0.74f);
        DrawRound(g, pen, frame, r.Width * 0.1f);
        g.DrawLine(pen, frame.X + r.Width * 0.12f, frame.Y + r.Height * 0.42f,
            frame.X + r.Width * 0.36f, frame.Bottom - r.Height * 0.16f);
        g.DrawLine(pen, frame.X + r.Width * 0.36f, frame.Bottom - r.Height * 0.16f,
            frame.Right - r.Width * 0.12f, frame.Y + r.Height * 0.22f);
        var sun = new RectangleF(frame.Right - r.Width * 0.38f, frame.Y + r.Height * 0.12f,
            r.Width * 0.18f, r.Height * 0.18f);
        g.DrawEllipse(pen, sun);
    }

    private static void DrawScroll(Graphics g, RectangleF r, Pen pen)
    {
        float x = r.X + r.Width * 0.12f;
        float w = r.Width * 0.76f;
        g.DrawLine(pen, x, r.Y + r.Height * 0.12f, x + w, r.Y + r.Height * 0.12f);
        g.DrawLine(pen, x, r.Y + r.Height * 0.34f, x + w, r.Y + r.Height * 0.34f);
        float cx = r.X + r.Width / 2f;
        float top = r.Y + r.Height * 0.52f;
        float bot = r.Bottom - r.Height * 0.06f;
        g.DrawLine(pen, cx, top, cx, bot);
        g.DrawLine(pen, cx, bot, cx - r.Width * 0.22f, bot - r.Height * 0.22f);
        g.DrawLine(pen, cx, bot, cx + r.Width * 0.22f, bot - r.Height * 0.22f);
    }

    private static void DrawUndo(Graphics g, RectangleF r, Pen pen)
    {
        var box = new RectangleF(r.X + r.Width * 0.08f, r.Y + r.Height * 0.18f, r.Width * 0.84f, r.Height * 0.72f);
        g.DrawArc(pen, box, 40, 240);
        float cx = r.X + r.Width * 0.22f;
        float cy = r.Y + r.Height * 0.28f;
        g.DrawLine(pen, cx, cy, cx - r.Width * 0.18f, cy + r.Height * 0.02f);
        g.DrawLine(pen, cx, cy, cx + r.Width * 0.02f, cy + r.Height * 0.22f);
    }

    private static void DrawSave(Graphics g, RectangleF r, Pen pen)
    {
        float t = r.Top + r.Height * 0.08f;
        float mid = r.Top + r.Height * 0.52f;
        float cx = r.X + r.Width / 2f;
        g.DrawLine(pen, cx, t, cx, mid);
        g.DrawLine(pen, cx, mid, cx - r.Width * 0.28f, mid - r.Height * 0.22f);
        g.DrawLine(pen, cx, mid, cx + r.Width * 0.28f, mid - r.Height * 0.22f);
        var tray = new RectangleF(r.X + r.Width * 0.08f, r.Bottom - r.Height * 0.34f, r.Width * 0.84f, r.Height * 0.28f);
        g.DrawLine(pen, tray.X, tray.Y, tray.X, tray.Bottom);
        g.DrawLine(pen, tray.X, tray.Bottom, tray.Right, tray.Bottom);
        g.DrawLine(pen, tray.Right, tray.Bottom, tray.Right, tray.Y);
    }

    private static void DrawCopy(Graphics g, RectangleF r, Pen pen)
    {
        var back = new RectangleF(r.X, r.Y, r.Width * 0.68f, r.Height * 0.68f);
        var front = new RectangleF(r.X + r.Width * 0.32f, r.Y + r.Height * 0.32f, r.Width * 0.68f, r.Height * 0.68f);
        DrawRound(g, pen, back, r.Width * 0.12f);
        DrawRound(g, pen, front, r.Width * 0.12f);
    }

    private static void DrawPath(Graphics g, RectangleF r, Pen pen)
    {
        var page = new RectangleF(r.X + r.Width * 0.12f, r.Y, r.Width * 0.76f, r.Height);
        float fold = r.Width * 0.28f;
        g.DrawLines(pen, new[]
        {
            new PointF(page.Right, page.Y + fold),
            new PointF(page.Right, page.Bottom),
            new PointF(page.X, page.Bottom),
            new PointF(page.X, page.Y),
            new PointF(page.Right - fold, page.Y),
            new PointF(page.Right, page.Y + fold),
            new PointF(page.Right - fold, page.Y + fold),
            new PointF(page.Right - fold, page.Y)
        });
        float y1 = page.Y + r.Height * 0.52f;
        float y2 = page.Y + r.Height * 0.70f;
        g.DrawLine(pen, page.X + r.Width * 0.18f, y1, page.Right - r.Width * 0.18f, y1);
        g.DrawLine(pen, page.X + r.Width * 0.18f, y2, page.X + r.Width * 0.42f, y2);
    }

    private static void DrawTranslate(Graphics g, RectangleF r, Pen pen, Color color)
    {
        var hint = g.TextRenderingHint;
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
        float px = Math.Max(7f, r.Height * 0.48f);
        using var font = new Font("Segoe UI", px, FontStyle.Bold, GraphicsUnit.Pixel);
        using var brush = new SolidBrush(color);
        g.DrawString("A", font, brush, r.X - r.Width * 0.12f, r.Y - r.Height * 0.12f);
        g.DrawString("文", font, brush, r.X + r.Width * 0.28f, r.Y + r.Height * 0.18f);
        g.TextRenderingHint = hint;
        float y = r.Y + r.Height * 0.42f;
        g.DrawLine(pen, r.X + r.Width * 0.08f, y, r.Right - r.Width * 0.08f, y + r.Height * 0.22f);
    }

    private static void DrawText(Graphics g, RectangleF r, Pen pen)
    {
        var page = new RectangleF(r.X + r.Width * 0.10f, r.Y, r.Width * 0.80f, r.Height);
        DrawRound(g, pen, page, r.Width * 0.10f);
        float x1 = page.X + r.Width * 0.16f;
        float x2 = page.Right - r.Width * 0.16f;
        g.DrawLine(pen, x1, page.Y + r.Height * 0.28f, x2, page.Y + r.Height * 0.28f);
        g.DrawLine(pen, x1, page.Y + r.Height * 0.50f, x2, page.Y + r.Height * 0.50f);
        g.DrawLine(pen, x1, page.Y + r.Height * 0.72f, page.X + r.Width * 0.42f, page.Y + r.Height * 0.72f);
    }

    private static void DrawClose(Graphics g, RectangleF r, Pen pen)
    {
        float m = r.Width * 0.18f;
        g.DrawLine(pen, r.X + m, r.Y + m, r.Right - m, r.Bottom - m);
        g.DrawLine(pen, r.Right - m, r.Y + m, r.X + m, r.Bottom - m);
    }

    private static int Sc(int v, float s) => Math.Max(1, (int)Math.Round(v * s));

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

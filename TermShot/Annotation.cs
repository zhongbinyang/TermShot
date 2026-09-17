using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Drawing.Text;

namespace TermShot;

internal enum AnnotKind
{
    None = 0,
    Rect,
    Ellipse,
    Line,
    Arrow,
    Pencil,
    Marker,
    Mosaic,
    Text,
    Eraser
}

internal sealed class AnnotMark
{
    public AnnotKind Kind;
    public Color Color;
    public float Width;
    public PointF From;
    public PointF To;
    public List<PointF>? Path;
    public string? Text;
}

internal sealed class AnnotationSession : IDisposable
{
    public static readonly Color[] Colors =
    [
        Color.FromArgb(0xF2, 0x3B, 0x3B),
        Color.FromArgb(0xFF, 0x8C, 0x1A),
        Color.FromArgb(0xF5, 0xD0, 0x20),
        Color.FromArgb(0x2D, 0xE2, 0xA8),
        Color.FromArgb(0xF2, 0xF5, 0xF8),
        Color.FromArgb(0x1A, 0x1E, 0x24)
    ];

    public static readonly float[] Widths = [2.2f, 4f, 6.5f, 10f];
    public static readonly string[] WidthNames = ["细", "中", "粗", "特粗"];

    private readonly List<AnnotMark> _marks = [];
    private AnnotMark? _draft;
    private Bitmap? _source;
    private Bitmap? _layer;
    private AnnotKind _shapeKind = AnnotKind.Rect;
    private AnnotKind _strokeKind = AnnotKind.Arrow;

    public AnnotKind Tool { get; private set; } = AnnotKind.Arrow;
    public AnnotKind ShapeKind => _shapeKind;
    public AnnotKind StrokeKind => _strokeKind;
    public int ColorIndex { get; set; }
    public int WidthIndex { get; set; } = 1;
    public bool HasDraft => _draft != null;
    public bool HasMarks => _marks.Count > 0;
    public bool ToolActive => Tool != AnnotKind.None;
    public bool CanDraw => Tool is not AnnotKind.None and not AnnotKind.Text;
    public bool IsTextTool => Tool == AnnotKind.Text;
    public bool ShowPalette => ToolActive;
    public bool ShowColor => Tool is not AnnotKind.None and not AnnotKind.Mosaic and not AnnotKind.Eraser;
    public Color Color => Colors[Math.Clamp(ColorIndex, 0, Colors.Length - 1)];
    public float Width => Widths[Math.Clamp(WidthIndex, 0, Widths.Length - 1)];

    public void Attach(Bitmap source)
    {
        _source = source;
        RebuildLayer();
    }

    public void Select(AnnotKind kind)
    {
        CancelDraft();
        if (kind is AnnotKind.Rect or AnnotKind.Ellipse)
            _shapeKind = kind;
        if (kind is AnnotKind.Line or AnnotKind.Arrow)
            _strokeKind = kind;
        Tool = kind;
    }

    public void ToggleShape()
    {
        CancelDraft();
        Tool = Tool is AnnotKind.Rect or AnnotKind.Ellipse ? AnnotKind.None : _shapeKind;
    }

    public void ToggleStroke()
    {
        CancelDraft();
        Tool = Tool is AnnotKind.Line or AnnotKind.Arrow ? AnnotKind.None : _strokeKind;
    }

    public void Toggle(AnnotKind kind)
    {
        CancelDraft();
        if (kind is AnnotKind.Rect or AnnotKind.Ellipse)
        {
            _shapeKind = kind;
            Tool = Tool == kind ? AnnotKind.None : kind;
            return;
        }
        if (kind is AnnotKind.Line or AnnotKind.Arrow)
        {
            _strokeKind = kind;
            Tool = Tool == kind ? AnnotKind.None : kind;
            return;
        }
        Tool = Tool == kind ? AnnotKind.None : kind;
    }

    public void CycleTab()
    {
        CancelDraft();
        if (Tool is AnnotKind.Rect or AnnotKind.Ellipse)
        {
            Tool = Tool == AnnotKind.Rect ? AnnotKind.Ellipse : AnnotKind.Rect;
            _shapeKind = Tool;
        }
        else if (Tool is AnnotKind.Line or AnnotKind.Arrow)
        {
            Tool = Tool == AnnotKind.Line ? AnnotKind.Arrow : AnnotKind.Line;
            _strokeKind = Tool;
        }
    }

    public void Begin(PointF from)
    {
        if (!CanDraw) return;
        if (IsPath(Tool))
        {
            _draft = new AnnotMark
            {
                Kind = Tool,
                Color = Color,
                Width = Width,
                From = from,
                To = from,
                Path = [from]
            };
            return;
        }

        _draft = new AnnotMark
        {
            Kind = Tool,
            Color = Color,
            Width = Width,
            From = from,
            To = from
        };
    }

    public void Move(PointF to, bool snap45)
    {
        if (_draft is not { } d) return;
        if (d.Path != null)
        {
            var last = d.Path[^1];
            float dx = to.X - last.X, dy = to.Y - last.Y;
            if (dx * dx + dy * dy >= 2.2f)
                d.Path.Add(to);
            d.To = to;
            return;
        }

        d.To = Constrain(d.From, to, d.Kind, snap45);
    }

    public void CancelDraft() => _draft = null;

    public bool CommitDraft(float minLength = 8f)
    {
        if (_draft is not { } d)
            return false;
        _draft = null;

        if (d.Path != null)
        {
            if (d.Path.Count == 0)
                return false;
            _marks.Add(d);
            PaintMarkOnLayer(d);
            return true;
        }

        float dx = d.To.X - d.From.X, dy = d.To.Y - d.From.Y;
        if (dx * dx + dy * dy < minLength * minLength)
            return false;
        d.Color = Color;
        d.Width = Width;
        _marks.Add(d);
        PaintMarkOnLayer(d);
        return true;
    }

    public void AddText(PointF at, string text)
    {
        text = text.Trim();
        if (text.Length == 0) return;
        var mark = new AnnotMark
        {
            Kind = AnnotKind.Text,
            Color = Color,
            Width = Width,
            From = at,
            To = at,
            Text = text
        };
        _marks.Add(mark);
        PaintMarkOnLayer(mark);
    }

    public bool Undo()
    {
        if (_draft != null)
        {
            _draft = null;
            return true;
        }
        if (_marks.Count == 0)
            return false;
        _marks.RemoveAt(_marks.Count - 1);
        RebuildLayer();
        return true;
    }

    public void SetWidthIndex(int index)
    {
        WidthIndex = Math.Clamp(index, 0, Widths.Length - 1);
        if (_draft is { } d)
            d.Width = Width;
    }

    public void Paint(Graphics g, Rectangle srcRect, Rectangle destRect)
    {
        if (srcRect.Width < 1 || srcRect.Height < 1 || destRect.Width < 1 || destRect.Height < 1)
            return;

        if (_layer != null)
        {
            var old = g.InterpolationMode;
            g.InterpolationMode = InterpolationMode.NearestNeighbor;
            g.DrawImage(_layer, destRect, srcRect, GraphicsUnit.Pixel);
            g.InterpolationMode = old;
        }

        if (_draft is not { } d)
            return;

        float sx = destRect.Width / (float)srcRect.Width;
        float sy = destRect.Height / (float)srcRect.Height;
        float ws = (sx + sy) * 0.5f;
        PointF Map(PointF p) => new(
            destRect.X + (p.X - srcRect.X) * sx,
            destRect.Y + (p.Y - srcRect.Y) * sy);

        var state = g.Save();
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.PixelOffsetMode = PixelOffsetMode.HighQuality;
        PaintMark(g, d, Map, ws, preview: true);
        g.Restore(state);
    }

    public void Stamp(Bitmap dest, Point originInSource)
    {
        CommitDraft();
        if (_layer == null)
            return;
        using var g = Graphics.FromImage(dest);
        g.CompositingMode = CompositingMode.SourceOver;
        g.InterpolationMode = InterpolationMode.NearestNeighbor;
        var src = new Rectangle(originInSource.X, originInSource.Y, dest.Width, dest.Height);
        g.DrawImage(_layer, new Rectangle(0, 0, dest.Width, dest.Height), src, GraphicsUnit.Pixel);
    }

    public void Dispose()
    {
        _layer?.Dispose();
        _layer = null;
    }

    public static PointF Snap45(PointF from, PointF to)
    {
        float dx = to.X - from.X, dy = to.Y - from.Y;
        float len = MathF.Sqrt(dx * dx + dy * dy);
        if (len < 1) return to;
        float snapped = MathF.Round(MathF.Atan2(dy, dx) / (MathF.PI / 4f)) * (MathF.PI / 4f);
        return new PointF(from.X + MathF.Cos(snapped) * len, from.Y + MathF.Sin(snapped) * len);
    }

    public static PointF SnapSquare(PointF from, PointF to)
    {
        float dx = to.X - from.X, dy = to.Y - from.Y;
        float sx = dx < 0 ? -1f : 1f;
        float sy = dy < 0 ? -1f : 1f;
        float s = Math.Max(Math.Abs(dx), Math.Abs(dy));
        return new PointF(from.X + sx * s, from.Y + sy * s);
    }

    public static void PaintArrow(Graphics g, PointF from, PointF to, Color color, float width)
    {
        float dx = to.X - from.X, dy = to.Y - from.Y;
        float len = MathF.Sqrt(dx * dx + dy * dy);
        if (len < 2f) return;

        float ux = dx / len, uy = dy / len;
        float headLen = Math.Clamp(width * 3.4f, 9f, len * 0.72f);
        float headHalf = Math.Max(width * 1.55f, 4.5f);
        var back = new PointF(to.X - ux * headLen, to.Y - uy * headLen);
        var left = new PointF(back.X - uy * headHalf, back.Y + ux * headHalf);
        var right = new PointF(back.X + uy * headHalf, back.Y - ux * headHalf);
        var head = new[] { to, left, right };

        using (var halo = new Pen(Color.FromArgb(150, 0, 0, 0), width + 2.2f)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Flat,
            LineJoin = LineJoin.Round
        })
            g.DrawLine(halo, from, back);

        using (var pen = new Pen(color, width)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Flat,
            LineJoin = LineJoin.Round
        })
            g.DrawLine(pen, from, back);

        using (var outline = new SolidBrush(Color.FromArgb(150, 0, 0, 0)))
            g.FillPolygon(outline, Inflate(head, 1.1f));
        using var fill = new SolidBrush(color);
        g.FillPolygon(fill, head);
    }

    private static bool IsPath(AnnotKind kind) =>
        kind is AnnotKind.Pencil or AnnotKind.Marker or AnnotKind.Mosaic or AnnotKind.Eraser;

    private static PointF Constrain(PointF from, PointF to, AnnotKind kind, bool shift)
    {
        if (!shift) return to;
        return kind is AnnotKind.Rect or AnnotKind.Ellipse ? SnapSquare(from, to) : Snap45(from, to);
    }

    private void EnsureLayer()
    {
        if (_layer != null || _source == null)
            return;
        _layer = new Bitmap(_source.Width, _source.Height, PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(_layer);
        g.Clear(Color.Transparent);
    }

    private void RebuildLayer()
    {
        _layer?.Dispose();
        _layer = null;
        if (_source == null)
            return;
        EnsureLayer();
        if (_layer == null)
            return;
        using var g = Graphics.FromImage(_layer);
        g.Clear(Color.Transparent);
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.PixelOffsetMode = PixelOffsetMode.HighQuality;
        foreach (var mark in _marks)
            PaintMark(g, mark, p => p, 1f, preview: false);
    }

    private void PaintMarkOnLayer(AnnotMark mark)
    {
        EnsureLayer();
        if (_layer == null)
            return;
        using var g = Graphics.FromImage(_layer);
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.PixelOffsetMode = PixelOffsetMode.HighQuality;
        PaintMark(g, mark, p => p, 1f, preview: false);
    }

    private void PaintMark(Graphics g, AnnotMark m, Func<PointF, PointF> map, float widthScale, bool preview)
    {
        float w = Math.Max(1f, m.Width * widthScale);
        switch (m.Kind)
        {
            case AnnotKind.Arrow:
                PaintArrow(g, map(m.From), map(m.To), m.Color, w);
                break;
            case AnnotKind.Line:
                PaintStroke(g, map(m.From), map(m.To), m.Color, w);
                break;
            case AnnotKind.Rect:
                PaintRect(g, map(m.From), map(m.To), m.Color, w);
                break;
            case AnnotKind.Ellipse:
                PaintEllipse(g, map(m.From), map(m.To), m.Color, w);
                break;
            case AnnotKind.Pencil:
                PaintPath(g, m.Path, map, m.Color, w);
                break;
            case AnnotKind.Marker:
                var marker = Color.FromArgb(92, m.Color);
                PaintPath(g, m.Path, map, marker, w * 2.7f);
                break;
            case AnnotKind.Mosaic:
                PaintMosaic(g, m.Path, map, m.Width);
                break;
            case AnnotKind.Eraser:
                if (preview)
                    PaintEraserPreview(g, m.Path, map, w * 2.2f);
                else
                    PaintEraser(g, m.Path, w * 2.2f);
                break;
            case AnnotKind.Text:
                PaintText(g, map(m.From), m.Text, m.Color, w);
                break;
        }
    }

    private static void PaintStroke(Graphics g, PointF from, PointF to, Color color, float width)
    {
        using (var halo = new Pen(Color.FromArgb(140, 0, 0, 0), width + 2.2f)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round
        })
            g.DrawLine(halo, from, to);
        using var pen = new Pen(color, width)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round
        };
        g.DrawLine(pen, from, to);
    }

    private static RectangleF BoundsOf(PointF a, PointF b)
    {
        float x = Math.Min(a.X, b.X);
        float y = Math.Min(a.Y, b.Y);
        return new RectangleF(x, y, Math.Max(1f, Math.Abs(b.X - a.X)), Math.Max(1f, Math.Abs(b.Y - a.Y)));
    }

    private static void PaintRect(Graphics g, PointF a, PointF b, Color color, float width)
    {
        var r = BoundsOf(a, b);
        using (var halo = new Pen(Color.FromArgb(140, 0, 0, 0), width + 2f))
            g.DrawRectangle(halo, r.X, r.Y, r.Width, r.Height);
        using var pen = new Pen(color, width);
        g.DrawRectangle(pen, r.X, r.Y, r.Width, r.Height);
    }

    private static void PaintEllipse(Graphics g, PointF a, PointF b, Color color, float width)
    {
        var r = BoundsOf(a, b);
        using (var halo = new Pen(Color.FromArgb(140, 0, 0, 0), width + 2f))
            g.DrawEllipse(halo, r);
        using var pen = new Pen(color, width);
        g.DrawEllipse(pen, r);
    }

    private static void PaintPath(Graphics g, List<PointF>? path, Func<PointF, PointF> map, Color color, float width)
    {
        if (path == null || path.Count == 0) return;
        using var pen = new Pen(color, width)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round,
            LineJoin = LineJoin.Round
        };
        if (path.Count == 1)
        {
            var p = map(path[0]);
            float r = width / 2f;
            using var brush = new SolidBrush(color);
            g.FillEllipse(brush, p.X - r, p.Y - r, r * 2, r * 2);
            return;
        }

        var pts = new PointF[path.Count];
        for (int i = 0; i < path.Count; i++)
            pts[i] = map(path[i]);
        g.DrawLines(pen, pts);
    }

    private void PaintMosaic(Graphics g, List<PointF>? path, Func<PointF, PointF> map, float width)
    {
        if (path == null || path.Count == 0 || _source == null) return;
        int cell = Math.Max(6, (int)Math.Round(width * 3.2f));
        var seen = new HashSet<(int, int)>();
        foreach (var p in path)
        {
            int gx = (int)Math.Floor(p.X / cell) * cell;
            int gy = (int)Math.Floor(p.Y / cell) * cell;
            if (!seen.Add((gx, gy))) continue;
            var c = Sample(_source, gx + cell / 2, gy + cell / 2);
            var a = map(new PointF(gx, gy));
            var b = map(new PointF(gx + cell, gy + cell));
            float x = Math.Min(a.X, b.X);
            float y = Math.Min(a.Y, b.Y);
            using var brush = new SolidBrush(c);
            g.FillRectangle(brush, x, y, Math.Abs(b.X - a.X), Math.Abs(b.Y - a.Y));
        }
    }

    private static void PaintEraserPreview(Graphics g, List<PointF>? path, Func<PointF, PointF> map, float width)
    {
        if (path == null || path.Count == 0) return;
        using var brush = new SolidBrush(Color.FromArgb(70, 255, 255, 255));
        using var ring = new Pen(Color.FromArgb(160, 255, 255, 255), 1.2f);
        float r = width / 2f;
        foreach (var raw in path)
        {
            var p = map(raw);
            g.FillEllipse(brush, p.X - r, p.Y - r, r * 2, r * 2);
            g.DrawEllipse(ring, p.X - r, p.Y - r, r * 2, r * 2);
        }
    }

    private static void PaintEraser(Graphics g, List<PointF>? path, float width)
    {
        if (path == null || path.Count == 0) return;
        var old = g.CompositingMode;
        g.CompositingMode = CompositingMode.SourceCopy;
        using var clear = new SolidBrush(Color.FromArgb(0, 0, 0, 0));
        float r = width / 2f;
        if (path.Count == 1)
        {
            g.FillEllipse(clear, path[0].X - r, path[0].Y - r, r * 2, r * 2);
            g.CompositingMode = old;
            return;
        }

        using var gp = new GraphicsPath();
        using var widen = new Pen(Color.Black, width)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round,
            LineJoin = LineJoin.Round
        };
        gp.AddLines(path.ToArray());
        try
        {
            gp.Widen(widen);
            g.FillPath(clear, gp);
        }
        catch (OutOfMemoryException)
        {
            foreach (var p in path)
                g.FillEllipse(clear, p.X - r, p.Y - r, r * 2, r * 2);
        }
        g.CompositingMode = old;
    }

    private static void PaintText(Graphics g, PointF at, string? text, Color color, float width)
    {
        if (string.IsNullOrWhiteSpace(text)) return;
        float px = Math.Clamp(width * 4.2f, 12f, 64f);
        using var font = new Font("Segoe UI", px, FontStyle.Bold, GraphicsUnit.Pixel);
        g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
        using (var halo = new SolidBrush(Color.FromArgb(160, 0, 0, 0)))
            g.DrawString(text, font, halo, at.X + 1.2f, at.Y + 1.2f);
        using var fill = new SolidBrush(color);
        g.DrawString(text, font, fill, at.X, at.Y);
    }

    private static Color Sample(Bitmap bmp, int x, int y)
    {
        x = Math.Clamp(x, 0, bmp.Width - 1);
        y = Math.Clamp(y, 0, bmp.Height - 1);
        return bmp.GetPixel(x, y);
    }

    private static PointF[] Inflate(PointF[] tri, float px)
    {
        float cx = (tri[0].X + tri[1].X + tri[2].X) / 3f;
        float cy = (tri[0].Y + tri[1].Y + tri[2].Y) / 3f;
        var r = new PointF[3];
        for (int i = 0; i < 3; i++)
        {
            float dx = tri[i].X - cx, dy = tri[i].Y - cy;
            float d = MathF.Sqrt(dx * dx + dy * dy);
            if (d < 0.1f) { r[i] = tri[i]; continue; }
            r[i] = new PointF(tri[i].X + dx / d * px, tri[i].Y + dy / d * px);
        }
        return r;
    }
}

internal enum ToolbarResult
{
    Miss,
    Chrome,
    Shape,
    Stroke,
    Pencil,
    Marker,
    Mosaic,
    AnnotText,
    Eraser,
    Undo,
    Scroll,
    Pin,
    Save,
    CopyImage,
    CopyPath,
    CopyText,
    Translate,
    Close,
    Color,
    Width
}

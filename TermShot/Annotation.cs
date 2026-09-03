using System.Drawing.Drawing2D;

namespace TermShot;

internal readonly record struct ArrowMark(PointF From, PointF To, Color Color, float Width);

internal sealed class AnnotationSession
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

    private readonly List<ArrowMark> _arrows = [];

    public bool ArrowTool { get; set; } = true;
    public int ColorIndex { get; set; }
    public int WidthIndex { get; set; } = 1;
    public ArrowMark? Draft { get; private set; }
    public bool HasMarks => _arrows.Count > 0;
    public Color Color => Colors[Math.Clamp(ColorIndex, 0, Colors.Length - 1)];
    public float Width => Widths[Math.Clamp(WidthIndex, 0, Widths.Length - 1)];

    public void Begin(PointF from)
    {
        Draft = new ArrowMark(from, from, Color, Width);
    }

    public void Move(PointF to, bool snap45)
    {
        if (Draft is not { } d) return;
        Draft = d with { To = snap45 ? Snap45(d.From, to) : to };
    }

    public void CancelDraft() => Draft = null;

    public bool CommitDraft(float minLength = 8f)
    {
        if (Draft is not { } d)
            return false;
        Draft = null;
        float dx = d.To.X - d.From.X, dy = d.To.Y - d.From.Y;
        if (dx * dx + dy * dy < minLength * minLength)
            return false;
        _arrows.Add(d with { Color = Color, Width = Width });
        return true;
    }

    public bool Undo()
    {
        if (Draft != null)
        {
            Draft = null;
            return true;
        }
        if (_arrows.Count == 0)
            return false;
        _arrows.RemoveAt(_arrows.Count - 1);
        return true;
    }

    public void SetWidthIndex(int index)
    {
        WidthIndex = Math.Clamp(index, 0, Widths.Length - 1);
        if (Draft is { } d)
            Draft = d with { Width = Width };
    }

    public void Paint(Graphics g, Func<PointF, PointF> map, float widthScale)
    {
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.PixelOffsetMode = PixelOffsetMode.HighQuality;
        foreach (var a in _arrows)
            PaintArrow(g, map(a.From), map(a.To), a.Color, a.Width * widthScale);
        if (Draft is { } d)
            PaintArrow(g, map(d.From), map(d.To), d.Color, d.Width * widthScale);
    }

    public void Stamp(Bitmap dest, Point originInSource)
    {
        using var g = Graphics.FromImage(dest);
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.PixelOffsetMode = PixelOffsetMode.HighQuality;
        g.CompositingMode = CompositingMode.SourceOver;
        foreach (var a in _arrows)
        {
            var from = new PointF(a.From.X - originInSource.X, a.From.Y - originInSource.Y);
            var to = new PointF(a.To.X - originInSource.X, a.To.Y - originInSource.Y);
            PaintArrow(g, from, to, a.Color, a.Width);
        }
    }

    public static PointF Snap45(PointF from, PointF to)
    {
        float dx = to.X - from.X, dy = to.Y - from.Y;
        float len = MathF.Sqrt(dx * dx + dy * dy);
        if (len < 1) return to;
        float snapped = MathF.Round(MathF.Atan2(dy, dx) / (MathF.PI / 4f)) * (MathF.PI / 4f);
        return new PointF(from.X + MathF.Cos(snapped) * len, from.Y + MathF.Sin(snapped) * len);
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
        {
            var grow = Inflate(head, 1.1f);
            g.FillPolygon(outline, grow);
        }
        using var fill = new SolidBrush(color);
        g.FillPolygon(fill, head);
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
    Arrow,
    Undo,
    Scroll,
    Pin,
    Save,
    CopyImage,
    CopyPath,
    Close,
    Color,
    Width
}

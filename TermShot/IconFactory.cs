using System.Drawing.Drawing2D;
using System.Drawing.Imaging;

namespace TermShot;

internal static class IconFactory
{
    public static Icon Create()
    {
        using var bmp = Draw(32);
        IntPtr h = bmp.GetHicon();
        try
        {
            return (Icon)Icon.FromHandle(h).Clone();
        }
        finally
        {
            NativeMethods.DestroyIcon(h);
        }
    }

    public static void WriteIco(string path)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        using var bmp16 = Draw(16);
        using var bmp32 = Draw(32);
        using var bmp48 = Draw(48);
        using var bmp256 = Draw(256);
        SaveIco(path, bmp16, bmp32, bmp48, bmp256);
    }

    public static Bitmap Draw(int size)
    {
        var bmp = new Bitmap(size, size, PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(bmp);
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.PixelOffsetMode = PixelOffsetMode.HighQuality;
        g.Clear(Color.Transparent);

        float s = size / 32f;
        using var bg = new SolidBrush(Color.FromArgb(0x12, 0x18, 0x20));
        using var accent = new SolidBrush(Theme.Accent);
        using var accentPen = new Pen(Theme.Accent, Math.Max(1.2f, 2.2f * s))
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round
        };
        using var dimPen = new Pen(Color.FromArgb(0x3A, 0x4A, 0x58), Math.Max(1f, 1.6f * s));

        var frame = new RectangleF(3 * s, 4 * s, 26 * s, 22 * s);
        FillRound(g, bg, frame, 4 * s);
        g.DrawPath(dimPen, RoundPath(frame, 4 * s));

        float m = 7 * s;
        float t = 2.4f * s;
        using var corner = new Pen(Theme.Accent, Math.Max(1.4f, 2.1f * s))
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round
        };
        // selection corners
        g.DrawLines(corner, new[] { new PointF(frame.X + m, frame.Y + t), new PointF(frame.X + t, frame.Y + t), new PointF(frame.X + t, frame.Y + m) });
        g.DrawLines(corner, new[] { new PointF(frame.Right - m, frame.Y + t), new PointF(frame.Right - t, frame.Y + t), new PointF(frame.Right - t, frame.Y + m) });
        g.DrawLines(corner, new[] { new PointF(frame.X + m, frame.Bottom - t), new PointF(frame.X + t, frame.Bottom - t), new PointF(frame.X + t, frame.Bottom - m) });
        g.DrawLines(corner, new[] { new PointF(frame.Right - m, frame.Bottom - t), new PointF(frame.Right - t, frame.Bottom - t), new PointF(frame.Right - t, frame.Bottom - m) });

        float cx = 16 * s, cy = 15 * s;
        g.DrawLine(accentPen, cx - 5 * s, cy, cx + 5 * s, cy);
        g.DrawLine(accentPen, cx, cy - 5 * s, cx, cy + 5 * s);
        using var dot = new SolidBrush(Color.White);
        g.FillEllipse(dot, cx - 1.3f * s, cy - 1.3f * s, 2.6f * s, 2.6f * s);
        return bmp;
    }

    private static void SaveIco(string path, params Bitmap[] images)
    {
        using var fs = File.Create(path);
        using var bw = new BinaryWriter(fs);
        bw.Write((ushort)0);
        bw.Write((ushort)1);
        bw.Write((ushort)images.Length);

        var payloads = new byte[images.Length][];
        for (int i = 0; i < images.Length; i++)
        {
            using var ms = new MemoryStream();
            images[i].Save(ms, ImageFormat.Png);
            payloads[i] = ms.ToArray();
        }

        int offset = 6 + 16 * images.Length;
        for (int i = 0; i < images.Length; i++)
        {
            int w = images[i].Width;
            int h = images[i].Height;
            bw.Write((byte)(w >= 256 ? 0 : w));
            bw.Write((byte)(h >= 256 ? 0 : h));
            bw.Write((byte)0);
            bw.Write((byte)0);
            bw.Write((ushort)1);
            bw.Write((ushort)32);
            bw.Write(payloads[i].Length);
            bw.Write(offset);
            offset += payloads[i].Length;
        }

        foreach (var p in payloads)
            bw.Write(p);
    }

    private static void FillRound(Graphics g, Brush brush, RectangleF r, float radius)
    {
        using var path = RoundPath(r, radius);
        g.FillPath(brush, path);
    }

    private static GraphicsPath RoundPath(RectangleF r, float radius)
    {
        var path = new GraphicsPath();
        float d = Math.Max(1, radius * 2);
        path.AddArc(r.X, r.Y, d, d, 180, 90);
        path.AddArc(r.Right - d, r.Y, d, d, 270, 90);
        path.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
        path.AddArc(r.X, r.Bottom - d, d, d, 90, 90);
        path.CloseFigure();
        return path;
    }
}

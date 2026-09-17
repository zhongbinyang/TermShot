using System.Drawing.Imaging;

namespace TermShot;

internal static class VisionImage
{
    public const int MaxEdge = 1600;

    public static string EncodePngBase64(Bitmap source)
    {
        using var work = Shrink(source, MaxEdge);
        using var ms = new MemoryStream();
        work.Save(ms, ImageFormat.Png);
        return Convert.ToBase64String(ms.ToArray());
    }

    public static string EncodePngDataUrl(Bitmap source) =>
        "data:image/png;base64," + EncodePngBase64(source);

    private static Bitmap Shrink(Bitmap source, int maxEdge)
    {
        int w = source.Width, h = source.Height;
        int edge = Math.Max(w, h);
        if (edge <= maxEdge)
            return (Bitmap)source.Clone();

        float scale = maxEdge / (float)edge;
        int nw = Math.Max(1, (int)Math.Round(w * scale));
        int nh = Math.Max(1, (int)Math.Round(h * scale));
        var scaled = new Bitmap(nw, nh, PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(scaled);
        g.InterpolationMode = System.Drawing.Drawing2D.InterpolationMode.HighQualityBicubic;
        g.PixelOffsetMode = System.Drawing.Drawing2D.PixelOffsetMode.HighQuality;
        g.DrawImage(source, new Rectangle(0, 0, nw, nh));
        return scaled;
    }
}

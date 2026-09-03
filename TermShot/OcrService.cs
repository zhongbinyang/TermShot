using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;
using Windows.Globalization;
using Windows.Graphics.Imaging;
using Windows.Media.Ocr;
using Windows.Storage.Streams;

namespace TermShot;

internal readonly record struct OcrGlyph(int Index, int LineIndex, string Text, RectangleF Bounds);

internal sealed class OcrPage
{
    public IReadOnlyList<OcrGlyph> Glyphs { get; }
    public string FullText { get; }

    public OcrPage(IReadOnlyList<OcrGlyph> glyphs, string fullText)
    {
        Glyphs = glyphs;
        FullText = fullText;
    }

    public bool HasText => Glyphs.Count > 0;

    public string Slice(int from, int to)
    {
        if (Glyphs.Count == 0) return "";
        int a = Math.Clamp(Math.Min(from, to), 0, Glyphs.Count - 1);
        int b = Math.Clamp(Math.Max(from, to), 0, Glyphs.Count - 1);
        var sb = new StringBuilder();
        int lastLine = -1;
        string? prev = null;
        for (int i = a; i <= b; i++)
        {
            var g = Glyphs[i];
            if (lastLine >= 0 && g.LineIndex != lastLine)
                sb.AppendLine();
            else if (prev != null && !Tight(prev, g.Text))
                sb.Append(' ');
            sb.Append(g.Text);
            lastLine = g.LineIndex;
            prev = g.Text;
        }
        return sb.ToString();
    }

    public int LineStart(int index)
    {
        if (index < 0 || index >= Glyphs.Count) return index;
        int line = Glyphs[index].LineIndex;
        int i = index;
        while (i > 0 && Glyphs[i - 1].LineIndex == line) i--;
        return i;
    }

    public int LineEnd(int index)
    {
        if (index < 0 || index >= Glyphs.Count) return index;
        int line = Glyphs[index].LineIndex;
        int i = index;
        while (i + 1 < Glyphs.Count && Glyphs[i + 1].LineIndex == line) i++;
        return i;
    }

    private static bool Tight(string left, string right)
    {
        if (left.Length == 0 || right.Length == 0) return true;
        return IsCjk(left[^1]) || IsCjk(right[0]) || char.IsPunctuation(right[0]);
    }

    private static bool IsCjk(char c) =>
        c is >= '\u4E00' and <= '\u9FFF'
            or >= '\u3400' and <= '\u4DBF'
            or >= '\u3040' and <= '\u30FF'
            or >= '\uAC00' and <= '\uD7AF';
}

internal static class OcrService
{
    public static async Task<OcrPage?> RecognizeAsync(Bitmap source, CancellationToken ct)
    {
        var engine = CreateEngine();
        if (engine is null)
            return null;

        using var work = Prepare(source, out float scale);
        ct.ThrowIfCancellationRequested();
        using var software = ToSoftwareBitmap(work);
        ct.ThrowIfCancellationRequested();

        var result = await engine.RecognizeAsync(software).AsTask(ct).ConfigureAwait(false);
        var glyphs = new List<OcrGlyph>();
        int lineIndex = 0;
        foreach (var line in result.Lines)
        {
            foreach (var word in line.Words)
            {
                var r = word.BoundingRect;
                var bounds = new RectangleF(
                    (float)(r.X / scale),
                    (float)(r.Y / scale),
                    (float)(r.Width / scale),
                    (float)(r.Height / scale));
                if (bounds.Width < 0.5f || bounds.Height < 0.5f)
                    continue;
                if (string.IsNullOrWhiteSpace(word.Text))
                    continue;
                glyphs.Add(new OcrGlyph(glyphs.Count, lineIndex, word.Text, bounds));
            }
            lineIndex++;
        }

        return new OcrPage(glyphs, result.Text?.Trim() ?? "");
    }

    public static bool EngineAvailable() => CreateEngine() != null;

    private static OcrEngine? CreateEngine()
    {
        var profile = OcrEngine.TryCreateFromUserProfileLanguages();
        if (profile != null) return profile;

        foreach (var tag in new[] { "zh-Hans", "zh-CN", "zh-Hant", "zh-TW", "en-US", "en" })
        {
            var lang = new Language(tag);
            if (!OcrEngine.IsLanguageSupported(lang))
                continue;
            var engine = OcrEngine.TryCreateFromLanguage(lang);
            if (engine != null) return engine;
        }

        foreach (var lang in OcrEngine.AvailableRecognizerLanguages)
        {
            var engine = OcrEngine.TryCreateFromLanguage(lang);
            if (engine != null) return engine;
        }

        return null;
    }

    private static Bitmap Prepare(Bitmap source, out float scale)
    {
        scale = 1f;
        int max = (int)Math.Max(32, OcrEngine.MaxImageDimension);
        int w = source.Width, h = source.Height;
        if (w <= max && h <= max)
            return (Bitmap)source.Clone();

        scale = Math.Min(max / (float)w, max / (float)h);
        int nw = Math.Max(1, (int)Math.Round(w * scale));
        int nh = Math.Max(1, (int)Math.Round(h * scale));
        var scaled = new Bitmap(nw, nh, PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(scaled);
        g.InterpolationMode = System.Drawing.Drawing2D.InterpolationMode.HighQualityBicubic;
        g.DrawImage(source, new Rectangle(0, 0, nw, nh));
        return scaled;
    }

    private static SoftwareBitmap ToSoftwareBitmap(Bitmap source)
    {
        using var conv = source.PixelFormat == PixelFormat.Format32bppArgb
            ? null
            : new Bitmap(source.Width, source.Height, PixelFormat.Format32bppArgb);
        var bmp = conv ?? source;
        if (conv != null)
        {
            using var g = Graphics.FromImage(conv);
            g.DrawImage(source, 0, 0, source.Width, source.Height);
        }

        var data = bmp.LockBits(new Rectangle(0, 0, bmp.Width, bmp.Height),
            ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        try
        {
            int dstStride = bmp.Width * 4;
            var packed = new byte[dstStride * bmp.Height];
            int srcStride = data.Stride;
            if (srcStride == dstStride)
            {
                Marshal.Copy(data.Scan0, packed, 0, packed.Length);
            }
            else
            {
                for (int y = 0; y < bmp.Height; y++)
                    Marshal.Copy(IntPtr.Add(data.Scan0, y * srcStride), packed, y * dstStride, dstStride);
            }

            var software = new SoftwareBitmap(
                BitmapPixelFormat.Bgra8, bmp.Width, bmp.Height, BitmapAlphaMode.Premultiplied);
            using var writer = new DataWriter();
            writer.WriteBytes(packed);
            software.CopyFromBuffer(writer.DetachBuffer());
            return software;
        }
        finally
        {
            bmp.UnlockBits(data);
        }
    }
}

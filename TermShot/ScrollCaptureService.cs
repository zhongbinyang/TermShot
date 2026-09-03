using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

namespace TermShot;

internal sealed class ScrollStitcher : IDisposable
{
    public const int MaxHeight = 32000;

    private Bitmap? _canvas;
    private Bitmap? _last;
    private Bitmap? _pending;
    private int _footer;

    public int Height => _canvas?.Height ?? 0;
    public int Width => _canvas?.Width ?? 0;

    public void Begin(Bitmap first)
    {
        DisposeFrames();
        _canvas = Clone32(first);
        _last = Clone32(first);
        _footer = 0;
    }

    public bool Append(Bitmap frame)
    {
        if (_canvas is null || _last is null)
            return false;
        if (frame.Width != _last.Width || frame.Height != _last.Height)
            return false;
        if (_canvas.Height >= MaxHeight)
            return false;

        var incoming = Copy(frame);
        var committed = Copy(_last);
        if (AlmostEqual(incoming, committed))
            return false;

        if (_pending is null)
        {
            _pending = Clone32(frame);
            return false;
        }

        var pending = Copy(_pending);
        if (!AlmostEqual(incoming, pending))
        {
            _pending.Dispose();
            _pending = Clone32(frame);
            return false;
        }

        _pending.Dispose();
        _pending = null;

        int skipTop = DetectSticky(committed, incoming, fromTop: true);
        int skipBot = DetectSticky(committed, incoming, fromTop: false);
        int dy = FindContentDelta(committed, incoming, skipTop, skipBot);
        if (dy <= 0)
            return false;

        if (_footer == 0)
            _footer = skipBot;

        int add = Math.Min(dy, MaxHeight - _canvas.Height);
        if (add <= 0)
            return false;

        int srcY = frame.Height - _footer - add;
        if (srcY < skipTop)
            srcY = skipTop;
        add = Math.Min(add, frame.Height - _footer - srcY);
        if (add <= 0)
            return false;

        int bodyH = Math.Max(0, _canvas.Height - _footer);
        var grown = new Bitmap(_canvas.Width, bodyH + add + _footer, PixelFormat.Format32bppArgb);
        CopyRows(grown, 0, _canvas, 0, bodyH);
        CopyRows(grown, bodyH, frame, srcY, add);
        if (_footer > 0)
            CopyRows(grown, bodyH + add, frame, frame.Height - _footer, _footer);

        _canvas.Dispose();
        _canvas = grown;
        _last.Dispose();
        _last = Clone32(frame);
        return true;
    }

    public Bitmap TakeBitmap()
    {
        var bmp = _canvas ?? throw new InvalidOperationException("尚未开始拼接。");
        _canvas = null;
        return bmp;
    }

    public void Dispose() => DisposeFrames();

    private void DisposeFrames()
    {
        _canvas?.Dispose();
        _last?.Dispose();
        _pending?.Dispose();
        _canvas = null;
        _last = null;
        _pending = null;
    }

    private static void CopyRows(Bitmap dest, int destY, Bitmap src, int srcY, int rows)
    {
        if (rows <= 0) return;
        var srcRect = new Rectangle(0, srcY, src.Width, rows);
        var dstRect = new Rectangle(0, destY, dest.Width, rows);
        var sd = src.LockBits(srcRect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        var dd = dest.LockBits(dstRect, ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
        try
        {
            int bytes = src.Width * 4;
            var row = new byte[bytes];
            for (int y = 0; y < rows; y++)
            {
                Marshal.Copy(sd.Scan0 + y * sd.Stride, row, 0, bytes);
                Marshal.Copy(row, 0, dd.Scan0 + y * dd.Stride, bytes);
            }
        }
        finally
        {
            src.UnlockBits(sd);
            dest.UnlockBits(dd);
        }
    }

    private static Bitmap Clone32(Bitmap src)
    {
        var bmp = new Bitmap(src.Width, src.Height, PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(bmp);
        g.CompositingMode = CompositingMode.SourceCopy;
        g.DrawImageUnscaled(src, 0, 0);
        return bmp;
    }

    private static int DetectSticky(Pix a, Pix b, bool fromTop)
    {
        int h = a.H;
        int w = a.W;
        int max = Math.Max(4, h / 4);
        int run = 0;
        if (fromTop)
        {
            for (int y = 0; y < max; y++)
            {
                if (!RowClose(a, y, b, y, w))
                    break;
                run++;
            }
        }
        else
        {
            for (int y = h - 1; y >= h - max; y--)
            {
                if (!RowClose(a, y, b, y, w))
                    break;
                run++;
            }
        }
        return run >= 4 ? run : 0;
    }

    private static int FindContentDelta(Pix a, Pix b, int skipTop, int skipBot)
    {
        int h = a.H;
        int w = a.W;
        int top = skipTop;
        int bot = h - skipBot;
        int contentH = bot - top;
        if (contentH < 24)
            return 0;

        int minOverlap = Math.Max(16, contentH / 4);
        int maxDy = contentH - minOverlap;
        if (maxDy < 1)
            return 0;

        int step = Math.Max(1, w / 80);
        int cols = (w + step - 1) / step;
        long thresh = cols * 20L;

        long bestNorm = long.MaxValue;
        int bestDy = 0;
        for (int dy = 1; dy <= maxDy; dy++)
        {
            int rows = contentH - dy;
            long norm = Diff(a, top + dy, b, top, rows, w) / rows;
            if (norm < bestNorm)
            {
                bestNorm = norm;
                bestDy = dy;
            }
        }

        if (bestDy <= 0 || bestNorm > thresh)
            return 0;

        long allow = bestNorm * 115 / 100 + 3;
        for (int dy = 1; dy < bestDy; dy++)
        {
            int rows = contentH - dy;
            long norm = Diff(a, top + dy, b, top, rows, w) / rows;
            if (norm <= allow)
                return dy;
        }
        return bestDy;
    }

    private readonly record struct Pix(byte[] Buf, int Stride, int W, int H);

    private static Pix Copy(Bitmap bmp)
    {
        var rect = new Rectangle(0, 0, bmp.Width, bmp.Height);
        var data = bmp.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        try
        {
            int stride = Math.Abs(data.Stride);
            var buf = new byte[stride * bmp.Height];
            Marshal.Copy(data.Scan0, buf, 0, buf.Length);
            return new Pix(buf, stride, bmp.Width, bmp.Height);
        }
        finally
        {
            bmp.UnlockBits(data);
        }
    }

    private static bool AlmostEqual(Pix a, Pix b)
    {
        if (a.W != b.W || a.H != b.H)
            return false;
        int stepY = Math.Max(1, a.H / 80);
        int stepX = Math.Max(1, a.W / 80);
        long sum = 0;
        int n = 0;
        for (int y = 0; y < a.H; y += stepY)
        {
            int oa = y * a.Stride;
            int ob = y * b.Stride;
            for (int x = 0; x < a.W; x += stepX)
            {
                int ia = oa + x * 4;
                int ib = ob + x * 4;
                sum += Math.Abs(a.Buf[ia] - b.Buf[ib])
                     + Math.Abs(a.Buf[ia + 1] - b.Buf[ib + 1])
                     + Math.Abs(a.Buf[ia + 2] - b.Buf[ib + 2]);
                n++;
            }
        }
        return n > 0 && sum / n < 8;
    }

    private static bool RowClose(Pix a, int ay, Pix b, int by, int width)
    {
        int step = Math.Max(1, width / 80);
        int cols = (width + step - 1) / step;
        return Diff(a, ay, b, by, 1, width) < cols * 12L;
    }

    private static long Diff(Pix a, int ay, Pix b, int by, int rows, int width)
    {
        int step = Math.Max(1, width / 80);
        long sum = 0;
        for (int row = 0; row < rows; row++)
        {
            int oa = (ay + row) * a.Stride;
            int ob = (by + row) * b.Stride;
            for (int x = 0; x < width; x += step)
            {
                int ia = oa + x * 4;
                int ib = ob + x * 4;
                sum += Math.Abs(a.Buf[ia] - b.Buf[ib])
                     + Math.Abs(a.Buf[ia + 1] - b.Buf[ib + 1])
                     + Math.Abs(a.Buf[ia + 2] - b.Buf[ib + 2]);
            }
        }
        return sum;
    }
}

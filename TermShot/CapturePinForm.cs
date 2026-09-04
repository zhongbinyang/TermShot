using System.Drawing.Drawing2D;

namespace TermShot;

internal sealed class CapturePinForm : Form
{
    private readonly Bitmap _bmp;
    private readonly Rectangle _screenRect;
    private readonly ActionToolbar _toolbar = new();
    private readonly AnnotationSession _ann = new();
    private Rectangle _imageClient;
    private float _scale = 1f;

    public PostCaptureAction? Chosen { get; private set; }

    public CapturePinForm(Bitmap bmp, Rectangle screenRect)
    {
        _bmp = bmp;
        _screenRect = screenRect;
        AutoScaleMode = AutoScaleMode.None;
        FormBorderStyle = FormBorderStyle.None;
        ShowInTaskbar = false;
        TopMost = true;
        KeyPreview = true;
        StartPosition = FormStartPosition.Manual;
        BackColor = Color.FromArgb(0x12, 0x16, 0x1C);
        DoubleBuffered = true;
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer, true);
        Cursor = Cursors.Cross;
        Bounds = screenRect;
        KeyDown += OnKeyDown;
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
        _ann.WidthIndex = 1;
        LayoutPin(_screenRect);
    }

    protected override void OnShown(EventArgs e)
    {
        base.OnShown(e);
        LayoutPin(_screenRect);
        NativeMethods.SetWindowPos(Handle, new IntPtr(NativeMethods.HWND_TOPMOST),
            Left, Top, Width, Height, NativeMethods.SWP_SHOWWINDOW);
        Activate();
        Focus();
    }

    private void LayoutPin(Rectangle screenRect)
    {
        var working = Screen.FromPoint(screenRect.Location).WorkingArea;
        _scale = DeviceDpi > 0 ? DeviceDpi / 96f : 1f;
        int toolH = (int)Math.Round(76 * _scale);
        int gap = (int)Math.Round(8 * _scale);
        int margin = 8;

        int maxW = Math.Max(120, working.Width - margin * 2);
        int maxH = Math.Max(80, working.Height - margin * 2 - toolH - gap);
        float fit = Math.Min(1f, Math.Min(maxW / (float)_bmp.Width, maxH / (float)_bmp.Height));
        int imgW = Math.Max(1, (int)Math.Round(_bmp.Width * fit));
        int imgH = Math.Max(1, (int)Math.Round(_bmp.Height * fit));

        int toolW = (int)Math.Round(420 * _scale);
        int formW = Math.Max(imgW, toolW);
        bool outside = screenRect.Y + imgH + gap + toolH <= working.Bottom - margin;
        int formH = outside ? imgH + gap + toolH : imgH;

        int x = screenRect.X;
        int y = screenRect.Y;
        if (x + formW > working.Right - margin) x = working.Right - formW - margin;
        if (y + formH > working.Bottom - margin) y = working.Bottom - formH - margin;
        x = Math.Max(working.Left + margin, x);
        y = Math.Max(working.Top + margin, y);

        int inset = 1;
        formW += inset * 2;
        formH += inset;
        Bounds = new Rectangle(x, y, formW, formH);
        _imageClient = new Rectangle((formW - imgW) / 2, inset, imgW, imgH);
        SyncToolbar();
        _toolbar.Relayout(_imageClient, new Rectangle(0, 0, formW, formH), _scale);
    }

    private void SyncToolbar()
    {
        _toolbar.ArrowActive = _ann.ArrowTool;
        _toolbar.UndoEnabled = _ann.HasMarks || _ann.Draft != null;
        _toolbar.ColorIndex = _ann.ColorIndex;
        _toolbar.WidthIndex = _ann.WidthIndex;
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        g.Clear(BackColor);
        g.InterpolationMode = _imageClient.Width == _bmp.Width
            ? InterpolationMode.NearestNeighbor
            : InterpolationMode.HighQualityBicubic;
        g.PixelOffsetMode = PixelOffsetMode.Half;
        g.DrawImage(_bmp, _imageClient);
        g.SmoothingMode = SmoothingMode.None;
        g.PixelOffsetMode = PixelOffsetMode.None;
        using (var border = new Pen(Theme.Accent, 1f) { Alignment = PenAlignment.Inset })
            g.DrawRectangle(border, _imageClient.X, _imageClient.Y, _imageClient.Width - 1, _imageClient.Height - 1);

        float sx = _imageClient.Width / (float)Math.Max(1, _bmp.Width);
        float sy = _imageClient.Height / (float)Math.Max(1, _bmp.Height);
        var state = g.Save();
        g.SetClip(_imageClient);
        _ann.Paint(g, p => new PointF(_imageClient.X + p.X * sx, _imageClient.Y + p.Y * sy), sx);
        g.Restore(state);

        _scale = DeviceDpi / 96f;
        SyncToolbar();
        _toolbar.Relayout(_imageClient, ClientRectangle, _scale);
        _toolbar.Paint(g, _scale, 1f);
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        if (_ann.Draft != null)
        {
            _ann.Move(ClientToBmp(e.Location), snap45: ModifierKeys.HasFlag(Keys.Shift));
            Invalidate();
            return;
        }
        bool dirty = _toolbar.SetHover(e.Location);
        Cursor = CursorFor(e.Location);
        if (dirty) Invalidate();
        base.OnMouseMove(e);
    }

    protected override void OnMouseDown(MouseEventArgs e)
    {
        if (e.Button == MouseButtons.Right)
        {
            if (_ann.Draft != null)
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
                _toolbar.PressedIndex = _toolbar.HitTest(e.Location);
                if (_toolbar.PressedIndex >= 0)
                    Invalidate();
                return;
            }
            if (_ann.ArrowTool && _imageClient.Contains(e.Location))
            {
                _ann.Begin(ClientToBmp(e.Location));
                Cursor = Cursors.Cross;
                Invalidate();
            }
        }
        base.OnMouseDown(e);
    }

    protected override void OnMouseUp(MouseEventArgs e)
    {
        if (e.Button == MouseButtons.Left)
        {
            if (_ann.Draft != null)
            {
                _ann.Move(ClientToBmp(e.Location), snap45: ModifierKeys.HasFlag(Keys.Shift));
                _ann.CommitDraft();
                Invalidate();
                return;
            }
            int hit = _toolbar.HitTest(e.Location);
            int pressed = _toolbar.PressedIndex;
            _toolbar.PressedIndex = -1;
            if (pressed >= 0 && hit == pressed)
                Apply(_toolbar.ResultAt(pressed));
            else
                Invalidate();
        }
        base.OnMouseUp(e);
    }

    protected override void OnMouseLeave(EventArgs e)
    {
        if (_toolbar.SetHover(new Point(-1, -1)))
            Invalidate();
        base.OnMouseLeave(e);
    }

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        e.SuppressKeyPress = true;
        if (e.KeyCode == Keys.Escape)
        {
            if (_ann.Draft != null)
            {
                _ann.CancelDraft();
                Invalidate();
                return;
            }
            Cancel();
        }
        else if (e.KeyCode == Keys.A)
        {
            _ann.ArrowTool = !_ann.ArrowTool;
            Invalidate();
        }
        else if (e.KeyCode == Keys.Z)
        {
            if (_ann.Undo()) Invalidate();
        }
        else if (e.KeyCode is Keys.D1 or Keys.D2 or Keys.D3 or Keys.D4)
        {
            _ann.SetWidthIndex(e.KeyCode - Keys.D1);
            Invalidate();
        }
        else if (e.KeyCode == Keys.T)
            Choose(PostCaptureAction.Pin);
        else if (e.KeyCode == Keys.S)
            Choose(PostCaptureAction.SaveImage);
        else if (e.KeyCode == Keys.C)
            Choose(PostCaptureAction.CopyImage);
        else if (e.KeyCode == Keys.P)
            Choose(PostCaptureAction.CopyPath);
        else if (e.KeyCode == Keys.O)
            Choose(PostCaptureAction.CopyText);
        else if (e.KeyCode == Keys.L)
            Choose(PostCaptureAction.Translate);
    }

    private Cursor CursorFor(Point client)
    {
        var hit = _toolbar.Hit(client);
        if (hit is ToolbarResult.Color or ToolbarResult.Width || _toolbar.HoverIndex >= 0)
            return Cursors.Hand;
        if (hit is ToolbarResult.Chrome)
            return Cursors.Default;
        if (_ann.ArrowTool && _imageClient.Contains(client))
            return Cursors.Cross;
        return Cursors.Default;
    }

    private PointF ClientToBmp(Point client)
    {
        float x = (client.X - _imageClient.X) * _bmp.Width / (float)Math.Max(1, _imageClient.Width);
        float y = (client.Y - _imageClient.Y) * _bmp.Height / (float)Math.Max(1, _imageClient.Height);
        return new PointF(
            Math.Clamp(x, 0, _bmp.Width - 1),
            Math.Clamp(y, 0, _bmp.Height - 1));
    }

    private void Apply(ToolbarResult result)
    {
        switch (result)
        {
            case ToolbarResult.Arrow:
                _ann.ArrowTool = !_ann.ArrowTool;
                Invalidate();
                break;
            case ToolbarResult.Undo:
                if (_ann.Undo()) Invalidate();
                break;
            case ToolbarResult.Close:
                Cancel();
                break;
            default:
                if (ActionToolbar.ToAction(result) is { } action)
                    Choose(action);
                break;
        }
    }

    private void Choose(PostCaptureAction action)
    {
        _ann.CommitDraft();
        _ann.Stamp(_bmp, Point.Empty);
        Chosen = action;
        DialogResult = DialogResult.OK;
        Close();
    }

    private void Cancel()
    {
        Chosen = null;
        DialogResult = DialogResult.Cancel;
        Close();
    }
}

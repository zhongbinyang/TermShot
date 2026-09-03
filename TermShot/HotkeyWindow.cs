namespace TermShot;

internal sealed class HotkeyWindow : NativeWindow, IDisposable
{
    public const int CaptureId = 0x7E05;
    public event Action? CapturePressed;

    public HotkeyWindow()
    {
        CreateHandle(new CreateParams
        {
            Caption = "TermShotHotkeySink"
        });
    }

    public bool RegisterCapture(uint modifiers, Keys key)
    {
        Unregister();
        if (key == Keys.None) return false;
        return NativeMethods.RegisterHotKey(Handle, CaptureId, modifiers | NativeMethods.MOD_NOREPEAT, (uint)key);
    }

    public void Unregister()
    {
        if (Handle != IntPtr.Zero)
            NativeMethods.UnregisterHotKey(Handle, CaptureId);
    }

    protected override void WndProc(ref Message m)
    {
        if (m.Msg == NativeMethods.WM_HOTKEY && m.WParam.ToInt32() == CaptureId)
            CapturePressed?.Invoke();
        base.WndProc(ref m);
    }

    public void Dispose()
    {
        Unregister();
        if (Handle != IntPtr.Zero)
            DestroyHandle();
    }
}

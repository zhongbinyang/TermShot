namespace TermShot;

internal sealed class TrayContext : ApplicationContext
{
    private readonly NotifyIcon _tray;
    private readonly HotkeyWindow _hotkey = new();
    private readonly Icon _icon;
    private readonly List<PastePinForm> _pins = [];
    private readonly ToolStripMenuItem _closePinsItem;
    private AppSettings _settings;
    private SettingsForm? _settingsForm;
    private bool _busy;

    public TrayContext()
    {
        _settings = AppSettings.Load();
        _icon = IconFactory.Create();

        var menu = new ContextMenuStrip
        {
            Renderer = new DarkMenuRenderer(),
            Font = Theme.Ui,
            ShowImageMargin = false
        };
        menu.Items.Add(Item("截图", () => Queue(StartRegionCapture)));
        _closePinsItem = Item("关闭全部贴图", CloseAllPins);
        menu.Items.Add(_closePinsItem);
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(Item("打开保存目录", OpenSaveDir));
        menu.Items.Add(Item("设置", OpenSettings));
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(Item("退出", ExitApp));
        StyleMenu(menu);
        menu.Opening += (_, _) => _closePinsItem.Enabled = _pins.Count > 0;

        _tray = new NotifyIcon
        {
            Icon = _icon,
            Visible = true,
            Text = TrayTip(),
            ContextMenuStrip = menu
        };
        _tray.MouseClick += (_, e) =>
        {
            if (e.Button == MouseButtons.Left)
                Queue(StartRegionCapture);
        };

        _hotkey.CapturePressed += () => Queue(OnHotkey);
        ApplyHotkey();
        WarmupCursors();
        var firstRun = !File.Exists(AppSettings.FilePath);
        if (_settings.StartWithWindows)
        {
            try { StartupService.Apply(true); }
            catch { /* portable copy without registry is still usable */ }
        }
        if (firstRun)
        {
            _settings.Save();
            Balloon("终端截图已驻留托盘", "左键或 " + _settings.FormatHotkey() + " 框选，截完后选择保存、复制或贴图");
        }
    }

    private static void WarmupCursors()
    {
        try
        {
            _ = Cursors.Cross.Handle;
            _ = Cursors.Hand.Handle;
            _ = Cursors.Default.Handle;
            NativeMethods.LoadCursor(IntPtr.Zero, NativeMethods.IDC_CROSS);
        }
        catch { }
    }

    private static void Queue(Action action)
    {
        var sc = SynchronizationContext.Current;
        if (sc is null)
        {
            action();
            return;
        }
        sc.Post(_ => action(), null);
    }

    private static void ReadyCrosshair()
    {
        Application.UseWaitCursor = false;
        Cursor.Current = Cursors.Cross;
        NativeMethods.ApplyCursor(Cursors.Cross);
    }

    private void OnHotkey() => StartRegionCapture();

    public void StartRegionCapture() => _ = CaptureRegionAsync(scrollAfterSelect: false);

    private async Task CaptureRegionAsync(bool scrollAfterSelect)
    {
        if (_busy) return;
        _busy = true;
        try
        {
            ReadyCrosshair();
            DismissTrayChrome();
            var prev = NativeMethods.GetForegroundWindow();
            using var shot = CaptureService.CaptureVirtualScreen();
            var windows = WindowEnumerator.Snapshot(IntPtr.Zero);
            using var overlay = new OverlayForm(shot, windows, prev,
                askAfterSelect: !scrollAfterSelect && _settings.PostCaptureAction == PostCaptureAction.Ask,
                scrollAfterSelect: scrollAfterSelect);
            var result = overlay.ShowDialog();
            if (result != DialogResult.OK || overlay.SelectedScreenRect is not { } rect)
                return;

            if (overlay.ScrollCapture)
            {
                await RunScrollCaptureAsync(rect, prev);
                return;
            }

            var vs = NativeMethods.GetVirtualScreen();
            var bmpRect = new Rectangle(rect.X - vs.X, rect.Y - vs.Y, rect.Width, rect.Height);
            using var crop = CaptureService.Crop(shot, bmpRect);
            overlay.Annotations.Stamp(crop, bmpRect.Location);
            Finish(crop, rect, overlay.ChosenAction);
        }
        catch (Exception ex)
        {
            Balloon("截图失败", ex.Message);
        }
        finally
        {
            _busy = false;
        }
    }

    private async Task RunScrollCaptureAsync(Rectangle region, IntPtr foreground)
    {
        var vs = NativeMethods.GetVirtualScreen();
        region.Intersect(vs);
        if (region.Width < 8 || region.Height < 8)
            throw new InvalidOperationException("滚动区域太小。");

        if (foreground != IntPtr.Zero && NativeMethods.IsWindow(foreground))
            NativeMethods.SetForegroundWindow(foreground);

        ScrollHudForm? hud = null;
        ScrollStitcher? session = null;
        try
        {
            hud = new ScrollHudForm(region);
            hud.Show();
            await Task.Delay(160);

            using var first = CaptureService.CaptureRect(region);
            session = new ScrollStitcher();
            session.Begin(first);
            hud.SetHeight(session.Height);

            while (NativeMethods.IsKeyDown(Keys.Enter) || NativeMethods.IsKeyDown(Keys.Escape)
                   || NativeMethods.IsKeyDown(Keys.R))
                await Task.Delay(20);

            while (hud is { IsDisposed: false } && hud.Decision is null)
            {
                if (NativeMethods.IsKeyDown(Keys.Escape) || hud.Decision == ScrollHudDecision.Cancel)
                    return;
                if (NativeMethods.IsKeyDown(Keys.Enter) || hud.Decision == ScrollHudDecision.Finish)
                    break;
                if (session.Height >= ScrollStitcher.MaxHeight)
                    break;

                Application.DoEvents();

                if (hud.IsDisposed)
                    break;
                if (hud.Decision == ScrollHudDecision.Cancel)
                    return;
                if (hud.Decision == ScrollHudDecision.Finish)
                    break;
                if (NativeMethods.IsKeyDown(Keys.Escape))
                    return;
                if (NativeMethods.IsKeyDown(Keys.Enter))
                    break;

                using var frame = CaptureService.CaptureRect(region);
                if (session.Append(frame))
                    hud.SetHeight(session.Height);

                await Task.Delay(80);
            }

            if (hud is { IsDisposed: false })
            {
                hud.Close();
                hud.Dispose();
                hud = null;
            }

            using var result = session.TakeBitmap();
            session = null;
            Finish(result, region);
        }
        finally
        {
            if (hud is { IsDisposed: false })
                hud.Close();
            hud?.Dispose();
            session?.Dispose();
        }
    }

    private void DismissTrayChrome()
    {
        if (_tray.ContextMenuStrip is { Visible: true } menu)
        {
            menu.Close();
            Application.DoEvents();
            Thread.Sleep(150);
        }
    }

    private void Finish(Bitmap bmp, Rectangle screenRect, PostCaptureAction? chosen = null)
    {
        var action = chosen ?? _settings.PostCaptureAction;
        if (action == PostCaptureAction.Ask)
        {
            using var pin = new CapturePinForm(bmp, screenRect);
            if (pin.ShowDialog() != DialogResult.OK || pin.Chosen is not { } picked)
                return;
            action = picked;
        }

        switch (action)
        {
            case PostCaptureAction.SaveImage:
                SaveImage(bmp);
                break;
            case PostCaptureAction.CopyImage:
                CopyImage(bmp);
                break;
            case PostCaptureAction.Pin:
                PinBitmap(bmp, screenRect.Location);
                break;
            case PostCaptureAction.CopyText:
                CopyText(bmp);
                break;
            case PostCaptureAction.Translate:
                Translate(bmp);
                break;
            default:
                SaveAndCopyPath(bmp);
                break;
        }
    }

    private void SaveImage(Bitmap bmp)
    {
        if (!CaptureService.TrySavePng(bmp, _settings, out var path, out var err))
        {
            Balloon("保存失败", err ?? "未知错误");
            return;
        }
        Balloon("已保存", path);
    }

    private void CopyImage(Bitmap bmp)
    {
        if (!CaptureService.TrySetClipboardImage(bmp))
            Balloon("复制图片失败", "剪贴板正被占用，请再截一次");
        else
            Balloon("已复制图片", "可直接粘贴到聊天或文档");
    }

    private void CopyText(Bitmap bmp)
    {
        var clone = new Bitmap(bmp);
        var settings = _settings;
        var ui = SynchronizationContext.Current;
        Balloon("正在识别文字", settings.OllamaOcr
            ? "Ollama · " + OllamaClient.ModelName(settings)
            : "系统 OCR");

        _ = Task.Run(async () =>
        {
            string? text = null;
            string? error = null;
            try
            {
                var page = await OcrService.RecognizeAsync(clone, settings, CancellationToken.None)
                    .ConfigureAwait(false);
                text = page?.FullText?.Trim();
                if (string.IsNullOrWhiteSpace(text) && page is { HasText: true })
                    text = page.Slice(0, page.Glyphs.Count - 1).Trim();
            }
            catch (Exception ex)
            {
                error = ex.Message;
            }
            finally
            {
                clone.Dispose();
            }

            void Done()
            {
                if (error != null)
                    Balloon("识别失败", error);
                else if (string.IsNullOrWhiteSpace(text))
                    Balloon("未识别到文字", settings.OllamaOcr
                        ? "确认 ollama serve 已在 " + OllamaClient.NormalizeHost(settings.OllamaHost) + " 运行"
                        : "系统 OCR 没有读到字");
                else if (!CaptureService.TrySetClipboardText(text))
                    Balloon("复制文字失败", "剪贴板正被占用，请再试一次");
                else
                    Balloon("已复制文字", text);
            }

            if (ui != null)
                ui.Post(_ => Done(), null);
            else
                Done();
        });
    }

    private void Translate(Bitmap bmp)
    {
        var clone = new Bitmap(bmp);
        var settings = _settings;
        var ui = SynchronizationContext.Current;
        Balloon("正在翻译", "Ollama · " + OllamaClient.ModelName(settings)
            + " · " + TranslateService.TargetLabel(settings.TranslateTarget));

        _ = Task.Run(async () =>
        {
            string? text = null;
            string? error = null;
            try
            {
                text = await TranslateService.TranslateAsync(clone, settings, CancellationToken.None)
                    .ConfigureAwait(false);
            }
            catch (Exception ex)
            {
                error = ex.Message;
            }
            finally
            {
                clone.Dispose();
            }

            void Done()
            {
                if (error != null)
                    Balloon("翻译失败", error);
                else if (string.IsNullOrWhiteSpace(text))
                    Balloon("没有译出文字", "确认 ollama serve 已在 "
                        + OllamaClient.NormalizeHost(settings.OllamaHost) + " 运行");
                else if (!CaptureService.TrySetClipboardText(text))
                    Balloon("复制译文失败", "剪贴板正被占用，请再试一次");
                else
                    Balloon("已复制译文", text);
            }

            if (ui != null)
                ui.Post(_ => Done(), null);
            else
                Done();
        });
    }

    private void PinBitmap(Bitmap bmp, Point screenLocation)
    {
        PinOwned(new Bitmap(bmp), screenLocation);
    }

    private void PinOwned(Bitmap bmp, Point screenLocation)
    {
        var pin = new PastePinForm(bmp, screenLocation, _settings);
        pin.FormClosed += (_, _) => _pins.Remove(pin);
        _pins.Add(pin);
        pin.Show();
    }

    private void CloseAllPins()
    {
        foreach (var pin in _pins.ToArray())
            pin.Close();
    }

    private void SaveAndCopyPath(Bitmap bmp)
    {
        if (!CaptureService.TrySavePng(bmp, _settings, out var path, out var err))
        {
            Balloon("保存失败，剪贴板未改动", err ?? "未知错误");
            return;
        }

        if (!CaptureService.TrySetClipboardPath(path, _settings.QuotePath))
            Balloon("已保存，但写入剪贴板失败", path);
        else
            Balloon("已复制路径", _settings.QuotePath ? $"\"{path}\"" : path);
    }

    private void OpenSaveDir()
    {
        try
        {
            var dir = _settings.ResolvedSaveDirectory;
            Directory.CreateDirectory(dir);
            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
            {
                FileName = dir,
                UseShellExecute = true
            });
        }
        catch (Exception ex)
        {
            Balloon("无法打开目录", ex.Message);
        }
    }

    private void OpenSettings()
    {
        if (_settingsForm is { IsDisposed: false })
        {
            _settingsForm.BringToFront();
            _settingsForm.Activate();
            return;
        }

        _settingsForm = new SettingsForm(_settings, () => _hotkey.Unregister(), ApplySettings);
        _settingsForm.FormClosed += (_, _) =>
        {
            ApplyHotkey();
            _settingsForm = null;
        };
        _settingsForm.Show();
    }

    private void ApplySettings(AppSettings settings)
    {
        _settings = settings;
        _tray.Text = TrayTip();
        ApplyHotkey();
        try { StartupService.Apply(_settings.StartWithWindows); }
        catch (Exception ex) { Balloon("开机启动设置失败", ex.Message); }
    }

    private void ApplyHotkey()
    {
        if (!_hotkey.RegisterCapture(_settings.HotkeyModifiers, _settings.HotkeyKeys))
            Balloon("快捷键注册失败", _settings.FormatHotkey() + " 可能已被占用");
    }

    private string TrayTip() => "终端截图  " + _settings.FormatHotkey();

    private void ExitApp()
    {
        CloseAllPins();
        _tray.Visible = false;
        _hotkey.Dispose();
        _icon.Dispose();
        _tray.Dispose();
        ExitThread();
    }

    private void Balloon(string title, string text)
    {
        try
        {
            Toast.Show(title, text);
        }
        catch
        {
            try { _tray.ShowBalloonTip(1600, title, text, ToolTipIcon.None); }
            catch { }
        }
    }

    private static ToolStripMenuItem Item(string text, Action action)
    {
        var item = new ToolStripMenuItem(text);
        item.Click += (_, _) => action();
        item.ForeColor = Theme.Text;
        item.BackColor = Theme.WinBg;
        return item;
    }

    private static void StyleMenu(ContextMenuStrip menu)
    {
        menu.BackColor = Theme.WinBg;
        menu.ForeColor = Theme.Text;
        menu.Padding = new Padding(4, 4, 4, 4);
        menu.Opening += (_, _) =>
        {
            foreach (ToolStripItem it in menu.Items)
            {
                it.ForeColor = Theme.Text;
                it.BackColor = Theme.WinBg;
            }
        };
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            Toast.Close();
            CloseAllPins();
            _hotkey.Dispose();
            _tray.Dispose();
            _icon.Dispose();
        }
        base.Dispose(disposing);
    }
}

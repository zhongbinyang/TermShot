namespace TermShot;

internal sealed class SetupForm : Form
{
    private readonly CheckBox _startup;
    private readonly Label _status;
    private readonly Button _install;
    private readonly Label _path;

    public SetupForm()
    {
        AutoScaleMode = AutoScaleMode.Dpi;
        Text = "安装终端截图";
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false;
        MinimizeBox = false;
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(440, 268);
        BackColor = Theme.WinBg;
        ForeColor = Theme.Text;
        Font = Theme.Ui;
        ShowInTaskbar = true;
        try { Icon = IconFactory.Create(); } catch { }

        Controls.Add(new Label
        {
            Text = "终端截图",
            Left = 28,
            Top = 22,
            AutoSize = true,
            Font = new Font("Segoe UI", 16f, FontStyle.Bold),
            ForeColor = Theme.Text
        });
        Controls.Add(new Label
        {
            Text = "托盘常驻，框选存 PNG。无需管理员，安装到当前用户。",
            Left = 30,
            Top = 58,
            Width = 380,
            Height = 36,
            ForeColor = Theme.Dim
        });

        _path = new Label
        {
            Left = 30,
            Top = 102,
            Width = 380,
            Height = 36,
            ForeColor = Theme.Dim,
            Text = "位置  " + InstallService.InstallDir
        };
        Controls.Add(_path);

        _startup = new CheckBox
        {
            Text = "开机启动（默认开启）",
            Left = 30,
            Top = 142,
            AutoSize = true,
            Checked = true,
            ForeColor = Theme.Text,
            FlatStyle = FlatStyle.Flat
        };
        _startup.FlatAppearance.BorderColor = Theme.Border;
        Controls.Add(_startup);

        _status = new Label
        {
            Left = 30,
            Top = 176,
            Width = 380,
            Height = 22,
            ForeColor = Theme.Accent,
            Text = "快捷键默认 Ctrl+Shift+S，路径会进剪贴板。"
        };
        Controls.Add(_status);

        _install = MakeButton("安装", 250, 214, 88, true);
        var close = MakeButton("取消", 346, 214, 88, false);
        _install.Click += (_, _) => RunInstall();
        close.Click += (_, _) => Close();
        CancelButton = close;
        AcceptButton = _install;
        Controls.Add(_install);
        Controls.Add(close);
    }

    private void RunInstall()
    {
        _install.Enabled = false;
        _startup.Enabled = false;
        _status.ForeColor = Theme.Dim;
        _status.Text = "正在安装…";
        Refresh();
        try
        {
            InstallService.Install(launch: true, startWithWindows: _startup.Checked);
            _status.ForeColor = Theme.Accent;
            _status.Text = "已安装，正在启动托盘。";
            Refresh();
            Close();
        }
        catch (Exception ex)
        {
            _status.ForeColor = Theme.Danger;
            _status.Text = "安装失败：" + ex.Message;
            _install.Enabled = true;
            _startup.Enabled = true;
        }
    }

    private static Button MakeButton(string text, int x, int y, int w, bool accent)
    {
        var b = new Button
        {
            Text = text,
            Left = x,
            Top = y,
            Width = w,
            Height = 30,
            FlatStyle = FlatStyle.Flat,
            ForeColor = accent ? Color.FromArgb(0x06, 0x12, 0x10) : Theme.Text,
            BackColor = accent ? Theme.Accent : Theme.Panel,
            Font = Theme.UiBold,
            Cursor = Cursors.Hand
        };
        b.FlatAppearance.BorderColor = accent ? Theme.Accent : Theme.Border;
        return b;
    }
}

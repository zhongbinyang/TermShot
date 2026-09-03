using System.Threading;

namespace TermShot;

internal static class Program
{
    private const string MutexName = @"Local\TermShot.SingleInstance";

    [STAThread]
    private static void Main(string[] args)
    {
        Application.SetHighDpiMode(HighDpiMode.PerMonitorV2);
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        Application.SetUnhandledExceptionMode(UnhandledExceptionMode.CatchException);
        Application.ThreadException += (_, _) => { };
        AppDomain.CurrentDomain.UnhandledException += (_, _) => { };

        if (Has(args, "--write-icon"))
        {
            var dest = args.SkipWhile(a => a != "--write-icon").Skip(1).FirstOrDefault()
                       ?? Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "Assets", "app.ico");
            IconFactory.WriteIco(Path.GetFullPath(dest));
            return;
        }

        if (Has(args, "--uninstall"))
        {
            InstallService.Uninstall();
            return;
        }

        if (IsSetupLaunch(args))
        {
            if (Has(args, "--silent"))
            {
                InstallService.Install(launch: true, startWithWindows: !Has(args, "--no-startup"));
                return;
            }
            Application.Run(new SetupForm());
            return;
        }

        using var mutex = new Mutex(true, MutexName, out bool created);
        if (!created)
            return;

        Application.Run(new TrayContext());
        GC.KeepAlive(mutex);
    }

    private static bool Has(string[] args, string flag) =>
        args.Any(a => string.Equals(a, flag, StringComparison.OrdinalIgnoreCase));

    private static bool IsSetupLaunch(string[] args)
    {
        if (Has(args, "--install") || Has(args, "--setup"))
            return true;
        var name = Path.GetFileNameWithoutExtension(Environment.ProcessPath ?? "");
        return name.Contains("Setup", StringComparison.OrdinalIgnoreCase)
               || name.Contains("Install", StringComparison.OrdinalIgnoreCase);
    }
}

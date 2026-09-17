using System.Text.Json;
using System.Text.RegularExpressions;

namespace TermShot;

internal static class VisionClient
{
    private static readonly Regex ConversationalNoisePrefix = new(
        @"^(?:" +
        @"(?:好的[，,。]?\s*)?(?:我)?(?:可以|已)?(?:查看|看|看到)(?:此|这张|该)?(?:屏幕)?截图[。！!：:\s]*|" +
        @"(?:根据|在)(?:此|这张|该)?(?:屏幕)?截图(?:中|里)?(?:可见文字|内容)?[，,。：:\s]*|" +
        @"(?:以下|这里)是(?:翻译|转录|识别)?(?:结果|文本|内容)?[，,。：:\s]*|" +
        @"(?:Sure[!,.]?\s*)?(?:Here|Below) is the (?:translation|text|transcription)[，,.:\s]*|" +
        @"Translation:\s*" +
        @")",
        RegexOptions.IgnoreCase | RegexOptions.Compiled);

    private static readonly Regex TrailingNoiseSuffix = new(
        @"(?:\r?\n|\s)+(?:[β\u03b2\u3137]+.*|(?:Transcribe|Translate)\s+all\s+visible.*)$",
        RegexOptions.IgnoreCase | RegexOptions.Singleline | RegexOptions.Compiled);

    private static readonly Regex HyphenWrapRegex = new(
        @"([A-Za-z]{2,})-\n\s*([A-Za-z]{2,})",
        RegexOptions.Compiled);

    private static readonly Regex TrailingSpacesPerLineRegex = new(
        @"[ \t]+(?=\n)",
        RegexOptions.Compiled);

    public static Task<string?> GenerateAsync(
        Bitmap source, AppSettings settings, string prompt, CancellationToken ct, string? system = null) =>
        DeepSeekClient.GenerateVisionAsync(source, settings, prompt, ct, system);

    public static string ModelName(AppSettings settings) => DeepSeekClient.ModelName(settings);

    public static string StatusLine(AppSettings settings) =>
        "DeepSeek · " + ModelName(settings);

    public static string EmptyHint() =>
        "确认已在设置中填写有效的 DeepSeek API Key，且网络可访问 api.deepseek.com";

    public static string? CleanText(string? raw, params string[] jsonKeys)
    {
        if (string.IsNullOrWhiteSpace(raw))
            return null;
        var text = raw.Trim();
        text = text.Replace("\r\n", "\n").Replace('\r', '\n');
        if (text.StartsWith("```", StringComparison.Ordinal))
        {
            int nl = text.IndexOf('\n');
            if (nl > 0)
                text = text[(nl + 1)..];
            if (text.EndsWith("```", StringComparison.Ordinal))
                text = text[..^3];
            text = text.Trim();
        }

        if (jsonKeys.Length > 0 && text.Length >= 2 && text[0] == '{')
        {
            try
            {
                using var doc = JsonDocument.Parse(text);
                foreach (var key in jsonKeys)
                {
                    if (!doc.RootElement.TryGetProperty(key, out var node))
                        continue;
                    var extracted = node.GetString();
                    if (!string.IsNullOrWhiteSpace(extracted))
                    {
                        text = extracted.Trim();
                        break;
                    }
                }
            }
            catch (JsonException)
            {
                // keep stripped transcript
            }
        }

        text = StripConversationalNoise(text);
        text = TrailingNoiseSuffix.Replace(text, "").Trim();
        text = HyphenWrapRegex.Replace(text, "$1$2");
        text = TrailingSpacesPerLineRegex.Replace(text, "");
        text = Regex.Replace(text, @"\n{3,}", "\n\n");
        text = text.Replace("\n", "\r\n").Trim();

        return string.IsNullOrWhiteSpace(text) ? null : text;
    }

    private static string StripConversationalNoise(string text)
    {
        while (true)
        {
            var m = ConversationalNoisePrefix.Match(text);
            if (m.Success && m.Length > 0)
                text = text[m.Length..].Trim();
            else
                break;
        }
        return text;
    }
}

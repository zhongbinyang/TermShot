using System.Drawing.Imaging;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;

namespace TermShot;

internal static class OllamaClient
{
    public const int MaxEdge = 1600;

    private static readonly HttpClient Http = new()
    {
        Timeout = TimeSpan.FromMinutes(3)
    };

    private static readonly JsonSerializerOptions JsonOpts = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };

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

    public static string NormalizeHost(string? host)
    {
        var h = (host ?? "").Trim();
        if (h.Length == 0)
            h = AppSettings.DefaultOllamaHost;
        if (!h.StartsWith("http://", StringComparison.OrdinalIgnoreCase)
            && !h.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
            h = "http://" + h;
        return h.TrimEnd('/');
    }

    public static string ModelName(AppSettings settings) =>
        string.IsNullOrWhiteSpace(settings.OllamaModel)
            ? AppSettings.DefaultOllamaModel
            : settings.OllamaModel.Trim();

    public static async Task<string?> GenerateVisionAsync(
        Bitmap source, AppSettings settings, string prompt, CancellationToken ct, string? system = null)
    {
        var host = NormalizeHost(settings.OllamaHost);
        var model = ModelName(settings);
        var image = EncodePng(source);

        var payload = new OllamaGenerateRequest
        {
            Model = model,
            Prompt = prompt,
            System = system,
            Images = [image],
            Stream = false,
            Think = false,
            Options = new OllamaOptions { Temperature = 0 }
        };

        using var req = new HttpRequestMessage(HttpMethod.Post, host + "/api/generate")
        {
            Content = new StringContent(JsonSerializer.Serialize(payload, JsonOpts), Encoding.UTF8, "application/json")
        };
        using var resp = await Http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, ct)
            .ConfigureAwait(false);
        var body = await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
        if (!resp.IsSuccessStatusCode)
            throw new HttpRequestException($"Ollama {(int)resp.StatusCode}: {TrimErr(body)}");

        var parsed = JsonSerializer.Deserialize<OllamaGenerateResponse>(body, JsonOpts);
        return parsed?.Response;
    }

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

        // 修复跨行英文单词被连字符截断问题 (e.g. "inter-\nnational" -> "international")
        text = HyphenWrapRegex.Replace(text, "$1$2");

        // 去除每行尾随空格
        text = TrailingSpacesPerLineRegex.Replace(text, "");

        // 压缩多余的连续空白行（超过2个换行压缩为双换行）
        text = Regex.Replace(text, @"\n{3,}", "\n\n");

        // 规范化为 Windows 标准换行符 \r\n，避免不同应用粘贴错乱
        text = text.Replace("\n", "\r\n").Trim();

        return string.IsNullOrWhiteSpace(text) ? null : text;
    }

    private static string StripConversationalNoise(string text)
    {
        while (true)
        {
            var m = ConversationalNoisePrefix.Match(text);
            if (m.Success && m.Length > 0)
            {
                text = text[m.Length..].Trim();
            }
            else
            {
                break;
            }
        }
        return text;
    }

    private static string EncodePng(Bitmap source)
    {
        using var work = Shrink(source, MaxEdge);
        using var ms = new MemoryStream();
        work.Save(ms, ImageFormat.Png);
        return Convert.ToBase64String(ms.ToArray());
    }

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

    private static string TrimErr(string body)
    {
        body = body.Trim();
        if (body.Length > 240)
            body = body[..240] + "…";
        return string.IsNullOrWhiteSpace(body) ? "empty response" : body;
    }

    private sealed class OllamaGenerateRequest
    {
        public string Model { get; set; } = "";
        public string Prompt { get; set; } = "";
        public string? System { get; set; }
        public string[] Images { get; set; } = [];
        public bool Stream { get; set; }
        public bool Think { get; set; }
        public OllamaOptions? Options { get; set; }
    }

    private sealed class OllamaOptions
    {
        public float Temperature { get; set; }
    }

    private sealed class OllamaGenerateResponse
    {
        public string? Response { get; set; }
    }
}

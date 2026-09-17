using System.Net.Http;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace TermShot;

internal static class DeepSeekClient
{
    public const string ChatCompletionsUrl = "https://api.deepseek.com/chat/completions";

    private static readonly HttpClient Http = new()
    {
        Timeout = TimeSpan.FromMinutes(3)
    };

    public static string ModelName(AppSettings settings) =>
        string.IsNullOrWhiteSpace(settings.DeepSeekModel)
            ? AppSettings.DefaultDeepSeekModel
            : settings.DeepSeekModel.Trim();

    public static async Task<string?> GenerateVisionAsync(
        Bitmap source, AppSettings settings, string prompt, CancellationToken ct, string? system = null)
    {
        var apiKey = (settings.DeepSeekApiKey ?? "").Trim();
        if (apiKey.Length == 0)
            throw new InvalidOperationException("请先在设置中填写 DeepSeek API Key");

        var model = ModelName(settings);
        var dataUrl = VisionImage.EncodePngDataUrl(source);
        var payload = BuildRequest(model, prompt, dataUrl, system);

        using var req = new HttpRequestMessage(HttpMethod.Post, ChatCompletionsUrl)
        {
            Content = new StringContent(payload.ToJsonString(), Encoding.UTF8, "application/json")
        };
        req.Headers.Authorization = new AuthenticationHeaderValue("Bearer", apiKey);

        using var resp = await Http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, ct)
            .ConfigureAwait(false);
        var body = await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
        if (!resp.IsSuccessStatusCode)
            throw new HttpRequestException($"DeepSeek {(int)resp.StatusCode}: {TrimErr(body)}");

        return ParseContent(body);
    }

    private static JsonObject BuildRequest(string model, string prompt, string dataUrl, string? system)
    {
        var messages = new JsonArray();
        if (!string.IsNullOrWhiteSpace(system))
        {
            messages.Add(new JsonObject
            {
                ["role"] = "system",
                ["content"] = system
            });
        }

        messages.Add(new JsonObject
        {
            ["role"] = "user",
            ["content"] = new JsonArray
            {
                new JsonObject
                {
                    ["type"] = "text",
                    ["text"] = prompt
                },
                new JsonObject
                {
                    ["type"] = "image_url",
                    ["image_url"] = new JsonObject
                    {
                        ["url"] = dataUrl,
                        ["detail"] = "high"
                    }
                }
            }
        });

        return new JsonObject
        {
            ["model"] = model,
            ["stream"] = false,
            ["temperature"] = 0,
            ["messages"] = messages
        };
    }

    private static string? ParseContent(string body)
    {
        using var doc = JsonDocument.Parse(body);
        if (!doc.RootElement.TryGetProperty("choices", out var choices)
            || choices.ValueKind != JsonValueKind.Array
            || choices.GetArrayLength() == 0)
            return null;

        var first = choices[0];
        if (!first.TryGetProperty("message", out var message))
            return null;
        if (!message.TryGetProperty("content", out var content)
            || content.ValueKind != JsonValueKind.String)
            return null;

        var text = content.GetString();
        return string.IsNullOrWhiteSpace(text) ? null : text;
    }

    private static string TrimErr(string body)
    {
        body = body.Trim();
        try
        {
            using var doc = JsonDocument.Parse(body);
            if (doc.RootElement.TryGetProperty("error", out var error))
            {
                if (error.ValueKind == JsonValueKind.Object
                    && error.TryGetProperty("message", out var message)
                    && message.ValueKind == JsonValueKind.String)
                {
                    var msg = message.GetString();
                    if (!string.IsNullOrWhiteSpace(msg))
                        body = msg;
                }
                else if (error.ValueKind == JsonValueKind.String)
                {
                    var msg = error.GetString();
                    if (!string.IsNullOrWhiteSpace(msg))
                        body = msg;
                }
            }
        }
        catch (JsonException)
        {
            // keep raw body
        }

        if (body.Length > 240)
            body = body[..240] + "…";
        return string.IsNullOrWhiteSpace(body) ? "empty response" : body;
    }
}

namespace TermShot;

internal static class TranslateService
{
    private const string SystemInstruction =
        "You are an expert translation engine. " +
        "Read all visible text in the image, infer its language(s), and choose the target language yourself. " +
        "Do not ask which language to use. Do not mention the source or target language in the output.\n" +
        "How to choose:\n" +
        "- If the text is primarily Chinese, translate into English.\n" +
        "- If the text is primarily not Chinese, translate into Simplified Chinese / 简体中文.\n" +
        "- If mixed, pick one coherent target so the whole result is a complete translation.\n" +
        "Strict rules:\n" +
        "1. Output ONLY the translated text without commentary, conversational filler, notes, or markdown fences.\n" +
        "2. Merge line-wrapped text within the same paragraph into natural, fluent sentences. Never break a sentence across lines simply because the source image wrapped to fit a column or window.\n" +
        "3. Preserve real structural line breaks: use an empty line between distinct paragraphs; keep bullet points (- or *), numbered lists, code, and table rows on separate lines.\n" +
        "4. If there is no text in the image, return nothing.";

    private const string UserPrompt =
        "Translate all visible text in this image.\n" +
        "You decide the target language: primarily Chinese → English; otherwise → Simplified Chinese / 简体中文. " +
        "If mixed, pick one coherent target and translate everything into it.\n" +
        "Formatting and Line Break Rules:\n" +
        "- Merge intra-paragraph line wraps into coherent sentences without artificial line breaks " +
        "(同一自然段落内的文字合并为通顺连贯的完整句子，切勿在句中随意硬换行).\n" +
        "- Keep paragraphs separated by an empty line (不同自然段落之间保留空行).\n" +
        "- Keep bullet points (- or *), numbered lists, titles, and code on their own separate lines " +
        "(列表项、要点、标题、代码必须独立成行).\n" +
        "- Output only the translation.";

    public static async Task<string?> TranslateAsync(Bitmap source, AppSettings settings, CancellationToken ct)
    {
        var raw = await VisionClient.GenerateAsync(
            source, settings, UserPrompt, ct, SystemInstruction)
            .ConfigureAwait(false);
        return VisionClient.CleanText(raw, "translation", "text");
    }
}

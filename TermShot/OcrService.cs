using System.Text;

namespace TermShot;

internal readonly record struct OcrGlyph(int Index, int LineIndex, string Text, RectangleF Bounds);

internal sealed class OcrPage
{
    public IReadOnlyList<OcrGlyph> Glyphs { get; }
    public string FullText { get; }

    public OcrPage(IReadOnlyList<OcrGlyph> glyphs, string fullText)
    {
        Glyphs = glyphs;
        FullText = fullText;
    }

    public bool HasText => Glyphs.Count > 0 || !string.IsNullOrWhiteSpace(FullText);

    public string Slice(int from, int to)
    {
        if (Glyphs.Count == 0) return FullText ?? "";
        int a = Math.Clamp(Math.Min(from, to), 0, Glyphs.Count - 1);
        int b = Math.Clamp(Math.Max(from, to), 0, Glyphs.Count - 1);
        var sb = new StringBuilder();
        int lastLine = -1;
        string? prev = null;
        for (int i = a; i <= b; i++)
        {
            var g = Glyphs[i];
            if (lastLine >= 0 && g.LineIndex != lastLine)
                sb.AppendLine();
            else if (prev != null && !Tight(prev, g.Text))
                sb.Append(' ');
            sb.Append(g.Text);
            lastLine = g.LineIndex;
            prev = g.Text;
        }
        return sb.ToString();
    }

    public int LineStart(int index)
    {
        if (index < 0 || index >= Glyphs.Count) return index;
        int line = Glyphs[index].LineIndex;
        int i = index;
        while (i > 0 && Glyphs[i - 1].LineIndex == line) i--;
        return i;
    }

    public int LineEnd(int index)
    {
        if (index < 0 || index >= Glyphs.Count) return index;
        int line = Glyphs[index].LineIndex;
        int i = index;
        while (i + 1 < Glyphs.Count && Glyphs[i + 1].LineIndex == line) i++;
        return i;
    }

    private static bool Tight(string left, string right)
    {
        if (left.Length == 0 || right.Length == 0) return true;
        return IsCjk(left[^1]) || IsCjk(right[0]) || char.IsPunctuation(right[0]);
    }

    private static bool IsCjk(char c) =>
        c is >= '\u4E00' and <= '\u9FFF'
            or >= '\u3400' and <= '\u4DBF'
            or >= '\u3040' and <= '\u30FF'
            or >= '\uAC00' and <= '\uD7AF';
}

internal static class OcrService
{
    private const string OcrSystemInstruction =
        "You are an accurate OCR transcription engine. " +
        "Transcribe all visible text from the image faithfully.\n" +
        "Strict rules:\n" +
        "1. Output ONLY the transcribed text without commentary, conversational filler, or markdown fences.\n" +
        "2. For prose paragraphs: merge lines that wrap naturally within the same sentence or paragraph, avoiding artificial line breaks caused by column/window margins.\n" +
        "3. Preserve paragraph breaks with an empty line.\n" +
        "4. For code, terminal commands, bullet points (- or *), numbered lists, table rows, and titles: strictly preserve each line separately.\n" +
        "5. Join words hyphenated across lines (e.g. 'con-\\ntinue' -> 'continue').";

    public static async Task<OcrPage?> RecognizeAsync(Bitmap source, AppSettings settings, CancellationToken ct)
    {
        if (!settings.HasVision)
            return null;

        var raw = await VisionClient.GenerateAsync(source, settings,
            "Transcribe all visible text in this image accurately.\n" +
            "Formatting and Line Break Rules:\n" +
            "- Merge soft-wrapped lines within the same paragraph into a continuous sentence; do NOT insert artificial line breaks caused only by page or window margins.\n" +
            "- Use an empty line between distinct paragraphs.\n" +
            "- Strictly preserve separate lines for code, terminal commands, bullet points (- or *), and numbered lists.\n" +
            "- Join hyphenated words across lines into single words.\n" +
            "- Output only the transcribed text.",
            ct,
            OcrSystemInstruction).ConfigureAwait(false);

        var text = VisionClient.CleanText(raw, "text");
        if (string.IsNullOrWhiteSpace(text))
            return null;
        return new OcrPage([], text);
    }
}

use crate::bitmap::Bitmap;
use crate::settings::Settings;
use base64::Engine;
use regex::Regex;
use serde_json::{json, Value};

const URL: &str = "https://api.deepseek.com/chat/completions";

const OCR_SYS: &str = "You are an accurate OCR transcription engine. \
Transcribe all visible text from the image faithfully.\n\
Strict rules:\n\
1. Output ONLY the transcribed text without commentary, conversational filler, or markdown fences.\n\
2. For prose paragraphs: merge lines that wrap naturally within the same sentence or paragraph, avoiding artificial line breaks caused by column/window margins.\n\
3. Preserve paragraph breaks with an empty line.\n\
4. For code, terminal commands, bullet points (- or *), numbered lists, table rows, and titles: strictly preserve each line separately.\n\
5. Join words hyphenated across lines (e.g. 'con-\\ntinue' -> 'continue').";

const OCR_USER: &str = "Transcribe all visible text in this image accurately.\n\
Formatting and Line Break Rules:\n\
- Merge soft-wrapped lines within the same paragraph into a continuous sentence; do NOT insert artificial line breaks caused only by page or window margins.\n\
- Use an empty line between distinct paragraphs.\n\
- Strictly preserve separate lines for code, terminal commands, bullet points (- or *), and numbered lists.\n\
- Join hyphenated words across lines into single words.\n\
- Output only the transcribed text.";

const TR_SYS: &str = "You are an expert translation engine. \
Read all visible text in the image, infer its language(s), and choose the target language yourself. \
Do not ask which language to use. Do not mention the source or target language in the output.\n\
How to choose:\n\
- If the text is primarily Chinese, translate into English.\n\
- If the text is primarily not Chinese, translate into Simplified Chinese / 简体中文.\n\
- If mixed, pick one coherent target so the whole result is a complete translation.\n\
Strict rules:\n\
1. Output ONLY the translated text without commentary, conversational filler, notes, or markdown fences.\n\
2. Merge line-wrapped text within the same paragraph into natural, fluent sentences. Never break a sentence across lines simply because the source image wrapped to fit a column or window.\n\
3. Preserve real structural line breaks: use an empty line between distinct paragraphs; keep bullet points (- or *), numbered lists, code, and table rows on separate lines.\n\
4. If there is no text in the image, return nothing.";

const TR_USER: &str = "Translate all visible text in this image.\n\
You decide the target language: primarily Chinese → English; otherwise → Simplified Chinese / 简体中文. \
If mixed, pick one coherent target and translate everything into it.\n\
Formatting and Line Break Rules:\n\
- Merge intra-paragraph line wraps into coherent sentences without artificial line breaks \
(同一自然段落内的文字合并为通顺连贯的完整句子，切勿在句中随意硬换行).\n\
- Keep paragraphs separated by an empty line (不同自然段落之间保留空行).\n\
- Keep bullet points (- or *), numbered lists, titles, and code on their own separate lines \
(列表项、要点、标题、代码必须独立成行).\n\
- Output only the translation.";

pub fn status_line(s: &Settings) -> String {
    format!("DeepSeek · {}", s.model_name())
}

#[allow(dead_code)]
pub fn empty_hint() -> &'static str {
    "确认已在设置中填写有效的 DeepSeek API Key，且网络可访问 api.deepseek.com"
}

pub fn ocr(bmp: &Bitmap, settings: &Settings) -> Result<String, String> {
    if !settings.has_vision() {
        return Err("请先在设置中填写 DeepSeek API Key".into());
    }
    let raw = generate(bmp, settings, OCR_USER, Some(OCR_SYS))?;
    clean_text(&raw, &["text"]).ok_or_else(|| "未识别到文字".into())
}

pub fn translate(bmp: &Bitmap, settings: &Settings) -> Result<String, String> {
    if !settings.has_vision() {
        return Err("请先在设置中填写 DeepSeek API Key".into());
    }
    let raw = generate(bmp, settings, TR_USER, Some(TR_SYS))?;
    clean_text(&raw, &["translation", "text"]).ok_or_else(|| "没有译出文字".into())
}

fn generate(bmp: &Bitmap, settings: &Settings, prompt: &str, system: Option<&str>) -> Result<String, String> {
    let key = settings.deep_seek_api_key.trim();
    if key.is_empty() {
        return Err("请先在设置中填写 DeepSeek API Key".into());
    }
    let small = bmp.scaled_max_edge(1600);
    let png = small.to_png_bytes()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(png);
    let data_url = format!("data:image/png;base64,{b64}");
    let mut messages = Vec::new();
    if let Some(sys) = system {
        messages.push(json!({"role":"system","content":sys}));
    }
    messages.push(json!({
        "role":"user",
        "content":[
            {"type":"text","text":prompt},
            {"type":"image_url","image_url":{"url":data_url,"detail":"high"}}
        ]
    }));
    let body = json!({
        "model": settings.model_name(),
        "stream": false,
        "temperature": 0,
        "messages": messages
    });
    let resp = match ureq::post(URL)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(180))
        .send_string(&body.to_string())
    {
        Ok(r) => r,
        Err(ureq::Error::Status(status, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            return Err(format!("DeepSeek {status}: {}", trim_err(&text)));
        }
        Err(e) => return Err(format!("DeepSeek: {e}")),
    };
    let status = resp.status();
    let text = resp.into_string().unwrap_or_default();
    if status != 200 {
        return Err(format!("DeepSeek {status}: {}", trim_err(&text)));
    }
    parse_content(&text).ok_or_else(|| "empty response".into())
}

fn parse_content(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let s = v["choices"][0]["message"]["content"].as_str()?;
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn trim_err(body: &str) -> String {
    let mut b = body.trim().to_string();
    if let Ok(v) = serde_json::from_str::<Value>(&b) {
        if let Some(m) = v["error"]["message"].as_str() {
            b = m.to_string();
        } else if let Some(m) = v["error"].as_str() {
            b = m.to_string();
        }
    }
    if b.chars().count() > 240 {
        b = b.chars().take(240).collect::<String>() + "…";
    }
    if b.is_empty() {
        "empty response".into()
    } else {
        b
    }
}

fn clean_text(raw: &str, json_keys: &[&str]) -> Option<String> {
    let mut text = raw.trim().replace("\r\n", "\n").replace('\r', "\n");
    if text.starts_with("```") {
        if let Some(i) = text.find('\n') {
            text = text[i + 1..].to_string();
        }
        if text.ends_with("```") {
            text = text[..text.len() - 3].to_string();
        }
        text = text.trim().to_string();
    }
    if text.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            for k in json_keys {
                if let Some(s) = v[*k].as_str() {
                    if !s.trim().is_empty() {
                        text = s.trim().to_string();
                        break;
                    }
                }
            }
        }
    }
    text = strip_noise(&text);
    static SUFFIX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let suffix = SUFFIX.get_or_init(|| {
        Regex::new(r"(?is)(?:\r?\n|\s)+(?:[β\u03b2\u3137]+.*|(?:Transcribe|Translate)\s+all\s+visible.*)$")
            .unwrap()
    });
    text = suffix.replace(&text, "").to_string();
    static HY: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let hy = HY.get_or_init(|| Regex::new(r"([A-Za-z]{2,})-\n\s*([A-Za-z]{2,})").unwrap());
    text = hy.replace_all(&text, "$1$2").to_string();
    text = text
        .lines()
        .map(|l| l.trim_end_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n");
    static NL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let nl = NL.get_or_init(|| Regex::new(r"\n{3,}").unwrap());
    text = nl.replace_all(&text, "\n\n").to_string();
    text = text.replace('\n', "\r\n").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn strip_noise(text: &str) -> String {
    static PRE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let pre = PRE.get_or_init(|| {
        Regex::new(
            r"^(?:(?:好的[，,。]?\s*)?(?:我)?(?:可以|已)?(?:查看|看|看到)(?:此|这张|该)?(?:屏幕)?截图[。！!：:\s]*|(?:根据|在)(?:此|这张|该)?(?:屏幕)?截图(?:中|里)?(?:可见文字|内容)?[，,。：:\s]*|(?:以下|这里)是(?:翻译|转录|识别)?(?:结果|文本|内容)?[，,。：:\s]*|(?:Sure[!,.]?\s*)?(?:Here|Below) is the (?:translation|text|transcription)[，,.:\s]*|Translation:\s*)",
        )
        .unwrap()
    });
    let mut text = text.to_string();
    loop {
        if let Some(m) = pre.find(&text) {
            if m.start() == 0 && m.end() > 0 {
                text = text[m.end()..].trim().to_string();
                continue;
            }
        }
        break;
    }
    text
}

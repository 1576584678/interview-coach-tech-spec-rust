//! 简历文件解析:纯本地实现,不依赖外部服务。
//!
//! 支持 txt/md/xml/csv/htm/html(纯文本类)、docx、pdf(文本型)。
//! 纯文本会自动识别 BOM、UTF-8 与 GBK/GB18030(中文 Windows 记事本默认编码)。
//! 扫描件、图片型 PDF、旧版 .doc 无法解析,接口会返回明确提示,前端引导用户粘贴文本。

use std::io::Read;

use encoding_rs::GB18030;

use crate::error::{code, AppError, AppResult};
use crate::llm::abbreviate;

/// 单文件最大 12MB,避免误传大文件把内存吃满;路由层也用它作为请求体上限。
pub const MAX_FILE_SIZE: usize = 12 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 60_000;

pub fn extract(file_name: &str, bytes: &[u8]) -> AppResult<String> {
    if bytes.is_empty() {
        return Err(AppError::business(code::FILE_PARSE_FAILED, "文件内容为空"));
    }
    if bytes.len() > MAX_FILE_SIZE {
        return Err(AppError::business(
            code::FILE_PARSE_FAILED,
            format!("文件超过 {}MB,请压缩或直接粘贴文本", MAX_FILE_SIZE / 1024 / 1024),
        ));
    }
    let ext = file_name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    let text = match ext.as_str() {
        "txt" | "md" | "markdown" | "text" | "json" | "csv" | "log" | "yml" | "yaml" => {
            if looks_binary(bytes) {
                return Err(binary_hint(&ext));
            }
            decode_text(bytes)
        }
        "html" | "htm" => {
            if looks_binary(bytes) {
                return Err(binary_hint(&ext));
            }
            strip_html(&decode_text(bytes))
        }
        "docx" => docx_text(bytes)?,
        "pdf" => pdf_text(bytes)?,
        "doc" => {
            return Err(AppError::business(
                code::FILE_PARSE_FAILED,
                "暂不支持旧版 .doc 格式,请另存为 .docx 或 PDF,或直接粘贴简历文本",
            ))
        }
        other => {
            let decoded = decode_text(bytes);
            if looks_binary(bytes) || decoded.chars().filter(|c| c.is_control() && *c != '\n' && *c != '\t').count() > decoded.len() / 20 {
                return Err(AppError::business(
                    code::FILE_PARSE_FAILED,
                    format!("不支持的文件类型 .{other},请上传 txt/md/docx/pdf 或直接粘贴文本"),
                ));
            }
            decoded
        }
    };

    let cleaned = cleanup(&text);
    if cleaned.trim().chars().count() < 20 {
        return Err(AppError::business(
            code::FILE_PARSE_FAILED,
            "未能从文件中解析出足够文本(可能是扫描件或图片),请直接粘贴简历文本",
        ));
    }
    Ok(truncate_chars(&cleaned, MAX_TEXT_CHARS))
}

fn decode_text(bytes: &[u8]) -> String {
    // 1. 有 BOM 就按 BOM 声明的编码解(UTF-8 / UTF-16LE / UTF-16BE)。
    if let Some((encoding, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        let (text, _) = encoding.decode_without_bom_handling(&bytes[bom_len..]);
        return text.trim_start_matches('\u{feff}').to_string();
    }
    // 2. 合法 UTF-8(含纯 ASCII)直接用,避免对 UTF-8 文本做多余猜测。
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    // 3. 中文 Windows 上记事本/Word 导出的 txt 常见 GBK/GB2312,用 GB18030(超集)解。
    let (text, _, had_errors) = GB18030.decode(bytes);
    if !had_errors && !looks_garbled(&text) {
        return text.into_owned();
    }
    // 4. 都不像,退回 UTF-8 宽松解码(保留替换字符,便于上层提示用户)。
    String::from_utf8_lossy(bytes).to_string()
}

/// 判断解码结果是否大面积异常(替换字符或控制字符过多),用于放弃错误编码的猜测。
fn looks_garbled(text: &str) -> bool {
    let total = text.chars().count();
    if total == 0 {
        return false;
    }
    let bad = text
        .chars()
        .filter(|c| *c == '\u{fffd}' || (c.is_control() && !matches!(c, '\n' | '\r' | '\t')))
        .count();
    bad * 10 > total
}

fn looks_binary(bytes: &[u8]) -> bool {
    // 带 BOM 的 UTF-16/UTF-8 文本本身会含 0 字节,先按 BOM 放行。
    if encoding_rs::Encoding::for_bom(bytes).is_some() {
        return false;
    }
    let sample = &bytes[..bytes.len().min(1024)];
    sample.contains(&0u8)
}

fn binary_hint(ext: &str) -> AppError {
    AppError::business(
        code::FILE_PARSE_FAILED,
        format!("文件 .{ext} 看起来不是纯文本(可能是图片或二进制文件),请上传 txt/md/docx/pdf 或直接粘贴简历文本"),
    )
}

fn cleanup(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(normalized.len());
    let mut blank_run = 0;
    for line in normalized.lines() {
        let trimmed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(&trimmed);
        out.push('\n');
    }
    out.trim().to_string()
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    format!("{head}\n…(内容过长已截断)")
}

fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut chars = html.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                in_tag = true;
                // br/p/div/li 之类换行标签补一个换行,避免文字粘在一起
                let rest: String = chars.clone().take(6).collect::<String>().to_lowercase();
                if rest.starts_with("br") || rest.starts_with("/p") || rest.starts_with("/div") || rest.starts_with("/li") || rest.starts_with("/tr") {
                    out.push('\n');
                }
            }
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    decode_entities(&out)
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

// ============================ DOCX ============================

/// 从 docx(zip)中取出 word/document.xml 并转成纯文本。
fn docx_text(bytes: &[u8]) -> AppResult<String> {
    let xml = zip_read_entry(bytes, "word/document.xml").map_err(|e| {
        AppError::business(code::FILE_PARSE_FAILED, format!("docx 解析失败: {e}"))
    })?;
    let xml = decode_text(&xml);
    Ok(xml_to_text(&xml))
}

fn xml_to_text(xml: &str) -> String {
    let with_breaks = xml
        .replace("</w:p>", "\n")
        .replace("<w:br/>", "\n")
        .replace("<w:br />", "\n")
        .replace("</w:tc>", " ")
        .replace("</w:tr>", "\n");
    let mut out = String::with_capacity(with_breaks.len());
    let mut in_tag = false;
    for ch in with_breaks.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    decode_entities(&out)
}

/// 读取 zip 中指定条目的原始内容(支持 store 与 deflate 两种压缩方式)。
fn zip_read_entry(bytes: &[u8], target: &str) -> Result<Vec<u8>, String> {
    let eocd = find_last(bytes, &[0x50, 0x4b, 0x05, 0x06]).ok_or("不是合法的 zip/docx 文件")?;
    if eocd + 22 > bytes.len() {
        return Err("zip 结尾标记不完整".to_string());
    }
    let cd_offset = read_u32(bytes, eocd + 16)? as usize;
    let mut cursor = cd_offset;
    while cursor + 46 <= bytes.len() {
        if read_u32(bytes, cursor)? != 0x0201_4b50 {
            break;
        }
        let method = read_u16(bytes, cursor + 10)?;
        let comp_size = read_u32(bytes, cursor + 20)? as usize;
        let name_len = read_u16(bytes, cursor + 28)? as usize;
        let extra_len = read_u16(bytes, cursor + 30)? as usize;
        let comment_len = read_u16(bytes, cursor + 32)? as usize;
        let local_offset = read_u32(bytes, cursor + 42)? as usize;
        let name_start = cursor + 46;
        let name_end = name_start + name_len;
        if name_end > bytes.len() {
            return Err("zip 目录项越界".to_string());
        }
        let name = String::from_utf8_lossy(&bytes[name_start..name_end]).to_string();
        if name == target {
            return read_local_entry(bytes, local_offset, comp_size, method);
        }
        cursor = name_end + extra_len + comment_len;
    }
    Err(format!("docx 中缺少 {target}"))
}

fn read_local_entry(bytes: &[u8], local_offset: usize, comp_size: usize, method: u16) -> Result<Vec<u8>, String> {
    if local_offset + 30 > bytes.len() || read_u32(bytes, local_offset)? != 0x0403_4b50 {
        return Err("zip 本地头损坏".to_string());
    }
    let name_len = read_u16(bytes, local_offset + 26)? as usize;
    let extra_len = read_u16(bytes, local_offset + 28)? as usize;
    let data_start = local_offset + 30 + name_len + extra_len;
    let data_end = (data_start + comp_size).min(bytes.len());
    if data_start >= data_end {
        return Err("zip 数据区为空".to_string());
    }
    let data = &bytes[data_start..data_end];
    match method {
        0 => Ok(data.to_vec()),
        8 => {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(data)
                .read_to_end(&mut out)
                .map_err(|e| format!("deflate 解压失败: {e}"))?;
            Ok(out)
        }
        other => Err(format!("不支持的压缩方式 {other}")),
    }
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).rev().find(|i| &haystack[*i..*i + needle.len()] == needle)
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, String> {
    let slice = bytes.get(at..at + 2).ok_or("读取越界")?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    let slice = bytes.get(at..at + 4).ok_or("读取越界")?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

// ============================ PDF ============================

/// PDF 文本提取。
///
/// 首选 pdf-extract:它会按「字体」解析 ToUnicode CMap、CID 编码与各类字体编码。
/// 中文简历 PDF 几乎都是嵌入子集字体,多个字体的码位会互相覆盖,自己按全局
/// CMap 猜会解出「韩文谚文 + 全角字母」这种伪字符,所以这里不再自己猜。
/// pdf-extract 失败(非标准 PDF、缺 xref 等)时退回自研的简单内容流解析。
fn pdf_text(bytes: &[u8]) -> AppResult<String> {
    let primary = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem(bytes)
    }))
    .ok()
    .and_then(Result::ok);
    if let Some(text) = primary {
        if text.trim().chars().count() >= 20 {
            return Ok(glue_spaced_ascii(&text));
        }
    }
    let fallback = pdf_text_fallback(bytes);
    match fallback {
        Ok(text) if !looks_like_cid_garbage(&text) => Ok(glue_spaced_ascii(&text)),
        _ => Err(AppError::business(
            code::FILE_PARSE_FAILED,
            "PDF 中的文字是嵌入字体且没有可用的编码表,无法可靠还原为文字。请直接粘贴简历文本,或用 Word/WPS 另存为 .docx 后上传",
        )),
    }
}

/// 部分 PDF(浏览器打印、语雀导出等)把每个字母单独定位,提取出来会变成 "E S l i n t"。
/// 把连续出现(≥3 个)的单字符 ASCII 片段重新粘回单词。
fn glue_spaced_ascii(text: &str) -> String {
    fn is_glue_token(token: &str) -> bool {
        token.chars().count() == 1
            && token.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+' | '#' | '/' | ':'))
    }
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let tokens: Vec<&str> = line.split(' ').collect();
        let mut idx = 0usize;
        let mut first = true;
        while idx < tokens.len() {
            let mut end = idx;
            while end < tokens.len() && is_glue_token(tokens[end]) {
                end += 1;
            }
            let (piece, next) = if end - idx >= 3 {
                (tokens[idx..end].concat(), end)
            } else {
                (tokens[idx].to_string(), idx + 1)
            };
            if !first {
                out.push(' ');
            }
            out.push_str(&piece);
            first = false;
            idx = next;
        }
        out.push('\n');
    }
    out
}

/// 简单 PDF 文本提取:解压内容流,按 Tj/TJ 等操作符抽取文本,并尝试用 ToUnicode CMap 还原字符。
fn pdf_text_fallback(bytes: &[u8]) -> AppResult<String> {
    let streams: Vec<Vec<u8>> = pdf_content_streams(bytes)
        .into_iter()
        .filter(|stream| has_text_operator(stream))
        .collect();
    if streams.is_empty() {
        return Err(AppError::business(
            code::FILE_PARSE_FAILED,
            "PDF 中没有可读取的文本层(可能是扫描件),请直接粘贴简历文本",
        ));
    }
    let cmap = build_tounicode_map(bytes);
    let mut out = String::new();
    for stream in streams {
        out.push_str(&extract_stream_text(&stream, &cmap));
        out.push('\n');
    }
    Ok(out)
}

/// 只处理真正的内容流,跳过字体程序、图片、元数据等二进制流。
fn has_text_operator(stream: &[u8]) -> bool {
    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }
    contains(stream, b"Tj") || contains(stream, b"TJ") || contains(stream, b"Td") || contains(stream, b"BT")
}

/// 识别「嵌入子集字体 + ToUnicode 不全」时解出的伪字符:韩文谚文、私用区、替换字符。
/// 这类字符在简历里几乎不可能出现,比例一高就说明解码方式错了。
fn looks_like_cid_garbage(text: &str) -> bool {
    let mut total = 0usize;
    let mut bad = 0usize;
    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        total += 1;
        let code = ch as u32;
        let hangul = (0x1100..=0x11FF).contains(&code) || (0xAC00..=0xD7AF).contains(&code);
        let private_use = (0xE000..=0xF8FF).contains(&code);
        if hangul || private_use || ch == '\u{fffd}' {
            bad += 1;
        }
    }
    total > 0 && bad * 20 > total
}

/// 取出所有已解压的内容流(FlateDecode 会被解开,其他编码原样使用)。
fn pdf_content_streams(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut streams = Vec::new();
    let mut cursor = 0usize;
    while let Some(pos) = find_subslice(&bytes[cursor..], b"stream") {
        let start = cursor + pos;
        let mut data_start = start + "stream".len();
        // 跳过 stream 关键字后的换行
        if bytes.get(data_start) == Some(&b'\r') {
            data_start += 1;
        }
        if bytes.get(data_start) == Some(&b'\n') {
            data_start += 1;
        }
        let Some(end_rel) = find_subslice(&bytes[data_start..], b"endstream") else { break };
        let data_end = data_start + end_rel;
        let dict_start = start.saturating_sub(400);
        let dict = String::from_utf8_lossy(&bytes[dict_start..start]).to_string();
        let raw = &bytes[data_start..data_end];
        let decoded = if dict.contains("FlateDecode") {
            // FlateDecode 标准是 zlib 包装,少数文件是裸 deflate,失败时再试一次
            inflate(raw, true).or_else(|| inflate(raw, false)).unwrap_or_default()
        } else {
            raw.to_vec()
        };
        if !decoded.is_empty() {
            streams.push(decoded);
        }
        cursor = data_end + "endstream".len();
    }
    streams
}

fn inflate(raw: &[u8], zlib_header: bool) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let ok = if zlib_header {
        flate2::read::ZlibDecoder::new(raw).read_to_end(&mut out).is_ok()
    } else {
        flate2::read::DeflateDecoder::new(raw).read_to_end(&mut out).is_ok()
    };
    if ok && !out.is_empty() {
        Some(out)
    } else {
        None
    }
}

/// 解析所有 ToUnicode CMap(bfchar / bfrange),合并成 code -> 字符 的映射。
fn build_tounicode_map(bytes: &[u8]) -> std::collections::HashMap<u32, String> {
    let mut map = std::collections::HashMap::new();
    for stream in pdf_content_streams(bytes) {
        let text = String::from_utf8_lossy(&stream).to_string();
        if !text.contains("beginbfchar") && !text.contains("beginbfrange") {
            continue;
        }
        parse_cmap(&text, &mut map);
    }
    map
}

fn parse_cmap(text: &str, map: &mut std::collections::HashMap<u32, String>) {
    let tokens: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '[' || c == ']')
        .filter(|t| !t.is_empty())
        .collect();
    let mut idx = 0usize;
    while idx < tokens.len() {
        match tokens[idx] {
            "beginbfchar" => {
                idx += 1;
                while idx + 1 < tokens.len() && tokens[idx] != "endbfchar" {
                    if let (Some(src), Some(dst)) = (hex_to_code(tokens[idx]), hex_to_utf16(tokens[idx + 1])) {
                        map.insert(src, dst);
                    }
                    idx += 2;
                }
            }
            "beginbfrange" => {
                idx += 1;
                while idx + 2 < tokens.len() && tokens[idx] != "endbfrange" {
                    let (from, to) = (hex_to_code(tokens[idx]), hex_to_code(tokens[idx + 1]));
                    if let (Some(from), Some(to)) = (from, to) {
                        if let Some(base) = hex_to_utf16(tokens[idx + 2]) {
                            let base_code = base.chars().next().map(|c| c as u32).unwrap_or(0);
                            for offset in 0..=(to.saturating_sub(from)).min(65535) {
                                if let Some(ch) = char::from_u32(base_code + offset) {
                                    map.insert(from + offset, ch.to_string());
                                }
                            }
                        }
                    }
                    idx += 3;
                }
            }
            _ => idx += 1,
        }
    }
}

fn hex_to_code(token: &str) -> Option<u32> {
    let cleaned: String = token.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if cleaned.is_empty() || cleaned.len() > 8 {
        return None;
    }
    u32::from_str_radix(&cleaned, 16).ok()
}

fn hex_to_utf16(token: &str) -> Option<String> {
    let cleaned: String = token.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if cleaned.len() % 4 != 0 {
        return None;
    }
    let mut units = Vec::new();
    for chunk in cleaned.as_bytes().chunks(4) {
        let part = std::str::from_utf8(chunk).ok()?;
        units.push(u16::from_str_radix(part, 16).ok()?);
    }
    let decoded = String::from_utf16(&units).ok()?;
    Some(decoded)
}

/// 从内容流中抽取文本:Tj / TJ / ' / " 输出文本,Td/TD/T*/ET 换行。
fn extract_stream_text(stream: &[u8], cmap: &std::collections::HashMap<u32, String>) -> String {
    let content = String::from_utf8_lossy(stream).to_string();
    let chars: Vec<char> = content.chars().collect();
    let mut out = String::new();
    let mut pending: Vec<String> = Vec::new();
    let mut idx = 0usize;
    while idx < chars.len() {
        match chars[idx] {
            '(' => {
                let (text, next) = read_literal_string(&chars, idx + 1);
                pending.push(text);
                idx = next;
            }
            '<' if chars.get(idx + 1) != Some(&'<') => {
                let (text, next) = read_hex_string(&chars, idx + 1, cmap);
                if !text.is_empty() {
                    pending.push(text);
                }
                idx = next;
            }
            'T' if matches!(chars.get(idx + 1), Some('j') | Some('J') | Some('d') | Some('D') | Some('*')) => {
                let op = chars[idx + 1];
                if op == 'j' || op == 'J' {
                    out.push_str(&pending.concat());
                    pending.clear();
                } else {
                    pending.clear();
                    if !out.ends_with('\n') && !out.is_empty() {
                        out.push('\n');
                    }
                }
                idx += 2;
            }
            '\n' | '\r' => {
                out.push('\n');
                idx += 1;
            }
            _ => idx += 1,
        }
    }
    out.push_str(&pending.concat());
    out
}

/// 读取 PDF 字面量字符串,处理转义与括号嵌套。
fn read_literal_string(chars: &[char], mut idx: usize) -> (String, usize) {
    let mut out = String::new();
    let mut depth = 1;
    while idx < chars.len() {
        let ch = chars[idx];
        match ch {
            '\\' => {
                idx += 1;
                match chars.get(idx) {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('b') => out.push('\u{8}'),
                    Some('f') => out.push('\u{c}'),
                    Some('(') => out.push('('),
                    Some(')') => out.push(')'),
                    Some('\\') => out.push('\\'),
                    Some(other) => out.push(*other),
                    None => break,
                }
                idx += 1;
            }
            '(' => {
                depth += 1;
                out.push('(');
                idx += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return (out, idx + 1);
                }
                out.push(')');
                idx += 1;
            }
            _ => {
                out.push(ch);
                idx += 1;
            }
        }
    }
    (out, idx)
}

fn read_hex_string(chars: &[char], mut idx: usize, cmap: &std::collections::HashMap<u32, String>) -> (String, usize) {
    let mut hex = String::new();
    while idx < chars.len() {
        let ch = chars[idx];
        if ch == '>' {
            idx += 1;
            break;
        }
        if ch.is_ascii_hexdigit() {
            hex.push(ch);
        }
        idx += 1;
    }
    if hex.len() < 2 {
        return (String::new(), idx);
    }
    let mut text = String::new();
    for code in hex_codes(&hex) {
        if let Some(mapped) = cmap.get(&code) {
            text.push_str(mapped);
        } else if (0x20..=0x7e).contains(&code) {
            // 没有映射时只保留可打印 ASCII;非 ASCII 的码位多半是字体 CID,
            // 硬转成字符会得到韩文谚文之类的伪字符,不如直接丢弃。
            if let Some(ch) = char::from_u32(code) {
                text.push(ch);
            }
        }
    }
    (text, idx)
}

/// 十六进制串按 2 字节一组解析为字符码;长度为奇数时按 1 字节处理。
fn hex_codes(hex: &str) -> Vec<u32> {
    let padded = if hex.len() % 2 == 1 {
        let mut value = hex.to_string();
        value.push('0');
        value
    } else {
        hex.to_string()
    };
    if padded.len() % 4 == 0 && padded.len() >= 4 {
        // 常见 CMap(Identity-H)是 2 字节编码
        padded
            .as_bytes()
            .chunks(4)
            .filter_map(|chunk| std::str::from_utf8(chunk).ok())
            .filter_map(|part| u32::from_str_radix(part, 16).ok())
            .collect()
    } else {
        padded
            .as_bytes()
            .chunks(2)
            .filter_map(|chunk| std::str::from_utf8(chunk).ok())
            .filter_map(|part| u32::from_str_radix(part, 16).ok())
            .collect()
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 解析失败时的统一提示文案(供接口层复用)。
pub fn parse_hint(error: &AppError) -> String {
    abbreviate(&error.message(), 200)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        crc.sum()
    }

    fn deflate_raw(data: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).expect("deflate failed");
        encoder.finish().expect("deflate finish failed")
    }

    /// 手写一个最小 zip:测试自己拼字节比引入 zip 依赖更划算。
    fn build_zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut central: Vec<u8> = Vec::new();
        for (name, content, use_deflate) in entries {
            let offset = out.len() as u32;
            let (method, data): (u16, Vec<u8>) = if *use_deflate {
                (8, deflate_raw(content))
            } else {
                (0, content.to_vec())
            };
            let crc = crc32(content);
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(content.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&data);

            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(content.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    const DOC_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>张三 · Java 后端开发</w:t></w:r></w:p>
<w:p><w:r><w:t>5 年经验,负责订单系统重构,把下单耗时从 800ms 降到 200ms</w:t></w:r></w:p>
</w:body></w:document>"#;

    fn docx_bytes(use_deflate: bool) -> Vec<u8> {
        build_zip(&[("word/document.xml", DOC_XML.as_bytes(), use_deflate)])
    }

    #[test]
    fn docx_is_parsed_when_deflated() {
        let text = extract("简历.docx", &docx_bytes(true)).expect("docx 解析失败");
        assert!(text.contains("订单系统重构"), "解析结果: {text}");
        assert!(text.contains("Java 后端开发"));
        assert!(text.contains("200ms"));
    }

    #[test]
    fn docx_is_parsed_when_stored() {
        let text = extract("resume.docx", &docx_bytes(false)).expect("docx 解析失败");
        assert!(text.contains("订单系统重构"), "解析结果: {text}");
    }

    #[test]
    fn pdf_text_layer_is_extracted() {
        let content = "BT /F1 12 Tf (Java backend engineer with 5 years experience) Tj ET";
        let raw = format!(
            "%PDF-1.4\n4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n",
            content.len(),
            content
        );
        let text = extract("resume.pdf", raw.as_bytes()).expect("pdf 解析失败");
        assert!(text.contains("Java backend engineer"), "解析结果: {text}");
    }

    #[test]
    fn html_tags_are_stripped() {
        let html = "<html><body><h1>张三</h1><p>Java 后端开发,5 年经验,熟悉订单系统与高并发</p></body></html>";
        let text = extract("resume.html", html.as_bytes()).expect("html 解析失败");
        assert!(text.contains("Java 后端开发"));
        assert!(!text.contains("<h1>"));
    }

    #[test]
    fn plain_text_is_accepted() {
        let raw = "张三 · Java 后端开发\n5 年经验,负责订单系统重构,把下单耗时从 800ms 降到 200ms";
        let text = extract("resume.txt", raw.as_bytes()).expect("txt 解析失败");
        assert!(text.contains("订单系统重构"));
    }

    #[test]
    fn legacy_doc_gets_clear_hint() {
        let err = extract("resume.doc", b"some legacy binary content here").unwrap_err();
        assert!(err.message().contains("docx"), "提示文案: {}", err.message());
    }

    #[test]
    fn empty_file_is_rejected() {
        let err = extract("resume.txt", b"").unwrap_err();
        assert!(err.message().contains("为空"));
    }

    /// 中文 Windows 另存为 txt 默认是 GBK,不能当成 UTF-8 解出乱码。
    #[test]
    fn gbk_text_is_decoded() {
        let gbk: &[u8] = &[
            0xB6, 0xA9, 0xB5, 0xA5, 0xCF, 0xB5, 0xCD, 0xB3, 0xD6, 0xD8, 0xB9, 0xB9, // 订单系统重构
            0x2C, 0xB0, 0xD1, 0xCF, 0xC2, 0xB5, 0xA5, 0xBA, 0xC4, 0xCA, 0xB1, 0xB4, 0xD3, // ,把下单耗时
            0x38, 0x30, 0x30, 0x6D, 0x73, 0xBD, 0xB5, 0xB5, 0xBD, 0x32, 0x30, 0x30, 0x6D, 0x73, // 800ms降到200ms
            0x0D, 0x0A, 0xD5, 0xC5, 0xC8, 0xFD, 0x20, 0x4A, 0x61, 0x76, 0x61, // 张三 Java
        ];
        let bytes = gbk.to_vec();
        assert!(std::str::from_utf8(&bytes).is_err(), "该用例必须不是合法 UTF-8");
        let text = extract("简历.txt", &bytes).expect("GBK 解析失败");
        assert!(text.contains("订单系统重构"), "解析结果: {text}");
        assert!(text.contains("张三"), "解析结果: {text}");
        assert!(!text.contains('\u{fffd}'), "不应出现替换字符: {text}");
    }

    #[test]
    fn utf8_bom_is_stripped() {
        let mut raw = vec![0xEF, 0xBB, 0xBF];
        raw.extend_from_slice("张三 · Java 后端开发,负责订单系统重构".as_bytes());
        let text = extract("resume.txt", &raw).expect("UTF-8 BOM 解析失败");
        assert!(text.starts_with("张三"), "解析结果: {text}");
        assert!(text.contains("订单系统重构"));
    }

    #[test]
    fn utf16le_text_is_decoded() {
        let content = "张三 · Java 后端开发,负责订单系统重构";
        let mut raw = vec![0xFF, 0xFE];
        for unit in content.encode_utf16() {
            raw.extend_from_slice(&unit.to_le_bytes());
        }
        let text = extract("resume.txt", &raw).expect("UTF-16LE 解析失败");
        assert!(text.contains("订单系统重构"), "解析结果: {text}");
    }

    /// 图片/二进制文件被改名成 .txt 时,应给出明确提示而不是把乱码喂给大模型。
    #[test]
    fn binary_txt_is_rejected_with_hint() {
        let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&[0u8; 64]);
        let err = extract("resume.txt", &png).unwrap_err();
        assert!(err.message().contains("不是纯文本"), "提示文案: {}", err.message());
    }

    /// 真实案例:中文 PDF 用嵌入子集字体,按全局 CMap 硬解会得到韩文伪字符。
    #[test]
    fn cid_garbage_is_detected() {
        let garbled = "촀촐前端建设发规范\n촠E\nS\nl\ni\nn\nt\n겮\n촑l\ni\nn";
        assert!(looks_like_cid_garbage(garbled), "应识别为伪字符");
        assert!(!looks_like_cid_garbage("张三 · Java 后端开发,5 年经验,负责订单系统重构"));
        assert!(!looks_like_cid_garbage(""));
    }

    /// 逐字母定位的 PDF 会解出 "E S l i n t",这里把它们粘回单词。
    #[test]
    fn spaced_ascii_letters_are_glued() {
        let raw = "VSCode安装 E S l i n t 和 P r e t t i e r 插件\n配置VSCode s e t t i n g . j s o n 文件\n正常 的 句子 不受 影响";
        let out = glue_spaced_ascii(raw);
        assert!(out.contains("VSCode安装 ESlint 和 Prettier 插件"), "{out}");
        assert!(out.contains("配置VSCode setting.json 文件"), "{out}");
        assert!(out.contains("正常 的 句子 不受 影响"), "{out}");
    }

    /// 内容流里只有 CID 码位、又没有可用 ToUnicode 时,应报错要用户粘贴文本,而不是输出乱码。
    #[test]
    fn pdf_with_unmapped_cids_asks_for_text() {
        let content = "BT /F1 12 Tf <00B600A900B5> Tj ET";
        let raw = format!(
            "%PDF-1.4\n4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n",
            content.len(),
            content
        );
        let err = extract("resume.pdf", raw.as_bytes()).unwrap_err();
        assert!(err.message().contains("粘贴"), "提示文案: {}", err.message());
    }
}

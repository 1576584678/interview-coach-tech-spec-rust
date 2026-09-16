//! 小工具:时间格式化、文本截断。

use chrono::Local;

/// 本地时间字符串 `YYYY-MM-DD HH:MM:SS`(排序友好,前端直接展示)。
pub fn now_iso() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    format!("{head}…")
}

/// 面试题型的展示名。
pub fn question_type_label(question_type: &str) -> &'static str {
    match question_type {
        "basic" => "基础技术",
        "project" => "项目深挖",
        "deep" => "场景设计",
        "open" => "开放题",
        "diagnosis" => "简历追问",
        _ => "综合",
    }
}

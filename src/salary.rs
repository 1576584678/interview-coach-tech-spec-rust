//! 薪资定位:以本地基准表为准(可审计),未命中时由调用方走大模型兜底。

use std::collections::HashMap;

use serde::Deserialize;

use crate::models::SalaryRange;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRow {
    pub position: String,
    #[serde(default)]
    pub category: String,
    pub city_tier: String,
    pub experience_level: String,
    pub p25: i64,
    pub p50: i64,
    pub p75: i64,
    #[serde(default)]
    pub data_source: String,
}

pub struct SalaryPlanner {
    rows: Vec<BenchmarkRow>,
    aliases: HashMap<&'static str, &'static str>,
}

/// 未收录城市的默认系数(二线及其他)。
const DEFAULT_CITY_FACTOR: f64 = 0.62;

impl SalaryPlanner {
    pub fn embedded() -> Self {
        let raw = include_str!("data/salary_benchmark.json");
        let rows: Vec<BenchmarkRow> = serde_json::from_str(raw)
            .expect("内置薪资基准数据损坏,请检查 src/data/salary_benchmark.json");
        Self { rows, aliases: position_aliases() }
    }

    pub fn benchmark_count(&self) -> usize {
        self.rows.len()
    }

    /// 本地基准表查询;返回 None 表示需要大模型兜底。
    pub fn estimate_local(&self, position: &str, city: &str, experience: &str) -> Option<SalaryRange> {
        let normalized = normalize_position(position);
        if normalized.is_empty() {
            return None;
        }
        let city_tier = resolve_city_tier(city);
        let experience_level = resolve_experience(experience);

        let mut hit = self.lookup(&normalized, city_tier, experience_level);
        let mut confidence = "high";
        if hit.is_none() {
            if let Some(alias) = self.resolve_alias(&normalized) {
                hit = self.lookup(&normalize_position(alias), city_tier, experience_level);
            }
        }
        if hit.is_none() {
            hit = self.fuzzy_lookup(&normalized, city_tier, experience_level);
            confidence = "medium";
        }
        let row = hit?;

        let factor = resolve_city_factor(city);
        let factor_pct = (factor * 100.0).round() as i64;
        let (p25, p50, p75) = if factor_pct == 100 {
            (row.p25, row.p50, row.p75)
        } else {
            (round_salary(row.p25, factor), round_salary(row.p50, factor), round_salary(row.p75, factor))
        };
        let city_label = city_tier_label(city_tier);
        Some(SalaryRange {
            position: row.position.clone(),
            city: city.trim().to_string(),
            city_tier: city_label.to_string(),
            experience_level: experience_label(experience_level).to_string(),
            p25,
            p50,
            p75,
            confidence: confidence.to_string(),
            source: "benchmark".to_string(),
            note: Some(format!(
                "以北京公开招聘市场数据为基准×{factor_pct}%折算至{},反映主流范围,个体差异取决于公司、面试表现与谈判,仅供参考。",
                if city.trim().is_empty() { "该城市" } else { city.trim() }
            )),
        })
    }

    fn lookup(&self, normalized: &str, city_tier: &str, experience_level: &str) -> Option<&BenchmarkRow> {
        self.rows.iter().find(|row| {
            row.city_tier == city_tier
                && row.experience_level == experience_level
                && normalize_position(&row.position) == normalized
        })
    }

    fn fuzzy_lookup(&self, normalized: &str, city_tier: &str, experience_level: &str) -> Option<&BenchmarkRow> {
        self.rows.iter().find(|row| {
            if row.city_tier != city_tier || row.experience_level != experience_level {
                return false;
            }
            let bank = normalize_position(&row.position);
            !bank.is_empty() && (normalized.contains(&bank) || bank.contains(normalized))
        })
    }

    /// 输入包含多个别名时取最长的一个,例如「资深Web前端工程师」→「前端开发」。
    fn resolve_alias(&self, normalized: &str) -> Option<&'static str> {
        let mut best: Option<&'static str> = None;
        for (alias, _target) in &self.aliases {
            if normalized.contains(alias) && best.map(|b| alias.len() > b.len()).unwrap_or(true) {
                best = Some(alias);
            }
        }
        best.and_then(|alias| self.aliases.get(alias).copied())
    }
}

pub fn normalize_position(position: &str) -> String {
    position
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && !"·•-_/（）()【】[]".contains(*c))
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn resolve_city_tier(city: &str) -> &'static str {
    let trimmed = city.trim();
    if trimmed.is_empty() {
        return "tier2";
    }
    let short = trimmed.replace('市', "");
    if tier1_cities().contains(&short.as_str()) {
        return "tier1";
    }
    if new_tier1_cities().contains(&short.as_str()) {
        return "new_tier1";
    }
    "tier2"
}

pub fn resolve_experience(experience: &str) -> &'static str {
    let text = experience.trim();
    if text.is_empty() {
        return "junior";
    }
    if text.contains("校招") || text.contains("应届") || text.contains("实习") {
        return "junior";
    }
    if let Some(years) = extract_years(text) {
        return if years >= 5 { "senior" } else if years >= 3 { "mid" } else { "junior" };
    }
    if text.contains("资深") || text.contains("高级") || text.contains("专家") {
        return "senior";
    }
    if text.contains("中级") {
        return "mid";
    }
    "junior"
}

/// 从「3年」「3.5年经验」之类的文本中提取年限。
fn extract_years(text: &str) -> Option<i32> {
    let bytes: Vec<char> = text.chars().collect();
    for (idx, ch) in bytes.iter().enumerate() {
        if *ch != '年' {
            continue;
        }
        let digits: String = bytes[..idx]
            .iter()
            .rev()
            .take_while(|c| c.is_ascii_digit() || **c == '.' || c.is_whitespace())
            .filter(|c| !c.is_whitespace())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let digits = digits.split('.').next().unwrap_or("");
        if let Ok(value) = digits.parse::<i32>() {
            return Some(value);
        }
    }
    None
}

pub fn resolve_city_factor(city: &str) -> f64 {
    let trimmed = city.trim();
    if trimmed.is_empty() {
        return DEFAULT_CITY_FACTOR;
    }
    let table = city_factors();
    if let Some(factor) = table.get(trimmed) {
        return *factor;
    }
    let short = trimmed.replace('市', "");
    table.get(short.as_str()).copied().unwrap_or(DEFAULT_CITY_FACTOR)
}

pub fn city_tier_label(tier: &str) -> &'static str {
    match tier {
        "tier1" => "一线城市",
        "new_tier1" => "新一线城市",
        _ => "二线及其他城市",
    }
}

pub fn experience_label(level: &str) -> &'static str {
    match level {
        "mid" => "3-5 年经验",
        "senior" => "5 年以上经验",
        _ => "1-3 年 / 应届",
    }
}

/// 薪资取整到百位,避免出现 22799 之类的数字。
fn round_salary(value: i64, factor: f64) -> i64 {
    (((value as f64) * factor / 100.0).round() * 100.0) as i64
}

fn tier1_cities() -> &'static [&'static str] {
    &["北京", "上海", "广州", "深圳"]
}

fn new_tier1_cities() -> &'static [&'static str] {
    &[
        "杭州", "成都", "武汉", "南京", "西安", "苏州", "天津", "长沙", "重庆", "青岛", "郑州",
        "宁波", "合肥", "东莞", "佛山", "沈阳", "无锡", "昆明", "厦门", "福州", "济南", "大连", "珠海",
    ]
}

fn city_factors() -> HashMap<&'static str, f64> {
    HashMap::from([
        ("北京", 1.00),
        ("上海", 0.95),
        ("深圳", 0.93),
        ("广州", 0.82),
        ("杭州", 0.88),
        ("苏州", 0.84),
        ("南京", 0.82),
        ("成都", 0.75),
        ("武汉", 0.74),
        ("西安", 0.68),
        ("重庆", 0.70),
        ("长沙", 0.70),
        ("青岛", 0.70),
        ("郑州", 0.68),
        ("合肥", 0.72),
        ("天津", 0.78),
        ("宁波", 0.76),
        ("无锡", 0.76),
        ("厦门", 0.74),
        ("福州", 0.72),
        ("济南", 0.70),
        ("大连", 0.68),
        ("东莞", 0.72),
        ("佛山", 0.70),
        ("珠海", 0.74),
        ("沈阳", 0.62),
        ("昆明", 0.62),
    ])
}

fn position_aliases() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("java后端", "Java后端"),
        ("java工程师", "Java后端"),
        ("java开发", "Java后端"),
        ("j2ee", "Java后端"),
        ("后端开发", "Java后端"),
        ("后端工程师", "Java后端"),
        ("golang", "Java后端"),
        ("go工程师", "Java后端"),
        ("web前端", "前端开发"),
        ("前端工程师", "前端开发"),
        ("前端开发工程师", "前端开发"),
        ("javascript", "前端开发"),
        ("react", "前端开发"),
        ("vue", "前端开发"),
        ("前端", "前端开发"),
        ("h5开发", "前端开发"),
        ("python工程师", "Python后端"),
        ("python开发", "Python后端"),
        ("django", "Python后端"),
        ("flask", "Python后端"),
        ("算法", "算法工程师"),
        ("机器学习", "算法工程师"),
        ("深度学习", "算法工程师"),
        ("nlp", "算法工程师"),
        ("测试开发", "测试工程师"),
        ("自动化测试", "测试工程师"),
        ("测试", "测试工程师"),
        ("qa", "测试工程师"),
        ("产品经理", "产品经理"),
        ("产品助理", "产品经理"),
        ("数据分析师", "数据分析师"),
        ("数据分析", "数据分析师"),
        ("商业分析", "数据分析师"),
        ("ui设计", "UI设计师"),
        ("视觉设计", "UI设计师"),
        ("交互设计", "UI设计师"),
        ("设计师", "UI设计师"),
        ("ui", "UI设计师"),
        ("用户运营", "运营"),
        ("内容运营", "运营"),
        ("活动运营", "运营"),
        ("新媒体运营", "运营"),
        ("运营", "运营"),
        ("安卓开发", "移动端开发"),
        ("android", "移动端开发"),
        ("ios", "移动端开发"),
        ("app开发", "移动端开发"),
        ("移动端", "移动端开发"),
        ("运维", "运维工程师"),
        ("devops", "运维工程师"),
        ("sre", "运维工程师"),
        ("区块链", "区块链开发"),
        ("web3", "区块链开发"),
        ("智能合约", "区块链开发"),
        ("agent开发", "Agent开发工程师"),
        ("大模型应用", "Agent开发工程师"),
        ("llm应用", "Agent开发工程师"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_lookup_uses_city_factor() {
        let planner = SalaryPlanner::embedded();
        assert!(planner.benchmark_count() >= 100);
        let beijing = planner.estimate_local("Java后端", "北京", "3年经验").unwrap();
        let guangzhou = planner.estimate_local("Java后端", "广州", "3年经验").unwrap();
        assert_eq!(beijing.p50, 24000);
        assert!(guangzhou.p50 < beijing.p50);
        assert_eq!(guangzhou.p50 % 100, 0);
    }

    #[test]
    fn alias_and_fuzzy_match() {
        let planner = SalaryPlanner::embedded();
        let alias = planner.estimate_local("资深Web前端工程师", "上海", "5年以上").unwrap();
        assert_eq!(alias.position, "前端开发");
        let fuzzy = planner.estimate_local("资深Java后端专家", "杭州", "2年").unwrap();
        assert_eq!(fuzzy.position, "Java后端");
    }

    #[test]
    fn unknown_position_needs_llm() {
        let planner = SalaryPlanner::embedded();
        assert!(planner.estimate_local("火星探测器驾驶员", "北京", "3年").is_none());
        assert_eq!(resolve_experience("应届生"), "junior");
        assert_eq!(resolve_experience("6 年经验"), "senior");
        assert_eq!(resolve_city_tier("杭州市"), "new_tier1");
    }
}

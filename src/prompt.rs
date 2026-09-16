//! Prompt 模板加载与渲染(模板与 Java 版保持一致,便于对比效果)。

use std::collections::HashMap;

use crate::error::{code, AppError, AppResult};

pub mod name {
    pub const INTERVIEWER: &str = "interviewer";
    pub const REVIEWER: &str = "reviewer";
    pub const RESUME_OPTIMIZER: &str = "resume_optimizer";
    pub const RESUME_DIAGNOSIS: &str = "resume_diagnosis";
    pub const RESUME_STYLE: &str = "resume_style";
    pub const SALARY_ESTIMATOR: &str = "salary_estimator";
    pub const STAR_PROJECT: &str = "star_project";
    pub const IMPROVEMENT_PLAN: &str = "improvement_plan";
}

pub fn template(name: &str) -> AppResult<&'static str> {
    let text = match name {
        name::INTERVIEWER => include_str!("prompts/interviewer.txt"),
        name::REVIEWER => include_str!("prompts/reviewer.txt"),
        name::RESUME_OPTIMIZER => include_str!("prompts/resume_optimizer.txt"),
        name::RESUME_DIAGNOSIS => include_str!("prompts/resume_diagnosis.txt"),
        name::RESUME_STYLE => include_str!("prompts/resume_style.txt"),
        name::SALARY_ESTIMATOR => include_str!("prompts/salary_estimator.txt"),
        name::STAR_PROJECT => include_str!("prompts/star_project.txt"),
        name::IMPROVEMENT_PLAN => include_str!("prompts/improvement_plan.txt"),
        other => return Err(AppError::internal(format!("未知 prompt 模板: {other}"))),
    };
    Ok(text)
}

/// 渲染模板:把 `{var}` 占位符替换为变量值,未提供的占位符原样保留。
pub fn render(name: &str, vars: &HashMap<&str, String>) -> AppResult<String> {
    let tpl = template(name)?;
    let mut out = tpl.to_string();
    for (key, value) in vars {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    Ok(out)
}

/// 简历风格提示:模板文件里是 `style:说明` 的多行格式,这里取对应行。
pub fn resume_style_hint(style: &str) -> String {
    let key = if style.trim().is_empty() { "general" } else { style.trim() };
    let text = template(name::RESUME_STYLE).unwrap_or("");
    for line in text.lines() {
        if let Some((name, hint)) = line.split_once(':') {
            if name.trim() == key {
                return hint.trim().to_string();
            }
        }
    }
    "输出通用求职简历:重点清晰、动词开头、避免套话、每条经历尽量量化。".to_string()
}

/// 校验 prompt 模板是否包含必需占位符(用于启动自检与测试)。
pub fn missing_placeholders(tpl_name: &str, required: &[&str]) -> AppResult<Vec<String>> {
    let text = template(tpl_name)?;
    let missing = required
        .iter()
        .filter(|key| !text.contains(&format!("{{{key}}}")))
        .map(|key| (*key).to_string())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(missing)
    } else {
        Err(AppError::business(
            code::INTERNAL,
            format!("prompt 模板 {tpl_name} 缺少占位符: {}", missing.join(", ")),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_replaces_known_placeholders() {
        let mut vars = HashMap::new();
        vars.insert("position", "Java后端".to_string());
        let rendered = render(name::INTERVIEWER, &vars).unwrap();
        assert!(rendered.contains("Java后端"));
        // 简历风格模板按 key 取值
        assert!(resume_style_hint("bigtech").contains("大厂"));
    }

    #[test]
    fn interviewer_template_has_required_placeholders() {
        let missing = missing_placeholders(
            name::INTERVIEWER,
            &["position", "difficulty", "resume_context", "conversation_history"],
        )
        .unwrap();
        assert!(missing.is_empty());
    }
}

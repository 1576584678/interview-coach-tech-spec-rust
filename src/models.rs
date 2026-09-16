//! 领域模型与接口 DTO(全部以 camelCase 输出,与前端约定一致)。

use serde::{Deserialize, Serialize};

/// 面试状态常量。
pub mod status {
    pub const ONGOING: &str = "ongoing";
    pub const COMPLETED: &str = "completed";
    pub const ABANDONED: &str = "abandoned";
}

/// 复盘状态常量。
pub mod review_status {
    pub const PROCESSING: &str = "processing";
    pub const COMPLETED: &str = "completed";
    pub const FAILED: &str = "failed";
}

/// 跳题时写入的占位回答(复盘时视为未作答)。
pub const SKIPPED_ANSWER: &str = "(候选人跳过了这道题)";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
}

/// 单条问答。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterviewQa {
    pub question_order: i32,
    pub question: String,
    pub question_type: String,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub answered_at: Option<String>,
    #[serde(default)]
    pub score: Option<i32>,
    #[serde(default)]
    pub feedback: Option<String>,
    #[serde(default)]
    pub better_answer: Option<String>,
}

impl InterviewQa {
    /// 是否真实作答(跳题、空回答都算未作答)。
    pub fn has_answer(&self) -> bool {
        match self.answer.as_deref() {
            None => false,
            Some(text) => {
                let trimmed = text.trim();
                !trimmed.is_empty() && trimmed != SKIPPED_ANSWER
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterviewSession {
    pub id: u64,
    pub position: String,
    pub position_category: String,
    pub difficulty: String,
    pub interviewer_style: String,
    pub mode: String,
    pub status: String,
    pub total_questions: i32,
    #[serde(default)]
    pub resume_id: Option<u64>,
    pub started_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub total_score: Option<i32>,
    #[serde(default)]
    pub review_status: Option<String>,
    #[serde(default)]
    pub review_error: Option<String>,
    #[serde(default)]
    pub review: Option<ReviewReport>,
    #[serde(default)]
    pub qa_list: Vec<InterviewQa>,
}

impl InterviewSession {
    pub fn last_qa(&self) -> Option<&InterviewQa> {
        self.qa_list.last()
    }

    pub fn last_qa_mut(&mut self) -> Option<&mut InterviewQa> {
        self.qa_list.last_mut()
    }

    pub fn all_answered(&self) -> bool {
        self.qa_list.len() >= self.total_questions as usize && self.qa_list.iter().all(|qa| qa.has_answer())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DimensionScores {
    pub professional: i32,
    pub expression: i32,
    pub logic: i32,
    pub communication: i32,
    pub stress: i32,
}

impl DimensionScores {
    pub const KEYS: [&'static str; 5] =
        ["professional", "expression", "logic", "communication", "stress"];

    pub fn get(&self, key: &str) -> i32 {
        match key {
            "professional" => self.professional,
            "expression" => self.expression,
            "logic" => self.logic,
            "communication" => self.communication,
            "stress" => self.stress,
            _ => 0,
        }
    }

    pub fn set(&mut self, key: &str, value: i32) {
        let value = value.clamp(0, 100);
        match key {
            "professional" => self.professional = value,
            "expression" => self.expression = value,
            "logic" => self.logic = value,
            "communication" => self.communication = value,
            "stress" => self.stress = value,
            _ => {}
        }
    }

    pub fn values(&self) -> [i32; 5] {
        [self.professional, self.expression, self.logic, self.communication, self.stress]
    }

    /// 五维平均分(四舍五入),即总分口径。
    pub fn average(&self) -> i32 {
        let sum: i32 = self.values().iter().sum();
        (sum + 2) / 5
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QaFeedback {
    pub question_order: i32,
    pub score: i32,
    #[serde(default)]
    pub feedback: String,
    #[serde(default)]
    pub better_answer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReport {
    pub overall_score: i32,
    pub dimension_scores: DimensionScores,
    #[serde(default)]
    pub strengths: String,
    #[serde(default)]
    pub weaknesses: String,
    #[serde(default)]
    pub improvement_plan: String,
    #[serde(default)]
    pub risk_warnings: String,
    pub created_at: String,
    #[serde(default)]
    pub qa_feedback: Vec<QaFeedback>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CriticalIssue {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub fix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeDiagnosis {
    #[serde(default)]
    pub match_score: i32,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub critical_issues: Vec<CriticalIssue>,
    #[serde(default)]
    pub missing_skills: Vec<String>,
    #[serde(default)]
    pub follow_up_questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOptimization {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub original: String,
    #[serde(default)]
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizationSuggestion {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub original: String,
    #[serde(default)]
    pub optimized: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub impact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeRisk {
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub original: String,
    #[serde(default)]
    pub risk: String,
    #[serde(default)]
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResumeDiagnostics {
    #[serde(default)]
    pub missing_skills: Vec<String>,
    #[serde(default)]
    pub risks: Vec<ResumeRisk>,
    #[serde(default)]
    pub follow_up_questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeOptimization {
    #[serde(default)]
    pub match_score: i32,
    #[serde(default)]
    pub match_analysis: String,
    #[serde(default)]
    pub projects: Vec<ProjectOptimization>,
    #[serde(default)]
    pub summary_advice: String,
    #[serde(default)]
    pub suggestions: Vec<OptimizationSuggestion>,
    #[serde(default)]
    pub optimized_resume: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub diagnostics: ResumeDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resume {
    pub id: u64,
    pub file_name: String,
    pub raw_text: String,
    #[serde(default)]
    pub target_position: Option<String>,
    #[serde(default)]
    pub match_score: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub diagnosis: Option<ResumeDiagnosis>,
    #[serde(default)]
    pub diagnosis_at: Option<String>,
    #[serde(default)]
    pub optimization: Option<ResumeOptimization>,
    #[serde(default)]
    pub optimized_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusArea {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub exercises: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyPlan {
    #[serde(default)]
    pub week: i32,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub tasks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImprovementPlan {
    #[serde(default)]
    pub focus_areas: Vec<FocusArea>,
    #[serde(default)]
    pub weekly_plan: Vec<WeeklyPlan>,
    #[serde(default)]
    pub expected_gains: String,
    #[serde(default)]
    pub generated_at: String,
    /// 生成时所依据的面试场次数量,用于判断是否需要重新生成。
    #[serde(default)]
    pub based_on_sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScorePoint {
    pub session_id: u64,
    pub position: String,
    pub score: i32,
    pub completed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeakPoint {
    pub question_type: String,
    pub label: String,
    pub average_score: i32,
    pub count: usize,
    #[serde(default)]
    pub sample_questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterviewStats {
    pub total_sessions: usize,
    pub completed_sessions: usize,
    #[serde(default)]
    pub avg_score: Option<i32>,
    #[serde(default)]
    pub max_score: Option<i32>,
    #[serde(default)]
    pub latest_score: Option<i32>,
    #[serde(default)]
    pub trend: Vec<ScorePoint>,
    #[serde(default)]
    pub dimension_averages: DimensionScores,
    #[serde(default)]
    pub weak_points: Vec<WeakPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SalaryRange {
    pub position: String,
    pub city: String,
    pub city_tier: String,
    pub experience_level: String,
    pub p25: i64,
    pub p50: i64,
    pub p75: i64,
    /// high = 命中本地基准表;low = 大模型兜底估计。
    pub confidence: String,
    pub source: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// 本地存储的完整数据库(单个 JSON 文件,便于备份/迁移)。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Database {
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub sessions: Vec<InterviewSession>,
    #[serde(default)]
    pub resumes: Vec<Resume>,
    #[serde(default)]
    pub improvement_plan: Option<ImprovementPlan>,
}

impl Database {
    pub fn next_id(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }
}

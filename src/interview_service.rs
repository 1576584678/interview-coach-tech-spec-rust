//! 面试流程:出题、答题、跳题、结束。

use std::collections::HashMap;

use crate::error::{code, AppError, AppResult};
use crate::models::{
    status, ChatMessage, InterviewQa, InterviewSession, WeakPoint, SKIPPED_ANSWER,
};
use crate::prompt::{self, name as prompt_name};
use crate::question_bank::QuestionBank;
use crate::state::SharedState;
use crate::util::now_iso;

/// 单次回答最大长度,防止把超长文本塞进 prompt。
pub const MAX_ANSWER_CHARS: usize = 4000;
/// 对话历史在 prompt 中最多保留的条数。
const HISTORY_LIMIT: usize = 20;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResponse {
    pub session_id: u64,
    pub first_question: String,
    pub question_type: String,
    pub total_questions: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_question: Option<String>,
    pub question_type: String,
    pub question_order: i32,
    pub is_last: bool,
}

/// 流式接口的事件。
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Delta(String),
    Done(AnswerResponse),
    Error { code: i32, message: String },
}

/// 请求开始面试所需的参数(由路由层从 DTO 转换)。
pub struct StartRequest {
    pub position: String,
    pub position_category: String,
    pub difficulty: String,
    pub style: String,
    pub mode: String,
    pub resume_id: Option<u64>,
    pub total_questions: i32,
}

pub fn resolve_style(style: &str) -> &'static str {
    match style.trim() {
        "strict" | "stress" => match style.trim() {
            "strict" => "strict",
            _ => "stress",
        },
        _ => "friendly",
    }
}

pub fn resolve_mode(mode: &str) -> &'static str {
    if mode.trim() == "real" {
        "real"
    } else {
        "normal"
    }
}

pub fn resolve_difficulty(difficulty: &str) -> &'static str {
    match difficulty.trim() {
        "easy" | "hard" => match difficulty.trim() {
            "easy" => "easy",
            _ => "hard",
        },
        _ => "normal",
    }
}

pub fn style_hint(style: &str) -> &'static str {
    match style {
        "strict" => "面试风格:严谨挑剔,对回答中的漏洞和模糊之处进行追问,标准严格。",
        "stress" => "面试风格:压力面试,语气直接,连环追问,考察候选人在压力下的反应。",
        _ => "面试风格:友好亲和,以鼓励为主,提问循序渐进。",
    }
}

pub fn mode_hint(mode: &str) -> &'static str {
    if mode == "real" {
        "面试模式: 真实面试模拟。像真人面试官一样自然交流,可以对冗长、含糊或背模板的回答直接打断并追问;候选人可能跳过当前题目,不要纠结,自然切换话题;整体保持适度压迫感。"
    } else {
        "面试模式: 标准训练。循序渐进,以帮助候选人练习为主。"
    }
}

pub fn category_hint(category: &str) -> &'static str {
    match category {
        "backend" => "重点考察:编程语言基础、数据库、分布式、系统设计、高并发场景。",
        "frontend" => "重点考察:JavaScript/框架原理、浏览器机制、性能优化、工程化。",
        "product" => "重点考察:需求分析、用户思维、数据驱动、项目推进、商业理解。",
        "data" => "重点考察:SQL 能力、统计学基础、业务分析思维、可视化表达。",
        "design" => "重点考察:设计方法论、用户体验、作品集深挖、跨部门协作。",
        "operation" => "重点考察:增长思维、数据分析、活动策划、用户运营。",
        "agent" => "重点考察:LLM 原理与能力边界、Prompt 工程、RAG/Agent 架构、工具调用、向量检索、多轮对话状态管理。",
        "devops" => "重点考察:Linux 与网络基础、容器与编排(Docker/K8s)、CI/CD 流水线、监控告警、云原生、自动化运维脚本。",
        "web3" => "重点考察:区块链基础、智能合约(Solidity)、DeFi/NFT 协议、密码学基础、链上数据分析、安全审计。",
        _ => "",
    }
}

/// 固定节奏的题型:1-3 基础,4-7 项目,8-10 深挖,11 开放。
pub fn base_question_type(order: i32) -> &'static str {
    if order <= 3 {
        "basic"
    } else if order <= 7 {
        "project"
    } else if order <= 10 {
        "deep"
    } else {
        "open"
    }
}

/// 自适应题型:第 4 题起每隔一题穿插一道历史薄弱题型。
pub fn adaptive_question_type(order: i32, weak_points: &[WeakPoint]) -> String {
    let base = base_question_type(order);
    if order < 4 || weak_points.is_empty() {
        return base.to_string();
    }
    if (order - 4) % 2 == 0 {
        let index = ((order - 4) / 2) as usize % weak_points.len();
        let candidate = weak_points[index].question_type.clone();
        if !candidate.is_empty() {
            return candidate;
        }
    }
    base.to_string()
}

/// 把问答列表展开成对话历史(供 prompt 使用)。
pub fn history_messages(session: &InterviewSession) -> Vec<ChatMessage> {
    let mut messages = Vec::with_capacity(session.qa_list.len() * 2);
    for qa in &session.qa_list {
        messages.push(ChatMessage::assistant(qa.question.clone()));
        if let Some(answer) = qa.answer.as_deref() {
            messages.push(ChatMessage::user(answer.to_string()));
        }
    }
    messages
}

fn format_history(messages: &[ChatMessage]) -> String {
    if messages.is_empty() {
        return "无(面试刚开始)".to_string();
    }
    let start = messages.len().saturating_sub(HISTORY_LIMIT);
    let mut out = String::new();
    for msg in &messages[start..] {
        let label = if msg.role == "assistant" { "面试官" } else { "候选人" };
        out.push_str(&format!("{label}: {}\n", msg.content));
    }
    out
}

/// 拼装面试官 prompt 变量。
fn interviewer_vars(
    state: &SharedState,
    session: &InterviewSession,
    weak_points: &[WeakPoint],
    conversation_history: &str,
) -> AppResult<HashMap<&'static str, String>> {
    let asked: Vec<String> = session.qa_list.iter().map(|qa| qa.question.clone()).collect();
    let mut bank_questions = state.question_bank.build_hint(&session.position_category, &asked);
    bank_questions.push_str(&QuestionBank::build_weak_point_hint(weak_points));
    bank_questions.push_str(&diagnosis_hint(state, session.resume_id));

    let mut vars: HashMap<&'static str, String> = HashMap::new();
    vars.insert("position", session.position.clone());
    vars.insert("difficulty", session.difficulty.clone());
    vars.insert("style_hint", style_hint(&session.interviewer_style).to_string());
    vars.insert("mode_hint", mode_hint(&session.mode).to_string());
    vars.insert("category_hint", category_hint(&session.position_category).to_string());
    vars.insert("bank_questions", bank_questions);
    vars.insert("resume_context", resume_context(state, session.resume_id));
    vars.insert("conversation_history", conversation_history.to_string());
    Ok(vars)
}

/// 简历诊断里的追问题,作为出题优先级最高的参考。
fn diagnosis_hint(state: &SharedState, resume_id: Option<u64>) -> String {
    let Some(resume_id) = resume_id else { return String::new() };
    let questions = state.store.read(|db| {
        db.resumes
            .iter()
            .find(|r| r.id == resume_id)
            .and_then(|r| r.diagnosis.as_ref())
            .map(|d| d.follow_up_questions.clone())
            .unwrap_or_default()
    });
    if questions.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n\n## 简历诊断追问题\n");
    for (index, question) in questions.iter().enumerate() {
        out.push_str(&format!("{}. {question}\n", index + 1));
    }
    out.push_str("出题规则:这些是诊断后发现的高风险追问点,应优先作为项目/技术深挖问题。\n");
    out
}

pub fn resume_context(state: &SharedState, resume_id: Option<u64>) -> String {
    let Some(resume_id) = resume_id else {
        return "无简历信息".to_string();
    };
    state.store.read(|db| {
        db.resumes
            .iter()
            .find(|r| r.id == resume_id)
            .filter(|r| !r.raw_text.trim().is_empty())
            .map(|r| format!("候选人简历:\n{}", crate::util::truncate_chars(&r.raw_text, 12_000)))
            .unwrap_or_else(|| "无简历信息".to_string())
    })
}

/// 开始面试:生成第一题并落库。
pub async fn start(state: &SharedState, req: StartRequest) -> AppResult<StartResponse> {
    let position = req.position.trim().to_string();
    if position.is_empty() {
        return Err(AppError::bad_request("岗位不能为空"));
    }
    if position.chars().count() > 100 {
        return Err(AppError::bad_request("岗位名称过长"));
    }
    let total_questions = state.config_snapshot().interview.total_questions as i32;
    let total_questions = if req.total_questions > 0 { req.total_questions } else { total_questions };

    let weak_points = crate::analysis::weak_points(&state.store);
    let mut session = InterviewSession {
        id: 0,
        position: position.clone(),
        position_category: req.position_category.trim().to_lowercase(),
        difficulty: resolve_difficulty(&req.difficulty).to_string(),
        interviewer_style: resolve_style(&req.style).to_string(),
        mode: resolve_mode(&req.mode).to_string(),
        status: status::ONGOING.to_string(),
        total_questions,
        resume_id: req.resume_id,
        started_at: now_iso(),
        completed_at: None,
        total_score: None,
        review_status: None,
        review_error: None,
        review: None,
        qa_list: Vec::new(),
    };

    let vars = interviewer_vars(state, &session, &weak_points, "无(面试刚开始)")?;
    let system_prompt = prompt::render(prompt_name::INTERVIEWER, &vars)?;

    // 已有简历诊断追问题:直接作为第一题,保证问到点子上
    let diagnosis_questions = state.store.read(|db| {
        session
            .resume_id
            .and_then(|id| db.resumes.iter().find(|r| r.id == id))
            .and_then(|r| r.diagnosis.as_ref())
            .map(|d| d.follow_up_questions.clone())
            .unwrap_or_default()
    });
    let (first_question, first_type) = if let Some(question) = diagnosis_questions.first() {
        (question.clone(), "diagnosis".to_string())
    } else {
        let llm = state.llm()?;
        let question = llm
            .chat(&system_prompt, "请开始面试,先让候选人做自我介绍。", &[])
            .await?;
        (question.trim().to_string(), base_question_type(1).to_string())
    };
    if first_question.is_empty() {
        return Err(AppError::business(code::LLM_EMPTY, "面试启动失败,请重试"));
    }

    session.qa_list.push(InterviewQa {
        question_order: 1,
        question: first_question.clone(),
        question_type: first_type.clone(),
        answer: None,
        answered_at: None,
        score: None,
        feedback: None,
        better_answer: None,
    });
    let session_id = state.store.write(|db| {
        let id = db.next_id();
        session.id = id;
        db.sessions.push(session);
        Ok(id)
    })?;

    Ok(StartResponse {
        session_id,
        first_question,
        question_type: first_type,
        total_questions,
    })
}

/// 一次回答的准备结果:需要生成下一题时返回 Some,已是最后一题时返回 None(此时答案已落库)。
pub struct AnswerPlan {
    pub session_id: u64,
    /// 本次回答对应的题号;提交时用来确认状态没有被并发推进过。
    pub answered_order: i32,
    /// 本次回答的正文,提交成功时才会写入本地数据。
    pub answer: String,
    pub next_order: i32,
    pub next_type: String,
    pub is_last: bool,
    pub system_prompt: String,
    pub history: Vec<ChatMessage>,
}

/// 第一步:校验当前题并准备好下一题的 prompt(不落库)。
///
/// 答案只有在大模型成功生成下一题后才由 `commit_answer` 写入,避免出现
/// 「答案已存、下一题却没生成」的半完成状态把整场面试卡死。
pub fn prepare_answer(
    state: &SharedState,
    session_id: u64,
    answer: Option<&str>,
) -> AppResult<Option<AnswerPlan>> {
    let raw = answer.map(|a| a.to_string()).unwrap_or_else(|| SKIPPED_ANSWER.to_string());
    if raw.chars().count() > MAX_ANSWER_CHARS {
        return Err(AppError::bad_request(format!("回答过长,请控制在 {MAX_ANSWER_CHARS} 字以内")));
    }

    let session = state
        .store
        .read(|db| db.sessions.iter().find(|s| s.id == session_id).cloned())
        .ok_or_else(|| AppError::not_found("面试会话不存在"))?;
    if session.status != status::ONGOING {
        return Err(AppError::business(code::INTERVIEW_FINISHED, "面试已结束"));
    }
    let last = session.last_qa().ok_or_else(|| AppError::internal("面试问答记录缺失"))?;
    if last.answer.is_some() {
        return Err(AppError::bad_request("当前题目已经回答过了"));
    }
    let current_order = last.question_order;

    // 已经是最后一题:不需要再问大模型,直接把答案落库
    if current_order >= session.total_questions {
        store_answer(state, session_id, current_order, &raw)?;
        return Ok(None);
    }

    let weak_points = crate::analysis::weak_points(&state.store);
    let next_order = current_order + 1;
    let next_type = adaptive_question_type(next_order, &weak_points);
    // 出题用的历史里要包含这一次的回答
    let mut with_answer = session.clone();
    if let Some(qa) = with_answer.last_qa_mut() {
        qa.answer = Some(raw.clone());
        qa.answered_at = Some(now_iso());
    }
    let history = history_messages(&with_answer);
    let vars = interviewer_vars(state, &with_answer, &weak_points, &format_history(&history))?;
    Ok(Some(AnswerPlan {
        session_id,
        answered_order: current_order,
        answer: raw,
        next_order,
        next_type,
        is_last: next_order >= with_answer.total_questions,
        system_prompt: prompt::render(prompt_name::INTERVIEWER, &vars)?,
        history,
    }))
}

/// 第二步:把「这次回答」与「AI 生成的新问题」一次性落库,保证两者同生共死。
pub fn commit_answer(state: &SharedState, plan: &AnswerPlan, question: &str) -> AppResult<AnswerResponse> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err(AppError::business(code::LLM_EMPTY, "生成下一题失败,请重试"));
    }
    state.store.write(|db| {
        let session = db
            .sessions
            .iter_mut()
            .find(|s| s.id == plan.session_id)
            .ok_or_else(|| AppError::not_found("面试会话不存在"))?;
        if session.status != status::ONGOING {
            return Err(AppError::business(code::INTERVIEW_FINISHED, "面试已结束"));
        }
        let Some(qa) = session.qa_list.last_mut() else {
            return Err(AppError::internal("面试问答记录缺失"));
        };
        // 期间被其他请求推进过:拒绝提交,避免出现重复题目
        if qa.answer.is_some() || qa.question_order != plan.answered_order {
            return Err(AppError::bad_request("当前题目已经回答过了"));
        }
        qa.answer = Some(plan.answer.clone());
        qa.answered_at = Some(now_iso());
        session.qa_list.push(InterviewQa {
            question_order: plan.next_order,
            question: question.clone(),
            question_type: plan.next_type.clone(),
            answer: None,
            answered_at: None,
            score: None,
            feedback: None,
            better_answer: None,
        });
        Ok(())
    })?;
    Ok(AnswerResponse {
        next_question: Some(question),
        question_type: plan.next_type.clone(),
        question_order: plan.next_order,
        is_last: plan.is_last,
    })
}

/// 最后一题(不需要下一题时)只落答案。
fn store_answer(state: &SharedState, session_id: u64, order: i32, answer: &str) -> AppResult<()> {
    state.store.write(|db| {
        let session = db
            .sessions
            .iter_mut()
            .find(|s| s.id == session_id)
            .ok_or_else(|| AppError::not_found("面试会话不存在"))?;
        if session.status != status::ONGOING {
            return Err(AppError::business(code::INTERVIEW_FINISHED, "面试已结束"));
        }
        let Some(qa) = session.qa_list.last_mut() else {
            return Err(AppError::internal("面试问答记录缺失"));
        };
        if qa.answer.is_some() || qa.question_order != order {
            return Err(AppError::bad_request("当前题目已经回答过了"));
        }
        qa.answer = Some(answer.to_string());
        qa.answered_at = Some(now_iso());
        Ok(())
    })
}

/// 「最后一题已答完、没有下一题」时返回给前端的收尾响应。
pub fn answered_last(state: &SharedState, session_id: u64) -> AnswerResponse {
    let (order, qa_type) = state.store.read(|db| {
        db.sessions
            .iter()
            .find(|s| s.id == session_id)
            .and_then(|s| s.last_qa().map(|qa| (qa.question_order, qa.question_type.clone())))
            .unwrap_or((0, "open".to_string()))
    });
    AnswerResponse {
        next_question: None,
        question_type: qa_type,
        question_order: order,
        is_last: true,
    }
}

/// 非流式回答当前问题。
pub async fn answer(state: &SharedState, session_id: u64, answer: Option<&str>) -> AppResult<AnswerResponse> {
    let Some(plan) = prepare_answer(state, session_id, answer)? else {
        return Ok(answered_last(state, session_id));
    };

    let llm = state.llm()?;
    let question = llm
        .chat(&plan.system_prompt, "请基于候选人的回答提出下一个问题。", &plan.history)
        .await?;
    commit_answer(state, &plan, &question)
}

pub fn complete(state: &SharedState, session_id: u64) -> AppResult<()> {
    state.store.write(|db| {
        let session = db
            .sessions
            .iter_mut()
            .find(|s| s.id == session_id)
            .ok_or_else(|| AppError::not_found("面试会话不存在"))?;
        if session.status == status::COMPLETED {
            return Ok(());
        }
        if !session.all_answered() {
            return Err(AppError::bad_request("所有题目完成后才能结束面试"));
        }
        session.status = status::COMPLETED.to_string();
        session.completed_at = Some(now_iso());
        Ok(())
    })
}

pub fn abandon(state: &SharedState, session_id: u64) -> AppResult<()> {
    state.store.write(|db| {
        let session = db
            .sessions
            .iter_mut()
            .find(|s| s.id == session_id)
            .ok_or_else(|| AppError::not_found("面试会话不存在"))?;
        session.status = status::ABANDONED.to_string();
        session.completed_at = Some(now_iso());
        Ok(())
    })
}

//! 面试相关接口。

use std::convert::Infallible;

use async_stream::stream;
use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::api::{ok_empty, ApiBody};
use crate::error::{AppResult, AppError};
use crate::interview_service::{self, AnswerResponse, StartResponse, StreamEvent};
use crate::models::{review_status, status, InterviewSession};
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRequestDto {
    pub position: String,
    #[serde(default)]
    pub position_category: String,
    #[serde(default)]
    pub difficulty: String,
    #[serde(default)]
    pub style: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub resume_id: Option<u64>,
    #[serde(default)]
    pub total_questions: i32,
}

pub async fn start(
    State(state): State<SharedState>,
    Json(dto): Json<StartRequestDto>,
) -> AppResult<Json<ApiBody<StartResponse>>> {
    let defaults = state.config_snapshot().interview;
    let req = interview_service::StartRequest {
        position: dto.position,
        position_category: if dto.position_category.trim().is_empty() {
            defaults.default_position_category.clone()
        } else {
            dto.position_category
        },
        difficulty: if dto.difficulty.trim().is_empty() { defaults.default_difficulty.clone() } else { dto.difficulty },
        style: if dto.style.trim().is_empty() { defaults.default_style.clone() } else { dto.style },
        mode: if dto.mode.trim().is_empty() { defaults.default_mode.clone() } else { dto.mode },
        resume_id: dto.resume_id,
        total_questions: dto.total_questions,
    };
    let response = interview_service::start(&state, req).await?;
    Ok(Json(ApiBody::ok(response)))
}

#[derive(Debug, Deserialize)]
pub struct AnswerRequestDto {
    #[serde(default)]
    pub answer: String,
}

pub async fn answer(
    State(state): State<SharedState>,
    Path(session_id): Path<u64>,
    Json(dto): Json<AnswerRequestDto>,
) -> AppResult<Json<ApiBody<AnswerResponse>>> {
    if dto.answer.trim().is_empty() {
        return Err(AppError::bad_request("回答不能为空"));
    }
    let response = interview_service::answer(&state, session_id, Some(&dto.answer)).await?;
    Ok(Json(ApiBody::ok(response)))
}

pub async fn skip(
    State(state): State<SharedState>,
    Path(session_id): Path<u64>,
) -> AppResult<Json<ApiBody<AnswerResponse>>> {
    let response = interview_service::answer(&state, session_id, None).await?;
    Ok(Json(ApiBody::ok(response)))
}

/// 流式回答:先通过 SSE 推送下一题增量文本,最后推送 done 事件。
pub async fn answer_stream(
    State(state): State<SharedState>,
    Path(session_id): Path<u64>,
    Json(dto): Json<AnswerRequestDto>,
) -> AppResult<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>> {
    if dto.answer.trim().is_empty() {
        return Err(AppError::bad_request("回答不能为空"));
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<StreamEvent>();
    let task_state = state.clone();
    let answer = dto.answer.clone();
    tokio::spawn(async move {
        run_stream_answer(task_state, session_id, answer, tx.clone()).await;
    });

    let body = stream! {
        while let Some(event) = rx.recv().await {
            let rendered = match event {
                StreamEvent::Delta(text) => {
                    Event::default().event("delta").data(serde_json::json!({ "text": text }).to_string())
                }
                StreamEvent::Done(response) => {
                    Event::default().event("done").data(
                        serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string()),
                    )
                }
                StreamEvent::Error { code, message } => {
                    Event::default().event("error").data(
                        serde_json::json!({ "code": code, "message": message }).to_string(),
                    )
                }
            };
            yield Ok::<Event, Infallible>(rendered);
        }
    };
    Ok(Sse::new(body).keep_alive(KeepAlive::default()))
}

async fn run_stream_answer(
    state: SharedState,
    session_id: u64,
    answer: String,
    tx: mpsc::UnboundedSender<StreamEvent>,
) {
    let plan = match interview_service::prepare_answer(&state, session_id, Some(&answer)) {
        Ok(Some(plan)) => plan,
        Ok(None) => {
            let _ = tx.send(StreamEvent::Done(interview_service::answered_last(&state, session_id)));
            return;
        }
        Err(err) => {
            let _ = tx.send(StreamEvent::Error { code: err.code(), message: err.message() });
            return;
        }
    };

    let llm = match state.llm() {
        Ok(llm) => llm,
        Err(err) => {
            let _ = tx.send(StreamEvent::Error { code: err.code(), message: err.message() });
            return;
        }
    };
    let sender = tx.clone();
    let result = llm
        .chat_stream(&plan.system_prompt, "请基于候选人的回答提出下一个问题。", &plan.history, move |delta| {
            let _ = sender.send(StreamEvent::Delta(delta.to_string()));
        })
        .await;

    match result {
        Ok(question) => match interview_service::commit_answer(&state, &plan, &question) {
            Ok(response) => {
                let _ = tx.send(StreamEvent::Done(response));
            }
            Err(err) => {
                let _ = tx.send(StreamEvent::Error { code: err.code(), message: err.message() });
            }
        },
        Err(err) => {
            let _ = tx.send(StreamEvent::Error { code: err.code(), message: err.message() });
        }
    }
}

pub async fn complete(
    State(state): State<SharedState>,
    Path(session_id): Path<u64>,
) -> AppResult<Json<ApiBody<()>>> {
    interview_service::complete(&state, session_id)?;
    crate::review_service::spawn_review(&state, session_id);
    Ok(ok_empty())
}

pub async fn abandon(
    State(state): State<SharedState>,
    Path(session_id): Path<u64>,
) -> AppResult<Json<ApiBody<()>>> {
    interview_service::abandon(&state, session_id)?;
    Ok(ok_empty())
}

pub async fn detail(
    State(state): State<SharedState>,
    Path(session_id): Path<u64>,
) -> AppResult<Json<ApiBody<InterviewSession>>> {
    let session = load_session(&state, session_id)?;
    Ok(Json(ApiBody::ok(session)))
}

/// 复盘结果:未完成时返回当前状态,前端据此轮询。
pub async fn result(
    State(state): State<SharedState>,
    Path(session_id): Path<u64>,
) -> AppResult<Json<ApiBody<InterviewSession>>> {
    let session = load_session(&state, session_id)?;
    if session.status == status::COMPLETED
        && session.review.is_none()
        && session.review_status.is_none()
    {
        crate::review_service::spawn_review(&state, session_id);
        return Ok(Json(ApiBody::ok(load_session(&state, session_id)?)));
    }
    Ok(Json(ApiBody::ok(session)))
}

pub async fn retry_review(
    State(state): State<SharedState>,
    Path(session_id): Path<u64>,
) -> AppResult<Json<ApiBody<InterviewSession>>> {
    let session = load_session(&state, session_id)?;
    if session.review.is_some() {
        return Ok(Json(ApiBody::ok(session)));
    }
    if session.review_status.as_deref() == Some(review_status::FAILED) {
        state.store.write(|db| {
            if let Some(session) = db.sessions.iter_mut().find(|s| s.id == session_id) {
                session.review_status = None;
                session.review_error = None;
            }
            Ok(())
        })?;
    }
    crate::review_service::spawn_review(&state, session_id);
    Ok(Json(ApiBody::ok(load_session(&state, session_id)?)))
}

fn load_session(state: &SharedState, session_id: u64) -> AppResult<InterviewSession> {
    state
        .store
        .read(|db| db.sessions.iter().find(|s| s.id == session_id).cloned())
        .ok_or_else(|| AppError::not_found("面试会话不存在"))
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    #[serde(default)]
    pub page: Option<usize>,
    #[serde(default)]
    pub size: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub session_id: u64,
    pub position: String,
    pub position_category: String,
    pub difficulty: String,
    pub interviewer_style: String,
    pub mode: String,
    pub status: String,
    pub total_score: Option<i32>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub question_count: usize,
    pub answered_count: usize,
    pub review_status: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub list: Vec<HistoryItem>,
    pub total: usize,
    pub page: usize,
    pub size: usize,
}

pub async fn history(
    State(state): State<SharedState>,
    Query(query): Query<HistoryQuery>,
) -> AppResult<Json<ApiBody<HistoryPage>>> {
    let page = query.page.unwrap_or(1).max(1);
    let size = query.size.unwrap_or(10).clamp(1, 100);
    let (items, total) = state.store.read(|db| {
        let mut sessions: Vec<_> = db.sessions.iter().collect();
        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        let total = sessions.len();
        let start = (page - 1) * size;
        let list = sessions
            .into_iter()
            .skip(start)
            .take(size)
            .map(|session| HistoryItem {
                session_id: session.id,
                position: session.position.clone(),
                position_category: session.position_category.clone(),
                difficulty: session.difficulty.clone(),
                interviewer_style: session.interviewer_style.clone(),
                mode: session.mode.clone(),
                status: session.status.clone(),
                total_score: session.total_score,
                started_at: session.started_at.clone(),
                completed_at: session.completed_at.clone(),
                question_count: session.qa_list.len(),
                answered_count: session.qa_list.iter().filter(|qa| qa.has_answer()).count(),
                review_status: session.review_status.clone(),
            })
            .collect();
        (list, total)
    });
    Ok(Json(ApiBody::ok(HistoryPage { list: items, total, page, size })))
}

pub async fn stats(
    State(state): State<SharedState>,
) -> AppResult<Json<ApiBody<crate::models::InterviewStats>>> {
    Ok(Json(ApiBody::ok(crate::analysis::compute_stats(&state.store))))
}

pub async fn delete_session(
    State(state): State<SharedState>,
    Path(session_id): Path<u64>,
) -> AppResult<Json<ApiBody<()>>> {
    let removed = state.store.write(|db| {
        let before = db.sessions.len();
        db.sessions.retain(|s| s.id != session_id);
        Ok(before != db.sessions.len())
    })?;
    if !removed {
        return Err(AppError::not_found("面试会话不存在"));
    }
    Ok(ok_empty())
}

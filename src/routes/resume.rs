//! 简历接口:上传/粘贴、诊断、优化、STAR 改写。

use axum::extract::{Multipart, Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::{ok_empty, ApiBody};
use crate::error::{code, AppError, AppResult};
use crate::models::{Resume, ResumeDiagnosis, ResumeOptimization};
use crate::state::SharedState;
use crate::util::now_iso;

/// 上传/粘贴后返回的简历摘要。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeView {
    pub resume_id: u64,
    pub file_name: String,
    pub target_position: Option<String>,
    pub match_score: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
    pub char_count: usize,
    pub has_diagnosis: bool,
    pub has_optimization: bool,
    pub diagnosis_at: Option<String>,
    pub optimized_at: Option<String>,
}

fn to_view(resume: &Resume) -> ResumeView {
    ResumeView {
        resume_id: resume.id,
        file_name: resume.file_name.clone(),
        target_position: resume.target_position.clone(),
        match_score: resume.match_score,
        created_at: resume.created_at.clone(),
        updated_at: resume.updated_at.clone(),
        char_count: resume.raw_text.chars().count(),
        has_diagnosis: resume.diagnosis.is_some(),
        has_optimization: resume.optimization.is_some(),
        diagnosis_at: resume.diagnosis_at.clone(),
        optimized_at: resume.optimized_at.clone(),
    }
}

fn save_resume(
    state: &SharedState,
    file_name: String,
    raw_text: String,
    target_position: Option<String>,
) -> AppResult<Resume> {
    if raw_text.trim().chars().count() < 20 {
        return Err(AppError::bad_request("简历内容太短,请补充完整后再保存"));
    }
    let now = now_iso();
    let resume = Resume {
        id: 0,
        file_name,
        raw_text,
        target_position: target_position.filter(|p| !p.trim().is_empty()),
        match_score: None,
        created_at: now.clone(),
        updated_at: now,
        diagnosis: None,
        diagnosis_at: None,
        optimization: None,
        optimized_at: None,
    };
    state.store.write(move |db| {
        let mut resume = resume;
        let id = db.next_id();
        resume.id = id;
        let stored = resume.clone();
        db.resumes.push(resume);
        Ok(stored)
    })
}

/// multipart 上传:字段 file(必填)、targetPosition(可选)。
pub async fn upload(
    State(state): State<SharedState>,
    mut multipart: Multipart,
) -> AppResult<Json<ApiBody<ResumeView>>> {
    let mut file_name = String::new();
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut target_position: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("文件上传解析失败: {e}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => {
                file_name = field.file_name().unwrap_or("resume").to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::bad_request(format!("读取文件内容失败: {e}")))?;
                file_bytes = Some(data.to_vec());
            }
            "targetPosition" => {
                let value = field.text().await.unwrap_or_default();
                if !value.trim().is_empty() {
                    target_position = Some(value.trim().to_string());
                }
            }
            _ => {}
        }
    }

    let bytes = file_bytes.ok_or_else(|| AppError::bad_request("请选择要上传的简历文件"))?;
    let text = crate::file_parser::extract(&file_name, &bytes)?;
    let resume = save_resume(&state, file_name, text, target_position)?;
    Ok(Json(ApiBody::ok(to_view(&resume))))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFromTextRequest {
    pub raw_text: String,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub target_position: Option<String>,
}

/// 直接粘贴简历文本(单机版最省事的入口)。
pub async fn create_from_text(
    State(state): State<SharedState>,
    Json(req): Json<CreateFromTextRequest>,
) -> AppResult<Json<ApiBody<ResumeView>>> {
    let file_name = req
        .file_name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("粘贴简历-{}.txt", crate::util::now_iso().replace(':', "")));
    let resume = save_resume(&state, file_name, req.raw_text, req.target_position)?;
    Ok(Json(ApiBody::ok(to_view(&resume))))
}

pub async fn list(State(state): State<SharedState>) -> AppResult<Json<ApiBody<Vec<ResumeView>>>> {
    let list = state.store.read(|db| {
        let mut resumes: Vec<_> = db.resumes.iter().collect();
        resumes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        resumes.into_iter().map(to_view).collect::<Vec<_>>()
    });
    Ok(Json(ApiBody::ok(list)))
}

pub async fn detail(
    State(state): State<SharedState>,
    Path(resume_id): Path<u64>,
) -> AppResult<Json<ApiBody<Resume>>> {
    Ok(Json(ApiBody::ok(load_resume(&state, resume_id)?)))
}

fn load_resume(state: &SharedState, resume_id: u64) -> AppResult<Resume> {
    state
        .store
        .read(|db| db.resumes.iter().find(|r| r.id == resume_id).cloned())
        .ok_or_else(|| AppError::not_found("简历不存在"))
}

pub async fn delete(
    State(state): State<SharedState>,
    Path(resume_id): Path<u64>,
) -> AppResult<Json<ApiBody<()>>> {
    let removed = state.store.write(|db| {
        let before = db.resumes.len();
        db.resumes.retain(|r| r.id != resume_id);
        Ok(before != db.resumes.len())
    })?;
    if !removed {
        return Err(AppError::not_found("简历不存在"));
    }
    Ok(ok_empty())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnoseRequest {
    #[serde(default)]
    pub target_position: Option<String>,
    #[serde(default)]
    pub jd_text: Option<String>,
}

pub async fn diagnose(
    State(state): State<SharedState>,
    Path(resume_id): Path<u64>,
    Json(req): Json<DiagnoseRequest>,
) -> AppResult<Json<ApiBody<ResumeDiagnosis>>> {
    let position = req.target_position.or_else(|| {
        state.store.read(|db| {
            db.resumes.iter().find(|r| r.id == resume_id).and_then(|r| r.target_position.clone())
        })
    });
    let diagnosis = crate::resume_service::diagnose(&state, resume_id, position, req.jd_text).await?;
    Ok(Json(ApiBody::ok(diagnosis)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeRequest {
    #[serde(default)]
    pub target_position: Option<String>,
    #[serde(default)]
    pub jd_text: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
}

pub async fn optimize(
    State(state): State<SharedState>,
    Path(resume_id): Path<u64>,
    Json(req): Json<OptimizeRequest>,
) -> AppResult<Json<ApiBody<ResumeOptimization>>> {
    let position = req.target_position.or_else(|| {
        state.store.read(|db| {
            db.resumes.iter().find(|r| r.id == resume_id).and_then(|r| r.target_position.clone())
        })
    });
    let optimization =
        crate::resume_service::optimize(&state, resume_id, position, req.jd_text, req.style).await?;
    Ok(Json(ApiBody::ok(optimization)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StarRequest {
    pub project_text: String,
    #[serde(default)]
    pub target_position: Option<String>,
    #[serde(default)]
    pub jd_text: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StarResponse {
    pub markdown: String,
}

pub async fn star(
    State(state): State<SharedState>,
    Path(resume_id): Path<u64>,
    Json(req): Json<StarRequest>,
) -> AppResult<Json<ApiBody<StarResponse>>> {
    // resume_id 仅用于沿用其目标岗位;不存在的简历直接忽略
    let resume = state.store.read(|db| db.resumes.iter().find(|r| r.id == resume_id).cloned());
    if resume.is_none() && req.target_position.is_none() {
        return Err(AppError::business(code::NOT_FOUND, "简历不存在"));
    }
    let position = req
        .target_position
        .or_else(|| resume.and_then(|r| r.target_position));
    let markdown = crate::resume_service::star(&state, req.project_text, position, req.jd_text).await?;
    Ok(Json(ApiBody::ok(StarResponse { markdown })))
}

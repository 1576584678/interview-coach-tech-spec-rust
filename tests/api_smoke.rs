//! 接口冒烟测试:不需要大模型,验证路由、本地数据与确定性逻辑。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use interview_coach::config::{AppConfig, ConfigFile};
use interview_coach::models::{InterviewQa, InterviewSession};
use interview_coach::state::SharedState;
use serde_json::Value;
use tower::ServiceExt;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("interview-coach-test-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建临时目录失败");
    dir
}

fn test_state(tag: &str) -> SharedState {
    let dir = temp_dir(tag);
    let mut config = AppConfig::default();
    config.server.data_dir = dir.display().to_string();
    config.server.auto_open_browser = false;
    let config_file = ConfigFile::new(dir.join("config.toml"));
    interview_coach::build_state(config_file, config).expect("初始化状态失败")
}

/// 一场「进行中」的面试(只有第 1 题,未作答),用于状态机相关的测试。
fn sample_session(id: u64, total_questions: i32) -> InterviewSession {
    InterviewSession {
        id,
        position: "Java后端".to_string(),
        position_category: "backend".to_string(),
        difficulty: "normal".to_string(),
        interviewer_style: "friendly".to_string(),
        mode: "normal".to_string(),
        status: "ongoing".to_string(),
        total_questions,
        resume_id: None,
        started_at: "2026-09-16 10:00:00".to_string(),
        completed_at: None,
        total_score: None,
        review_status: None,
        review_error: None,
        review: None,
        qa_list: vec![InterviewQa {
            question_order: 1,
            question: "请先做个自我介绍".to_string(),
            question_type: "basic".to_string(),
            answer: None,
            answered_at: None,
            score: None,
            feedback: None,
            better_answer: None,
        }],
    }
}

async fn get_json(state: SharedState, uri: &str) -> (StatusCode, Value) {
    let router = interview_coach::build_router(state, std::env::temp_dir());
    let response = router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).expect("响应不是合法 JSON");
    (status, value)
}

async fn post_json(state: SharedState, uri: &str, body: Value) -> (StatusCode, Value) {
    let router = interview_coach::build_router(state, std::env::temp_dir());
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).expect("响应不是合法 JSON");
    (status, value)
}

#[tokio::test]
async fn health_and_meta_are_available() {
    let state = test_state("health");
    let (status, body) = get_json(state.clone(), "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["status"], "ok");

    let (_, meta) = get_json(state, "/api/meta").await;
    assert!(meta["data"]["questionBank"]["total"].as_u64().unwrap() >= 80);
    assert!(meta["data"]["salaryBenchmarkCount"].as_u64().unwrap() >= 100);
}

#[tokio::test]
async fn salary_estimate_hits_local_benchmark() {
    let state = test_state("salary");
    let (status, body) = get_json(
        state.clone(),
        "/api/salary/estimate?position=Java%E5%90%8E%E7%AB%AF&city=%E5%8C%97%E4%BA%AC&experience=3%E5%B9%B4",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["p50"], 24000);
    assert_eq!(body["data"]["source"], "benchmark");

    // 广州系数 0.82,金额取整到百位
    let (_, guangzhou) = get_json(
        state.clone(),
        "/api/salary/estimate?position=Java%E5%90%8E%E7%AB%AF&city=%E5%B9%BF%E5%B7%9E&experience=3%E5%B9%B4",
    )
    .await;
    let p50 = guangzhou["data"]["p50"].as_i64().unwrap();
    assert!(p50 < 24000 && p50 % 100 == 0);
}

#[tokio::test]
async fn stats_and_history_start_empty() {
    let state = test_state("stats");
    let (_, stats) = get_json(state.clone(), "/api/interview/stats").await;
    assert_eq!(stats["data"]["totalSessions"], 0);
    assert!(stats["data"]["avgScore"].is_null());

    let (_, history) = get_json(state, "/api/interview/history?page=1&size=10").await;
    assert_eq!(history["data"]["total"], 0);
    assert!(history["data"]["list"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn interview_start_without_llm_returns_config_hint() {
    let state = test_state("no-llm");
    let (status, body) = post_json(
        state.clone(),
        "/api/interview/start",
        serde_json::json!({ "position": "Java后端开发" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], 1007);

    // 岗位为空时返回 400
    let (status, body) = post_json(
        state,
        "/api/interview/start",
        serde_json::json!({ "position": "  " }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], 400);
}

#[tokio::test]
async fn resume_from_text_is_persisted() {
    let state = test_state("resume");
    let (status, body) = post_json(
        state.clone(),
        "/api/resume/text",
        serde_json::json!({
            "rawText": "张三 · Java 后端开发\n5 年经验,负责订单系统重构,把下单耗时从 800ms 降到 200ms",
            "targetPosition": "Java后端开发"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], 0);
    let resume_id = body["data"]["resumeId"].as_u64().unwrap();
    assert!(resume_id > 0);

    let (_, list) = get_json(state, "/api/resume/list").await;
    assert_eq!(list["data"].as_array().unwrap().len(), 1);

    // 数据落到本地 JSON 文件,重启进程后仍可读取
    let data_file = std::env::temp_dir()
        .join("interview-coach-test-resume")
        .join(interview_coach::DATA_FILE);
    assert!(data_file.exists(), "本地数据文件未生成: {}", data_file.display());
}

#[tokio::test]
async fn config_can_be_updated_and_saved() {
    let state = test_state("config");
    let (status, body) = post_update(state.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["code"], 0);
    assert_eq!(body["data"]["model"], "deepseek-chat");
    assert_eq!(body["data"]["apiKeySet"], true);
    assert!(body["data"]["apiKeyMasked"].as_str().unwrap().contains("****"));

    let (_, config) = get_json(state.clone(), "/api/config").await;
    assert_eq!(config["data"]["totalQuestions"], 11);

    let saved = std::env::temp_dir()
        .join("interview-coach-test-config")
        .join("config.toml");
    assert!(saved.exists(), "config.toml 未写入");
    let text = std::fs::read_to_string(&saved).unwrap();
    assert!(text.contains("sk-test-key-123456"));
    // 测试用的临时 data-dir 属于进程级覆盖,不能写回 config.toml
    assert!(
        !text.contains("interview-coach-test-config"),
        "临时 data-dir 被写进了 config.toml:\n{text}"
    );
    assert!(text.contains("data_dir = \"data\""), "config.toml 应保留文件基线里的 data_dir:\n{text}");
    // 但设置页看到的仍是本进程实际生效的目录
    let temp_data_dir = std::env::temp_dir().join("interview-coach-test-config");
    assert_eq!(config["data"]["dataDir"], temp_data_dir.display().to_string());
}

/// 大模型失败时不能把答案提前落库,否则这一场面试会永久卡死(修复前的缺陷)。
#[tokio::test]
async fn llm_failure_keeps_current_question_answerable() {
    let state = test_state("answer-retry");
    state
        .store
        .write(|db| {
            let id = db.next_id();
            db.sessions.push(sample_session(id, 11));
            Ok(())
        })
        .unwrap();

    let session_id = 1;
    for (label, answer) in [("作答", Some("我有五年 Java 经验")), ("重试", Some("我有五年 Java 经验")), ("跳题", None)] {
        let result = interview_coach::interview_service::answer(&state, session_id, answer).await;
        let err = result.expect_err("未配置大模型时应当失败");
        assert_eq!(
            err.code(),
            interview_coach::error::code::CONFIG_ERROR,
            "{label} 应因大模型不可用而失败,而不是返回其他错误"
        );
    }

    state.store.read(|db| {
        let session = &db.sessions[0];
        assert_eq!(session.qa_list.len(), 1, "失败时不应写入新题目");
        assert!(session.qa_list[0].answer.is_none(), "失败时不应写入答案");
        assert!(session.qa_list[0].answered_at.is_none());
    });
}

/// 复盘任务只活在内存里:进程中途退出会在盘上留下 processing,重启时必须复位。
#[tokio::test]
async fn stale_review_processing_is_reset_on_startup() {
    let dir = temp_dir("stale-review");
    let mut session = sample_session(1, 1);
    session.status = "completed".to_string();
    session.completed_at = Some("2026-09-16 10:05:00".to_string());
    session.review_status = Some("processing".to_string());
    session.qa_list[0].answer = Some("我有五年经验".to_string());
    std::fs::write(
        dir.join(interview_coach::DATA_FILE),
        serde_json::json!({ "seq": 1, "sessions": [session], "resumes": [] }).to_string(),
    )
    .unwrap();

    let mut config = AppConfig::default();
    config.server.data_dir = dir.display().to_string();
    let state = interview_coach::build_state(ConfigFile::new(dir.join("config.toml")), config)
        .expect("初始化状态失败");

    // 复位之后 /result 才会重新触发复盘,否则前端会永远停在「复盘生成中…」
    state.store.read(|db| {
        assert_eq!(db.sessions[0].review_status, None, "重启后应复位遗留的 processing 状态");
        assert_eq!(db.sessions[0].review_error, None);
    });
}

async fn post_update(state: SharedState) -> (StatusCode, Value) {
    let router = interview_coach::build_router(state, std::env::temp_dir());
    let response = router
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/config")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "baseUrl": "https://api.deepseek.com",
                        "apiKey": "sk-test-key-123456",
                        "model": "deepseek-chat"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

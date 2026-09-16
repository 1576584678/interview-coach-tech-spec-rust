//! 接口冒烟测试:不需要大模型,验证路由、本地数据与确定性逻辑。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use interview_coach::config::{AppConfig, ConfigFile};
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

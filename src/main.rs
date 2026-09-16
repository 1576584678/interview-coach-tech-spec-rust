//! 面试教练(单机版)启动入口。

use std::net::SocketAddr;
use std::path::PathBuf;

use interview_coach::config::ConfigFile;

const HELP: &str = "\
面试教练(单机版)

用法:
  interview-coach [选项]

选项:
  --host <HOST>       监听地址,默认 127.0.0.1(仅本机可访问)
  --port <PORT>       监听端口,默认 8080
  --config <FILE>     配置文件路径,默认 ./config.toml
  --data-dir <DIR>    本地数据目录,默认 ./data
  --web-dir <DIR>     前端静态目录,默认 ./web
  --no-browser        启动后不自动打开浏览器
  --help              查看帮助

配置也可以用环境变量覆盖(见 .env.example):SERVER_PORT / LLM_BASE_URL / LLM_API_KEY / LLM_MODEL 等。
";

struct Args {
    host: Option<String>,
    port: Option<u16>,
    config: Option<String>,
    data_dir: Option<String>,
    web_dir: Option<String>,
    open_browser: Option<bool>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        host: None,
        port: None,
        config: None,
        data_dir: None,
        web_dir: None,
        open_browser: None,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--host" => args.host = iter.next(),
            "--port" => {
                let value = iter.next().ok_or("--port 缺少参数")?;
                args.port = Some(value.parse::<u16>().map_err(|_| "--port 需要是数字")?);
            }
            "--config" => args.config = iter.next(),
            "--data-dir" => args.data_dir = iter.next(),
            "--web-dir" => args.web_dir = iter.next(),
            "--no-browser" => args.open_browser = Some(false),
            "--help" | "-h" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            other => return Err(format!("未知参数: {other}(用 --help 查看用法)")),
        }
    }
    Ok(args)
}

/// 前端静态目录:优先命令行指定,其次当前目录,最后可执行文件同级目录。
fn resolve_web_dir(explicit: Option<String>) -> PathBuf {
    if let Some(dir) = explicit {
        return PathBuf::from(dir);
    }
    let cwd = PathBuf::from("web");
    if cwd.join("index.html").exists() {
        return cwd;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("web");
            if candidate.join("index.html").exists() {
                return candidate;
            }
        }
    }
    cwd
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,tower_http=warn")),
        )
        .init();

    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let config_file = match &args.config {
        Some(path) => ConfigFile::new(path),
        None => ConfigFile::from_env(),
    };
    let mut config = match config_file.load() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("读取配置失败: {}", err.message());
            std::process::exit(1);
        }
    };
    // 记住文件里的原始配置:命令行/环境变量只影响本次进程,不写回 config.toml
    let file_baseline = config.clone();
    config.apply_env_overrides();
    if let Some(host) = args.host {
        config.server.host = host;
    }
    if let Some(port) = args.port {
        config.server.port = port;
    }
    if let Some(dir) = args.data_dir {
        config.server.data_dir = dir;
    }
    if let Some(open) = args.open_browser {
        config.server.auto_open_browser = open;
    }
    if let Err(err) = config.validate() {
        eprintln!("配置不合法: {}", err.message());
        std::process::exit(1);
    }
    // 首次运行落盘一份 config.toml,方便直接在文件里改配置
    if !config_file.path().exists() {
        if let Err(err) = config_file.save(&file_baseline) {
            eprintln!("写入配置文件失败: {}", err.message());
        }
    }

    let web_dir = resolve_web_dir(args.web_dir);
    let state = match interview_coach::build_state(config_file, config.clone()) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("初始化本地存储失败: {}", err.message());
            std::process::exit(1);
        }
    };

    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], config.server.port)));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("端口 {addr} 无法监听: {err}(可用 --port 换一个端口)");
            std::process::exit(1);
        }
    };

    let url_host = if config.server.host == "0.0.0.0" { "127.0.0.1" } else { config.server.host.as_str() };
    let url = format!("http://{url_host}:{}", config.server.port);
    println!("面试教练(单机版)已启动: {url}");
    println!("配置文件: {}", state.config_path());
    println!("数据文件: {}", state.store.path().display());
    if !config.llm.configured() {
        println!("提示:还没有配置大模型,请在页面右上角「设置」里填写 API Key");
    }

    if config.server.auto_open_browser {
        let url = url.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            if let Err(err) = interview_coach::browser::open(&url) {
                println!("自动打开浏览器失败({err}),请手动访问: {url}");
            }
        });
    }

    let router = interview_coach::build_router(state, &web_dir);
    if let Err(err) = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        eprintln!("服务异常退出: {err}");
    }
    println!("已退出。");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(windows)]
    let terminate = async {
        // Windows 下 Ctrl+Break / 关闭控制台窗口时退出
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(not(windows))]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
            sigterm.recv().await;
        }
    };
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

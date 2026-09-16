//! 应用自更新:查询 GitHub Releases,并把新版程序与前端资源就地覆盖。
//!
//! 单机版数据全部在本机,因此更新只替换「程序自身」:
//! - 覆盖 exe / `web/` / 启动脚本等发布包内的文件;
//! - **绝不**读取或改写用户的 `config.toml`、`data/`、`.env`;
//! - 只从 GitHub 官方域名下载;
//! - 解包时拒绝绝对路径、盘符与 `..`,避免 zip-slip 写到安装目录之外。
//!
//! Windows 下正在运行的 exe 不能删除或覆盖(但可以改名),所以替换主程序时
//! 先把旧文件改名成 `xxx.old`,再写入新文件,残留文件由下次启动时清理。

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{code, AppError, AppResult};

/// 默认 GitHub 仓库(owner/name),可用环境变量 `INTERVIEW_COACH_REPO` 覆盖。
pub const DEFAULT_REPO: &str = "1576584678/interview-coach-tech-spec-rust";

/// 建连超时:github.com 在部分网络下会被墙,必须能快速失败好走下一个地址。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// 单次请求超时:检查更新很快,但下载几 MB 的安装包在受限网络里可能要几十秒。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
/// 更新包大小上限,避免异常下载把磁盘写满(当前各平台包约 4-5 MB)。
const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;
/// Release 说明文本返回给前端时的截断长度。
const MAX_NOTES_CHARS: usize = 2000;

/// 本机程序版本(来自 Cargo.toml)。
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 生效的 GitHub 仓库(owner/name)。
pub fn repo() -> String {
    std::env::var("INTERVIEW_COACH_REPO")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_REPO.to_string())
}

/// 当前平台在 Release 里的资源名前缀(不含版本号与后缀)。
pub fn asset_stem() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("interview-coach-windows-x64"),
        ("windows", "aarch64") => Some("interview-coach-windows-arm64"),
        ("linux", "x86_64") => Some("interview-coach-linux-x64"),
        ("linux", "aarch64") => Some("interview-coach-linux-arm64"),
        ("macos", "aarch64") => Some("interview-coach-macos-arm64"),
        ("macos", "x86_64") => Some("interview-coach-macos-x64"),
        _ => None,
    }
}

/// 当前平台期望的压缩包后缀。
fn expected_extension() -> &'static str {
    if cfg!(windows) {
        ".zip"
    } else {
        ".tar.gz"
    }
}

/// 「检查更新」返回给前端的信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    /// 当前平台在这一次 Release 里是否有对应安装包(没有就只能手动下载)。
    pub supported: bool,
    pub asset_name: Option<String>,
    pub download_url: Option<String>,
    /// GitHub API 的资源地址(会 302 到 release-assets)。
    ///
    /// `github.com/releases/download/...` 在部分网络下连不上,而 API 域名通常可达,
    /// 因此下载时优先走这里;不对外暴露给前端。
    #[serde(skip)]
    pub api_download_url: Option<String>,
    pub size: Option<u64>,
    pub notes: String,
    pub release_url: String,
    pub repo: String,
    pub message: String,
}

/// 「立即更新」的结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub version: String,
    /// 实际被覆盖的顶层条目名。
    pub updated: Vec<String>,
    pub restart_required: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    #[serde(default)]
    id: u64,
    name: String,
    #[serde(default)]
    size: u64,
    browser_download_url: String,
}

fn http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("interview-coach-updater/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|err| AppError::internal(format!("初始化更新客户端失败: {err}")))
}

/// 查询最新 Release 并与本机版本比较。
pub async fn check() -> AppResult<UpdateInfo> {
    let repo = repo();
    let client = http_client()?;
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AppError::business(
            code::NOT_FOUND,
            format!("仓库 {repo} 还没有发布任何 Release"),
        ));
    }
    let release: GhRelease = response.error_for_status()?.json().await?;
    Ok(build_info(&repo, current_version(), release))
}

fn build_info(repo: &str, current: &str, release: GhRelease) -> UpdateInfo {
    let latest = release.tag_name.trim().to_string();
    let has_update = is_newer(&latest, current);
    let extension = expected_extension();
    let asset = asset_stem().and_then(|stem| {
        release
            .assets
            .iter()
            .find(|asset| asset.name.starts_with(stem) && asset.name.ends_with(extension))
    });
    let supported = asset.is_some();
    let message = if has_update && supported {
        format!("发现新版本 {latest},可一键更新")
    } else if has_update {
        format!("发现新版本 {latest},但当前平台没有对应安装包,请手动下载")
    } else {
        format!("已是最新版本 {current}")
    };
    UpdateInfo {
        current_version: current.to_string(),
        latest_version: latest,
        has_update,
        supported,
        asset_name: asset.map(|asset| asset.name.clone()),
        download_url: asset.map(|asset| asset.browser_download_url.clone()),
        api_download_url: asset.map(|asset| {
            format!(
                "https://api.github.com/repos/{repo}/releases/assets/{}",
                asset.id
            )
        }),
        size: asset.map(|asset| asset.size),
        notes: truncate_chars(release.body.trim(), MAX_NOTES_CHARS),
        release_url: release.html_url,
        repo: repo.to_string(),
        message,
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push_str("\n…");
    out
}

/// 把 `v1.2.3-beta.1` 拆成数字段,用于比较版本。
fn version_segments(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

/// `latest` 是否比 `current` 新(逐段比较数字,因此 0.10.0 > 0.9.9)。
pub fn is_newer(latest: &str, current: &str) -> bool {
    let latest = version_segments(latest);
    let current = version_segments(current);
    for index in 0..latest.len().max(current.len()) {
        let left = latest.get(index).copied().unwrap_or(0);
        let right = current.get(index).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    false
}

/// 下载更新包并就地替换程序文件。
pub async fn download_and_apply(info: &UpdateInfo) -> AppResult<ApplyResult> {
    let install_dir = install_dir()?;
    let client = http_client()?;
    let bytes = fetch_archive(&client, info).await?;

    let staging = staging_dir();
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;

    // 优先按文件头判断格式,拿不准再退回资源名后缀
    let is_zip = if looks_like_zip(&bytes) {
        true
    } else if looks_like_gzip(&bytes) {
        false
    } else {
        info.asset_name
            .as_deref()
            .map(|name| name.ends_with(".zip"))
            .unwrap_or(cfg!(windows))
    };
    let extracted = if is_zip {
        extract_zip(&bytes, &staging)
    } else {
        extract_tar_gz(&bytes, &staging)
    };
    if let Err(err) = extracted {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(err);
    }

    let root = match find_package_root(&staging) {
        Some(root) => root,
        None => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(AppError::internal(
                "更新包里没有找到程序文件(web/index.html 或主程序)",
            ));
        }
    };
    let updated = install_from(&root, &install_dir);
    let _ = std::fs::remove_dir_all(&staging);
    let updated = updated?;

    if updated.is_empty() {
        return Err(AppError::internal("更新包里没有可用的文件,已跳过"));
    }
    Ok(ApplyResult {
        version: info.latest_version.clone(),
        updated,
        restart_required: true,
        message: format!(
            "已更新到 {}。请关闭并重新启动面试教练,新版本才会生效。",
            info.latest_version
        ),
    })
}

/// 启动时清理上一轮更新留下的 `*.old`(更新当时旧 exe 仍在运行,删不掉)。
pub fn cleanup_old_files() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_old = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".old"))
            .unwrap_or(false);
        if is_old {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// 一个可尝试的下载地址;API 地址需要额外的 Accept 头才会返回文件本体。
struct DownloadSource {
    url: String,
    accept: Option<&'static str>,
}

fn download_sources(info: &UpdateInfo) -> Vec<DownloadSource> {
    let mut sources = Vec::new();
    if let Some(url) = info.api_download_url.as_ref() {
        sources.push(DownloadSource {
            url: url.clone(),
            accept: Some("application/octet-stream"),
        });
    }
    if let Some(url) = info.download_url.as_ref() {
        if !sources.iter().any(|source| &source.url == url) {
            sources.push(DownloadSource { url: url.clone(), accept: None });
        }
    }
    sources
}

/// 依次尝试各个下载地址,全部失败时把最后一个错误抛出去。
async fn fetch_archive(client: &reqwest::Client, info: &UpdateInfo) -> AppResult<Vec<u8>> {
    let sources = download_sources(info);
    if sources.is_empty() {
        return Err(AppError::business(
            code::BAD_REQUEST,
            "当前平台没有可用的更新包,请手动下载",
        ));
    }
    let mut last_error: Option<String> = None;
    for source in sources {
        if !is_trusted_url(&source.url) {
            last_error = Some(format!("地址不在 GitHub 官方域名下: {}", source.url));
            continue;
        }
        let mut request = client.get(&source.url);
        if let Some(accept) = source.accept {
            request = request.header("Accept", accept);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                last_error = Some(err.to_string());
                continue;
            }
        };
        let response = match response.error_for_status() {
            Ok(response) => response,
            Err(err) => {
                last_error = Some(err.to_string());
                continue;
            }
        };
        if let Some(length) = response.content_length() {
            if length > MAX_DOWNLOAD_BYTES {
                last_error = Some(format!("更新包大小 {length} 字节,超过上限"));
                continue;
            }
        }
        match response.bytes().await {
            Ok(bytes) if bytes.len() as u64 <= MAX_DOWNLOAD_BYTES => return Ok(bytes.to_vec()),
            Ok(_) => last_error = Some("更新包超过大小上限".to_string()),
            Err(err) => last_error = Some(err.to_string()),
        }
    }
    Err(AppError::internal(format!(
        "下载更新包失败: {}",
        last_error.unwrap_or_else(|| "没有可用地址".to_string())
    )))
}

fn is_trusted_url(url: &str) -> bool {
    const ALLOWED: [&str; 4] = [
        "https://api.github.com/",
        "https://github.com/",
        "https://objects.githubusercontent.com/",
        "https://release-assets.githubusercontent.com/",
    ];
    ALLOWED.iter().any(|prefix| url.starts_with(prefix))
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")
}

fn looks_like_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1f, 0x8b])
}

/// 主程序所在目录(即安装目录)。
fn install_dir() -> AppResult<PathBuf> {
    let exe = std::env::current_exe()
        .map_err(|err| AppError::internal(format!("无法定位当前程序: {err}")))?;
    exe.parent()
        .map(|dir| dir.to_path_buf())
        .ok_or_else(|| AppError::internal("无法定位当前程序所在目录"))
}

fn staging_dir() -> PathBuf {
    std::env::temp_dir().join(format!("interview-coach-update-{}", std::process::id()))
}

/// 在解包结果里找到发布包的根目录。
///
/// 发布包结构是 `interview-coach-<平台>/…`,但解包目录本身也可能是根目录,
/// 所以两者都试一遍。
fn find_package_root(staging: &Path) -> Option<PathBuf> {
    let looks_like_root = |dir: &Path| {
        dir.join("web").join("index.html").is_file()
            || dir.join("interview-coach.exe").is_file()
            || dir.join("interview-coach").is_file()
    };
    if looks_like_root(staging) {
        return Some(staging.to_path_buf());
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(staging)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    candidates.sort();
    candidates.into_iter().find(|dir| looks_like_root(dir))
}

/// 用户数据:更新时一律不动。
fn is_protected(name: &str) -> bool {
    matches!(name, "config.toml" | "data" | ".env")
}

/// 需要「先改名再写」才能替换的正在运行的主程序。
fn is_program_file(name: &str) -> bool {
    name == "interview-coach" || name == "interview-coach.exe"
}

/// 把发布包根目录下的顶层条目覆盖到安装目录,跳过用户数据。
fn install_from(root: &Path, install_dir: &Path) -> AppResult<Vec<String>> {
    let mut programs: Vec<PathBuf> = Vec::new();
    let mut others: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if is_protected(&name) {
            continue;
        }
        let path = entry.path();
        if path.is_file() && is_program_file(&name) {
            programs.push(path);
        } else {
            others.push(path);
        }
    }
    others.sort();
    programs.sort();

    let mut updated = Vec::new();
    // 先替换静态资源,最后再动正在运行的 exe,尽量缩短「换到一半」的时间窗。
    for source in others {
        let name = source
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| AppError::internal("更新包条目缺少文件名"))?;
        let dest = install_dir.join(&name);
        if source.is_dir() {
            copy_dir_overwrite(&source, &dest)?;
        } else {
            copy_file_overwrite(&source, &dest)?;
        }
        updated.push(name);
    }
    for source in programs {
        let name = source
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| AppError::internal("更新包条目缺少文件名"))?;
        replace_program(&source, &install_dir.join(&name))?;
        updated.push(name);
    }
    Ok(updated)
}

fn copy_file_overwrite(source: &Path, dest: &Path) -> AppResult<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(source)?.permissions().mode();
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

fn copy_dir_overwrite(source: &Path, dest: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            copy_dir_overwrite(&path, &target)?;
        } else {
            copy_file_overwrite(&path, &target)?;
        }
    }
    Ok(())
}

/// 替换正在运行的主程序。
///
/// - Windows:运行中的 exe 不能被删除/覆盖,但可以改名;
/// - Linux:直接写正在运行的可执行文件会返回 `ETXTBSY`。
///
/// 因此统一先把旧文件改名成 `xxx.old`,再把新文件写到原位置。
fn replace_program(source: &Path, dest: &Path) -> AppResult<()> {
    let backup = backup_path(dest);
    let _ = std::fs::remove_file(&backup);
    if dest.exists() {
        std::fs::rename(dest, &backup).map_err(|err| {
            AppError::internal(format!(
                "无法替换主程序(改名为 {} 失败): {err}",
                backup.display()
            ))
        })?;
    }
    if let Err(err) = std::fs::copy(source, dest) {
        // 写失败就把旧文件换回去,避免留下一个残缺的 exe
        let _ = std::fs::rename(&backup, dest);
        return Err(AppError::internal(format!("写入新主程序失败: {err}")));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn backup_path(dest: &Path) -> PathBuf {
    match dest.file_name() {
        Some(name) => dest.with_file_name(format!("{}.old", name.to_string_lossy())),
        None => dest.with_extension("old"),
    }
}

/// 包内路径 → 安全的相对路径:拒绝绝对路径、盘符与 `..`(防 zip-slip)。
fn safe_relative_path(raw: &str) -> Option<PathBuf> {
    let normalized = raw.replace('\\', "/");
    // 绝对路径(/etc/passwd、C:/Windows)一律拒绝
    if normalized.starts_with('/') {
        return None;
    }
    let mut out = PathBuf::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => continue,
            ".." => return None,
            _ => {
                if part.contains(':') {
                    return None;
                }
                out.push(part);
            }
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn extract_zip(data: &[u8], dest: &Path) -> AppResult<()> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
        .map_err(|err| AppError::internal(format!("解压更新包失败: {err}")))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| AppError::internal(format!("读取更新包条目失败: {err}")))?;
        let name = entry.name().to_string();
        let Some(relative) = safe_relative_path(&name) else {
            continue;
        };
        let out = dest.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut file)?;
    }
    Ok(())
}

fn extract_tar_gz(data: &[u8], dest: &Path) -> AppResult<()> {
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(data));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|err| AppError::internal(format!("解压更新包失败: {err}")))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|err| AppError::internal(format!("读取更新包条目失败: {err}")))?;
        let raw = entry
            .path()
            .map_err(|err| AppError::internal(format!("更新包条目路径非法: {err}")))?
            .to_string_lossy()
            .to_string();
        let Some(relative) = safe_relative_path(&raw) else {
            continue;
        };
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            // 忽略符号链接、设备文件等特殊条目
            continue;
        }
        let out = dest.join(&relative);
        if kind.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut file)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ic-updater-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("创建临时目录");
        dir
    }

    #[test]
    fn version_compare_is_numeric_not_lexical() {
        assert!(is_newer("v0.1.4", "0.1.3"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("v0.1.4", "0.1.4"));
        assert!(!is_newer("0.1.3", "0.1.4"));
        assert!(!is_newer("", "0.1.4"));
    }

    #[test]
    fn asset_stem_follows_platform() {
        let stem = asset_stem();
        if cfg!(windows) {
            assert_eq!(stem, Some("interview-coach-windows-x64"));
        } else if cfg!(target_os = "macos") {
            assert!(matches!(
                stem,
                Some("interview-coach-macos-arm64") | Some("interview-coach-macos-x64")
            ));
        } else {
            assert_eq!(stem, Some("interview-coach-linux-x64"));
        }
    }

    #[test]
    fn build_info_picks_matching_asset() {
        let release = GhRelease {
            tag_name: "v0.2.0".to_string(),
            html_url: "https://github.com/x/y/releases/tag/v0.2.0".to_string(),
            body: "更新说明".to_string(),
            assets: vec![
                GhAsset {
                    id: 1,
                    name: "interview-coach-linux-x64-v0.2.0.tar.gz".to_string(),
                    size: 11,
                    browser_download_url: "https://github.com/x/y/releases/download/v0.2.0/l".to_string(),
                },
                GhAsset {
                    id: 2,
                    name: "interview-coach-windows-x64-v0.2.0.zip".to_string(),
                    size: 22,
                    browser_download_url: "https://github.com/x/y/releases/download/v0.2.0/w".to_string(),
                },
            ],
        };
        let info = build_info("x/y", "0.1.4", release);
        assert!(info.has_update);
        assert!(info.supported);
        assert_eq!(info.latest_version, "v0.2.0");
        let expected_asset = asset_stem().expect("本平台有对应资源名");
        assert!(info.asset_name.as_deref().unwrap().starts_with(expected_asset));
        assert!(info.download_url.as_deref().unwrap().starts_with("https://github.com/"));
        assert!(info.message.contains("发现新版本"));
        let api = info.api_download_url.as_deref().expect("应有 API 下载地址");
        assert!(api.starts_with("https://api.github.com/repos/x/y/releases/assets/"));
        assert!(is_trusted_url(api));
    }

    #[test]
    fn download_prefers_api_endpoint_then_browser_url() {
        let release = GhRelease {
            tag_name: "v0.2.0".to_string(),
            html_url: String::new(),
            body: String::new(),
            assets: vec![GhAsset {
                id: 7,
                name: "interview-coach-windows-x64-v0.2.0.zip".to_string(),
                size: 3,
                browser_download_url: "https://github.com/x/y/releases/download/v0.2.0/w".to_string(),
            }],
        };
        let info = build_info("x/y", "0.1.0", release);
        let sources = download_sources(&info);
        assert_eq!(sources.len(), 2, "API 地址在前、浏览器地址兜底");
        assert!(sources[0].url.starts_with("https://api.github.com/"));
        assert_eq!(sources[0].accept, Some("application/octet-stream"));
        assert!(sources[1].url.starts_with("https://github.com/"));
        assert_eq!(sources[1].accept, None);
    }

    #[test]
    fn archive_format_is_sniffed_from_magic_bytes() {
        assert!(looks_like_zip(b"PK\x03\x04rest"));
        assert!(!looks_like_zip(b"\x1f\x8brest"));
        assert!(looks_like_gzip(b"\x1f\x8brest"));
        assert!(!looks_like_gzip(b"PK\x03\x04rest"));
    }

    #[test]
    fn build_info_reports_up_to_date() {
        let release = GhRelease {
            tag_name: "v0.1.4".to_string(),
            html_url: String::new(),
            body: String::new(),
            assets: vec![],
        };
        let info = build_info("x/y", "0.1.4", release);
        assert!(!info.has_update);
        assert!(info.message.contains("已是最新版本"));
    }

    #[test]
    fn safe_relative_path_rejects_zip_slip() {
        assert!(safe_relative_path("../evil.txt").is_none());
        assert!(safe_relative_path("a/../../evil.txt").is_none());
        assert!(safe_relative_path("/etc/passwd").is_none());
        assert!(safe_relative_path("C:/Windows/system32/x.dll").is_none());
        assert!(safe_relative_path("pkg\\..\\..\\evil").is_none());
        assert_eq!(
            safe_relative_path("pkg/web/index.html").unwrap(),
            PathBuf::from("pkg").join("web").join("index.html")
        );
        assert_eq!(
            safe_relative_path("./pkg//web/").unwrap(),
            PathBuf::from("pkg").join("web")
        );
    }

    #[test]
    fn extract_zip_skips_traversal_and_writes_files() {
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("../evil.txt", options).expect("写条目");
            writer.write_all(b"bad").expect("写内容");
            writer.start_file("pkg/web/index.html", options).expect("写条目");
            writer.write_all(b"ok").expect("写内容");
            writer.finish().expect("结束压缩");
        }
        let data = buffer.into_inner();
        let base = scratch("zip");
        let dest = base.join("out");
        std::fs::create_dir_all(&dest).expect("创建解包目录");
        extract_zip(&data, &dest).expect("解压");
        assert!(!base.join("evil.txt").exists(), "越界条目必须被忽略");
        assert_eq!(
            std::fs::read(dest.join("pkg").join("web").join("index.html")).expect("读文件"),
            b"ok"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn extract_tar_gz_writes_files() {
        let mut builder = tar::Builder::new(Vec::new());
        let content = b"hello-tar";
        let mut header = tar::Header::new_ustar();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "pkg/web/app.js", &content[..])
            .expect("写入 tar");
        let tar_bytes = builder.into_inner().expect("结束 tar");
        let mut encoder =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar_bytes).expect("gzip");
        let gz = encoder.finish().expect("结束 gzip");

        let base = scratch("tar");
        let dest = base.join("out");
        std::fs::create_dir_all(&dest).expect("创建解包目录");
        extract_tar_gz(&gz, &dest).expect("解压");
        assert_eq!(
            std::fs::read(dest.join("pkg").join("web").join("app.js")).expect("读文件"),
            content
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn install_from_keeps_user_data_and_replaces_program() {
        let base = scratch("install");
        let package = base.join("package");
        let target = base.join("target");
        std::fs::create_dir_all(package.join("web")).expect("建包目录");
        std::fs::create_dir_all(&target).expect("建安装目录");
        std::fs::write(package.join("web").join("index.html"), b"new").expect("写前端");
        std::fs::write(package.join("config.toml"), b"new-config").expect("写配置");
        std::fs::write(package.join("data"), b"new-data").expect("写数据");
        let program_name = if cfg!(windows) {
            "interview-coach.exe"
        } else {
            "interview-coach"
        };
        std::fs::write(package.join(program_name), b"new-exe").expect("写程序");

        // 安装目录里的用户数据与旧程序
        std::fs::write(target.join("config.toml"), b"user-config").expect("写用户配置");
        std::fs::create_dir_all(target.join("data")).expect("建用户数据目录");
        std::fs::write(target.join("data").join("db.json"), b"user-data").expect("写用户数据");
        std::fs::write(target.join(program_name), b"old-exe").expect("写旧程序");

        let updated = install_from(&package, &target).expect("安装");
        assert!(updated.contains(&"web".to_string()));
        assert!(updated.contains(&program_name.to_string()));
        assert!(!updated.contains(&"config.toml".to_string()));
        assert!(!updated.contains(&"data".to_string()));
        assert_eq!(std::fs::read(target.join("web").join("index.html")).unwrap(), b"new");
        assert_eq!(std::fs::read(target.join(program_name)).unwrap(), b"new-exe");
        assert_eq!(std::fs::read(target.join("config.toml")).unwrap(), b"user-config");
        assert_eq!(
            std::fs::read(target.join("data").join("db.json")).unwrap(),
            b"user-data"
        );
        assert!(backup_path(&target.join(program_name)).exists(), "旧程序应改名为 .old");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn trusted_url_only_allows_github() {
        assert!(is_trusted_url(
            "https://github.com/1576584678/interview-coach-tech-spec-rust/releases/download/v0.1.4/interview-coach-windows-x64-v0.1.4.zip"
        ));
        assert!(!is_trusted_url("https://example.com/x.zip"));
        assert!(!is_trusted_url("http://github.com/x.zip"));
        assert!(!is_trusted_url("file:///tmp/x.zip"));
    }
}

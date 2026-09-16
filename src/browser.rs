//! 用系统默认浏览器打开链接(自实现,避免引入额外依赖)。

use std::process::Command;

/// 用系统默认程序打开 URL。
pub fn open(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = Command::new("cmd");
        // `start` 会把第一个参数当作窗口标题,这里用空标题占位。
        command.args(["/C", "start", "", url]);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command.spawn().map(|_| ())
}

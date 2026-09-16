#!/usr/bin/env bash
# 组装 Linux / macOS 便携包(可执行文件 + web + 启动脚本 + 使用说明),并压成 tar.gz。
# 用法:
#   bash tools/make-dist.sh <可执行文件> [输出名] [输出目录]
# 示例:
#   bash tools/make-dist.sh target/release/interview-coach
#   bash tools/make-dist.sh target/aarch64-apple-darwin/release/interview-coach interview-coach-macos-arm64
set -euo pipefail

# 统一按字节处理文件名(tar 在非 UTF-8 locale 下对中文名可能报 Cannot stat)
export LC_ALL=C
export LANG=C

# IC_DIST_DEBUG=1 时打印每条命令,便于在 CI 日志里定位失败点
if [[ "${IC_DIST_DEBUG:-}" == "1" ]]; then
  set -x
fi

exe="${1:-}"
name="${2:-interview-coach}"
out_dir="${3:-dist}"

if [[ -z "$exe" ]]; then
  echo "用法: bash tools/make-dist.sh <可执行文件> [输出名] [输出目录]" >&2
  exit 2
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

exe_path="$exe"
if [[ ! -f "$exe_path" ]]; then
  exe_path="$root/$exe"
fi
if [[ ! -f "$exe_path" ]]; then
  echo "找不到可执行文件: $exe(先执行 cargo build --release)" >&2
  exit 1
fi

pkg_root="$root/$out_dir"
pkg="$pkg_root/$name"
rm -rf "$pkg"
mkdir -p "$pkg"

cp "$exe_path" "$pkg/interview-coach"
chmod +x "$pkg/interview-coach"
cp -R "$root/web" "$pkg/web"
cp "$root/config.example.toml" "$pkg/config.example.toml"

cat > "$pkg/start.sh" <<'LAUNCHER'
#!/usr/bin/env bash
# 双击或在终端执行本脚本即可启动面试教练。
cd "$(dirname "$0")"
echo "面试教练启动中: http://127.0.0.1:8080"
echo "按 Ctrl+C 停止"
exec ./interview-coach "$@"
LAUNCHER
chmod +x "$pkg/start.sh"

cat > "$pkg/使用说明.txt" <<'NOTICE'
面试教练 · 单机版(Linux / macOS 便携版)
======================================

一、怎么用
----------
1. 解压后进入该目录
2. 首次运行先加执行权限:chmod +x interview-coach start.sh
3. 运行 ./start.sh(或直接 ./interview-coach)
4. 浏览器打开 http://127.0.0.1:8080
5. 首次使用:右上角「设置」-> 填 Base URL / API Key / 模型名 -> 「测试连接」-> 保存

   常用大模型配置:
   - DeepSeek        https://api.deepseek.com                       deepseek-chat
   - 通义千问        https://dashscope.aliyuncs.com/compatible-mode  qwen-plus
   - 本地 Ollama     http://localhost:11434/v1                      qwen2.5:7b(需先装 Ollama)

二、数据存在哪
--------------
- config.toml                 首次运行自动生成,大模型配置(含 API Key)
- data/interview-coach.json   全部面试记录 / 简历 / 报告
备份就把这两个文件拷走;换机器拷过去即可继续用。

三、常见问题
------------
1) macOS 提示「无法打开,因为来自身份不明的开发者」
   本包没有代码签名。右键 -> 打开 -> 弹窗里再点「打开」;
   或执行:xattr -dr com.apple.quarantine 该目录
2) 提示 Permission denied
   chmod +x interview-coach start.sh
3) 提示端口被占用 / 想换端口
   ./interview-coach --port 9000
   加 --no-browser 可以不自动打开浏览器。
4) 页面空白或 404
   检查 web 目录是否和可执行文件在同一层(必须一起拷贝,不能只拷可执行文件)。

四、说明
--------
- 服务只监听 127.0.0.1,数据全部留在本机,除调用大模型外不联网。
NOTICE

tar -C "$pkg_root" -czf "$pkg_root/$name.tar.gz" "$name"

bytes=$(wc -c < "$pkg_root/$name.tar.gz")
echo "已生成: $pkg_root/$name.tar.gz ($((bytes / 1024 / 1024)) MB)"

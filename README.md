# 面试教练 · 单机版(Rust)

> AI 面试全链路练习工具:薪资定位 → 简历诊断/优化 → AI 模拟面试 → 打分复盘 → 提升计划。
> 参考 `C:\project\面试教练`(Java + MySQL + Redis + 微信小程序)重写为**单机版**:
> 无用户体系、无数据库、无 Redis,双击/一条命令就能在本机跑起来。

## 单机版做了什么

| 维度 | Java 版 | 本单机版 |
|---|---|---|
| 数据存储 | MySQL 8 + Redis 7 | **一个本地 JSON 文件**(`data/interview-coach.json`) |
| 用户体系 | 微信登录 / JWT / 额度 / 支付工单 | **不需要**,本机即你自己 |
| 部署 | Docker + Nginx + 云服务器 | **本机运行**,默认只监听 `127.0.0.1` |
| 前端 | React + Vite(需 Node 构建) | **免构建** 的原生 HTML/CSS/JS,由 Rust 进程直接托管 |
| 大模型 | 配置在 application.yml | **页面里随时改**,写入 `config.toml`,支持任意 OpenAI 兼容接口 |
| 题库/薪资基准 | 数据库种子脚本 | 内置 JSON 数据(93 道题 / 117 条薪资基准),离线可用 |

保留的核心能力(与 Java 版同款 Prompt,效果可对齐):

- **模拟面试**:11 题连续追问(自我介绍 → 基础技术 → 项目深挖 → 场景设计 → 开放题),支持难度/面试官风格(友好/严谨/压力)/真实模拟模式、跳题、SSE 流式出题、简历关联追问。
- **打分复盘**:5 维评分(专业/表达/逻辑/沟通/抗压)+ 逐题点评 + 更好回答示范 + 亮点/薄弱项/改进建议/减分行为提醒,支持导出 Markdown。
- **简历诊断与优化**:上传 `txt/md/docx/pdf`(文本型)或直接粘贴;诊断只找风险,优化输出逐条改写建议与完整优化稿,支持大厂/外企/国企等风格。
- **STAR 口述稿**:把简历里的一段项目经历改写成面试可直接口述的 60-90 秒版本。
- **薪资定位**:本地基准表 + 城市系数折算(北京为锚点),未命中时用大模型兜底并标注低置信度。
- **迭代追踪**:分数趋势、五维雷达图、薄弱题型统计,并据此生成两周提升计划;薄弱题型会自动穿插进下一场面试。

## 快速开始

### 1. 装 Rust(已装可跳过)

```powershell
winget install Rustlang.Rustup
```

> Windows 上建议使用 `x86_64-pc-windows-msvc` 工具链(GNU 工具链的 `dlltool` 对中文路径不友好)。

### 2. 启动

**方式 A:双击(最省事,推荐)**

直接双击项目根目录的 `启动面试教练.cmd`。它使用 `bin\interview-coach.exe`(已编译好的 debug 版),自动打开浏览器 `http://127.0.0.1:8080`。

**方式 B:命令行**

```powershell
cd C:\project\面试教练-rust版
cargo run            # 本机若开了智能应用控制,--release 会被拦,见「常见问题 1」,debug 版完全够用
```

首次运行会自动生成 `config.toml` 与 `data/` 目录,并打开浏览器 `http://127.0.0.1:8080`。

如果浏览器没自动打开,手动访问即可;不想自动打开就加 `--no-browser`。

> 不想每次都往 `%TEMP%` 里编译:GNU 工具链的 `ar`/`ld`/`dlltool` 读不了中文路径,
> 编译时请把产物目录指到纯英文路径,例如
> `$env:CARGO_TARGET_DIR = "$env:TEMP\ic-target2"; cargo run`。

### 3. 配置大模型

页面右上角 **设置** → 填 Base URL / API Key / 模型名 → 「测试连接」→ 保存。

- DeepSeek:`https://api.deepseek.com` + `deepseek-chat`
- 通义千问:`https://dashscope.aliyuncs.com/compatible-mode` + `qwen-plus`
- 本地 Ollama:`http://localhost:11434/v1` + `qwen2.5:7b`
- 也可以用环境变量覆盖(优先级高于 `config.toml`):

```powershell
$env:LLM_API_KEY="sk-xxxx"; cargo run --release
```

## 常用命令

```powershell
cargo run                           # 启动(默认 127.0.0.1:8080)
cargo run -- --port 9000            # 换端口
cargo run -- --no-browser           # 不自动打开浏览器
cargo test                          # 19 个单元测试 + 6 个接口冒烟测试
cargo run -- --help                 # 查看全部参数
cargo build --release               # 产出 target/release/interview-coach.exe(本机若开了智能应用控制会失败,见常见问题 1)
```

命令行参数:`--host` `--port` `--config` `--data-dir` `--web-dir` `--no-browser`。

## 分发 / 便携包

对方机器**不需要装 Rust**,便携包里已经带上了 exe、前端和启动脚本。依赖只有 Windows 自带的系统 DLL,
不需要 Node / MySQL / Redis / VC++ 运行库;题库、薪资基准、Prompt 模板都用 `include_str!` 编进了二进制。

```powershell
pwsh -File tools\make-dist.ps1                                    # 用 bin\interview-coach.exe 打包
pwsh -File tools\make-dist.ps1 -ExePath target\debug\interview-coach.exe
pwsh -File tools\make-dist.ps1 -NoZip                             # 只生成目录
```

产物:

- `dist\面试教练-便携版-win64\` —— exe + `web\` + `启动面试教练.cmd` + `使用说明.txt`
- `dist\面试教练-便携版-win64.zip` —— 约 34 MB(debug exe 压缩后),可直接发给别人

对方解压后双击 `启动面试教练.cmd` 即可,`config.toml` 与 `data\` 会在运行时自动生成。

注意:

- 只拷 exe 不拷 `web\` 会白屏,两者必须在同一层。
- exe 没有代码签名:目标机器若开了**智能应用控制**同样会被拦(见常见问题 1);
  首次运行可能弹 SmartScreen,需要点「更多信息 → 仍要运行」;杀软也可能误报,需加白名单。
- 只支持 64 位 Windows 10/11(本机为 `x86_64-pc-windows-gnu` 构建)。

## 目录结构

```
面试教练-rust版/
├── 启动面试教练.cmd           # 双击启动(debug 版)
├── bin/interview-coach.exe     # 已编译好的 debug 可执行文件(被 .gitignore 忽略)
├── tools/make-dist.ps1         # 组装便携包(目录 + zip)
├── tools/usage.txt             # 便携包里的「使用说明.txt」正文
├── dist/                       # 便携包产物(exe + web + 启动脚本 + 说明,被忽略)
├── Cargo.toml
├── config.toml                 # 运行后生成:大模型/服务配置(可直接编辑)
├── data/interview-coach.json   # 运行后生成:全部本地数据(备份就拷这个文件)
├── src/
│   ├── main.rs                 # 启动入口(参数解析、自动开浏览器、优雅退出)
│   ├── lib.rs                  # 模块组装
│   ├── config.rs               # config.toml + 环境变量
│   ├── store.rs                # 单文件 JSON 存储(原子写入)
│   ├── models.rs               # 领域模型 / DTO
│   ├── llm.rs                  # OpenAI 兼容客户端(重试、JSON 模式、SSE 流式)
│   ├── prompt.rs + prompts/    # Prompt 模板(与 Java 版一致)
│   ├── interview_service.rs    # 11 题面试流程、出题节奏、自适应薄弱题型
│   ├── review_service.rs       # 打分复盘(异步生成 + 状态轮询)
│   ├── resume_service.rs       # 简历诊断 / 优化 / STAR
│   ├── analysis.rs             # 统计、薄弱点、提升计划
│   ├── salary.rs               # 城市系数 + 基准表查询
│   ├── file_parser.rs          # txt/md/docx/pdf 文本抽取(纯本地实现)
│   ├── question_bank.rs        # 内置题库
│   ├── browser.rs              # 打开系统默认浏览器(自实现,无额外依赖)
│   ├── routes/                 # HTTP 路由
│   └── data/*.json             # 内置题库与薪资基准数据
├── web/                        # 免构建前端(原生 JS + CSS)
└── tests/api_smoke.rs          # 接口冒烟测试
```

## 接口一览(全部挂在 `/api`)

统一响应体:`{ "code": 0, "message": "success", "data": ... }`,`code != 0` 表示失败。

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/api/health` | 健康检查(版本、模型、数据文件路径) |
| GET | `/api/meta` | 前端枚举(岗位类别/难度/风格/模式/题库统计) |
| GET/PUT | `/api/config` | 读取 / 更新大模型配置 |
| POST | `/api/config/test` | 测试大模型连通性 |
| POST | `/api/interview/start` | 开始面试,返回第一题 |
| POST | `/api/interview/{id}/answer` | 回答当前题,返回下一题 |
| POST | `/api/interview/{id}/answer/stream` | 同上,SSE 流式返回下一题 |
| POST | `/api/interview/{id}/skip` | 跳过当前题 |
| POST | `/api/interview/{id}/complete` | 结束面试并异步生成复盘 |
| POST | `/api/interview/{id}/abandon` | 放弃面试 |
| GET | `/api/interview/{id}/detail` | 面试详情(含逐题问答) |
| GET | `/api/interview/{id}/result` | 复盘报告(生成中返回状态) |
| POST | `/api/interview/{id}/review/retry` | 复盘失败后重试 |
| DELETE | `/api/interview/{id}` | 删除某场面试 |
| GET | `/api/interview/history` | 历史记录(分页) |
| GET | `/api/interview/stats` | 统计、趋势、五维平均、薄弱点 |
| GET/POST | `/api/interview/improvement-plan` | 读取 / 重新生成提升计划 |
| POST | `/api/resume/upload` | 上传简历文件(multipart) |
| POST | `/api/resume/text` | 粘贴简历文本 |
| GET | `/api/resume/list`、`/api/resume/{id}` | 简历列表 / 详情 |
| POST | `/api/resume/{id}/diagnosis` | 简历诊断 |
| POST | `/api/resume/{id}/optimize` | 简历优化 |
| POST | `/api/resume/{id}/star` | 项目经历 STAR 改写 |
| DELETE | `/api/resume/{id}` | 删除简历 |
| GET | `/api/salary/estimate` | 薪资定位(`position` / `city` / `experience`) |

## 与 Java 版的行为差异

- 去掉了用户、额度、支付、管理员、内容安全审核(单机自用不需要);Prompt 与评分口径保持一致。
- 复盘为后台异步生成(状态 `processing`),前端自动轮询;失败可在页面重试。
- 简历解析为纯本地实现:支持 `txt/md/docx/pdf(文本型)`;扫描件、图片型 PDF、旧版 `.doc` 会提示改用粘贴文本。
- 薪资基准与题库内置于二进制,无需初始化数据库;想要更新数据直接改 `src/data/*.json` 重新编译。

## 常见问题

**1. 报错 `应用程序控制策略已阻止此文件`(os error 4551)**

本机开启了 Windows **智能应用控制(Smart App Control)**,它会拦截所有未签名的可执行文件,包括 Rust 编译过程产生的 `build-script-build.exe`,因此 `--release` 构建会被拦截。**已编译好的 debug 版不受影响,双击 `启动面试教练.cmd` 即可使用。**

实测结论:

- **`cargo build --release` 在这台机器上无法通过**:智能应用控制已从「评估模式」切到「强制模式」(`HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy` 里 `VerifiedAndReputablePolicyState = 1`、`SAC_PreviousState = 2`),CodeIntegrity 日志明确记录拦截 `release\deps\rustversion-*.dll` 等新生成的未签名二进制;连续重试 40 次仍停在同一个文件。
- **`cargo check` / `cargo test` / `cargo build`(debug)一直正常**,改代码后的增量编译也正常,所以直接用 `cargo run` 即可。

如果一定要 release 包,只能:

- 关闭智能应用控制:`Windows 安全中心 → 应用和浏览器控制 → 智能应用控制 → 关闭`(需要重启;官方说明关闭后无法再次开启,除非重装/重置系统)。
- 或换一台未启用该策略的机器 / Linux 构建,再把 `target/release/interview-coach.exe` 连同 `web/`、`config.toml` 拷回来运行(注意:在本机运行该 exe 也可能被拦)。

**2. `ar: C:\project\???-rust??\...: cause of error unknown` / `ld: cannot find -l...`**

项目路径含中文时,GNU 工具链(Rustup 的 `x86_64-pc-windows-gnu`)自带的 binutils(`ar`/`ld`/`dlltool`)
会把中文路径读成乱码。把**编译产物目录**指到纯英文路径即可(已实测可用):

```powershell
cd C:\project\面试教练-rust版
$env:CARGO_TARGET_DIR = "C:\rust-target\interview-coach"
cargo run
```

或者换成 MSVC 工具链:`rustup default stable-x86_64-pc-windows-msvc`。

**3. 提示「大模型未配置」/ 调用报 401**

到「设置」里检查 Base URL 与 API Key;点「测试连接」可直接看到错误信息。密钥只写在本地 `config.toml`。

**4. 简历上传后提示解析不到文本**

说明是扫描件/图片型 PDF,请把简历内容直接**粘贴**到「新建简历 → 粘贴文本」。

**5. 想换电脑/备份数据**

拷贝 `data/interview-coach.json`(全部面试与简历数据)和 `config.toml`(配置)即可。

## 开发与测试

```powershell
cargo test        # 19 个单元测试 + 6 个接口冒烟测试
cargo clippy      # 可选
```

测试不依赖真实大模型:涉及 LLM 的路径仅在未配置时校验错误码,避免测试打网络请求。

覆盖范围:

- 单元测试:Prompt 渲染、JSON 容错抽取、题库去重、薪资城市系数与年限解析、复盘解析、docx(store 与 deflate 两种压缩)与 PDF 文本抽取。
- 冒烟测试:健康检查、元数据、薪资查询、简历落盘、配置保存、未配置大模型时的错误码。
- 端到端:用本地 mock 的 OpenAI 兼容服务验证过完整链路(开始面试 → SSE 流式出题 → 逐题作答/跳题 → 结束 → 异步复盘 → 统计 → 提升计划 → 简历上传/诊断/优化/STAR → 薪资兜底),无需真实 API Key。

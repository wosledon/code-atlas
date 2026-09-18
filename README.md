# Code Atlas

**给代码仓库自动生成一套「活着」的 Wiki + 知识图谱 + 可检索知识库。**

模型不是坐在外面猜，而是带着只读工具（`read_file` / `grep` / `find_defs` / `git_log` …）在你的仓库里按需取证，写出**带 `path:line` 锚点**的文档；内容变了下一次增量更新只重写受影响的那几页。

[English](README.en.md) · 中文

![知识库对话](docs/images/chat.png)

## 目录

- [它解决什么](#它解决什么)
- [界面](#界面)
- [安装](#安装)
- [快速开始](#快速开始)
- [CLI 命令](#cli-命令)
- [MCP 接入](#mcp-接入)
- [生成行为（成本与增量）](#生成行为成本与增量)
- [压缩与体积](#压缩与体积)
- [配置](#配置)
- [项目结构](#项目结构)
- [安全提示](#安全提示)
- [开发](#开发)

## 它解决什么

接手一个陌生仓库，最先缺的从来不是"这个文件是干嘛的"，而是**跨模块的因果**：数据从哪个入口进来、在哪一层落库、改了这里会连带什么。Code Atlas 把这件事交给模型 + 图谱，产出两类可长期维护的资产：

| 资产              | 位置                         | 用途                                                                                                         |
| ----------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------ |
| **Wiki 文档**     | `atlas/*.md`                 | 按「快速上手 / 业务领域 / 系统设计 / 模块详解 / 数据模型 / 接口 / 流程 / 运维」分章，含 mermaid 图与代码锚点 |
| **知识库 + 图谱** | `data/*.db`（SQLite + FTS5） | 供 `atlas search`、Web 对话、图谱浏览和 MCP 工具检索                                                         |

三点设计取舍：

- **证据优先**：所有事实性描述都要能追到 `path:line`；读不到的文件不许编行号。
- **增量优先**：页面指纹（范围 + 证据 + 模块文件清单）未变就跳过模型，不重复烧钱。
- **费用可见**：运行汇总直接打印提示词缓存命中率、工具输出压缩省下的字符数、工具缓存命中率。

## 界面

### 项目首页

一张卡片一个仓库：页数 / 知识块 / 实体 / 运行次数、最近一次更新状态、章节入口。

![项目首页](docs/images/home.png)

### 文档阅读器

左侧目录树（可整体折叠展开）、正文带锚点与 mermaid 图；图表支持缩放、平移、全屏、复制源码。

![文档阅读器](docs/images/reader.png)

### 知识库对话

流式回答 + **召回内容面板**（页路径、行号、片段）。当召回不够时，模型会自己调用 `grep` / `read_file` 去仓库里找，再回答——所以问一个只存在于源码里的常量名也能答对。

![知识库对话](docs/images/chat.png)

### 知识图谱

实体与关系来自确定性符号图 + 模型抽取，可按邻域下钻。

![知识图谱](docs/images/graph.png)

### 运行记录与设置

每次 init / update 的模型、耗时、token、缓存命中与增量判定结果；设置页改 provider / model / 输出语言 / chunk 策略。

![运行记录](docs/images/runs.png)

![设置](docs/images/settings.png)

## 安装

### 下载预编译二进制（推荐）

从 [Releases](https://github.com/wosledon/code-atlas/releases) 下载对应平台，解压即用 —— **单个可执行文件，前端已内嵌，无需 Node、无需额外文件**：

| 平台                | 产物                                    |
| ------------------- | --------------------------------------- |
| Windows x64         | `atlas-x86_64-pc-windows-msvc.zip`      |
| Linux x64           | `atlas-x86_64-unknown-linux-gnu.tar.gz` |
| macOS Apple Silicon | `atlas-aarch64-apple-darwin.tar.gz`     |
| macOS Intel         | `atlas-x86_64-apple-darwin.tar.gz`      |

每个 Release 另外附带 `SHA256SUMS` 供校验。Linux 产物在 Ubuntu 22.04（glibc 2.35）上构建，可在更新的发行版直接运行。

### 从源码构建

```bash
cd web && npm install && npm run build && cd ..   # 必须先出前端：编译期会把它内嵌
cargo build --profile dist -p atlas-cli           # 体积优先档，见下文「压缩与体积」
```

> 前端产物是**编译期依赖**：`crates/atlas-server/build.rs` 只在 `web/dist/index.html` 存在时才把它 gzip 内嵌。顺序反了，打出来的二进制就没有 UI（运行时只会显示内置兜底页）。忘了也不会白跑 —— `scripts/smoke-binary.sh` 会检查这一点。

## 快速开始

### 1. 配置

```bash
./atlas init-config      # 生成 atlas.toml（也可全程用环境变量）
```

密钥优先级：环境变量 > `atlas.toml`。

```bash
export OPENAI_API_KEY=...      # 或 ANTHROPIC_API_KEY；本机 Ollama 可不设
export ATLAS_MODEL=...         # 可选，覆盖 atlas.toml
```

### 2. 生成 Wiki + 知识库

```bash
./atlas plan    # 先看要生成哪些页面、不调用模型
./atlas init    # 首次生成
./atlas update  # 之后增量更新（内容没变则 0 次模型调用）
```

### 3. 使用

```bash
./atlas search "认证流程"   # 命令行检索，命中含正文与行号
./atlas web                 # Web UI + REST API（默认 4321 端口）
```

## CLI 命令

```
atlas init          [--ci] [--instruction <text>] [--provider P] [--model M]
atlas update        [--ci] [--instruction <text>] [--provider P] [--model M]
atlas search <查询> [--limit N]        # 检索后端由 [kb] search 决定
atlas status [--last]                  # 查看历史运行
atlas reindex                          # 从 markdown 重建 SQLite 索引
atlas plan [--instruction <text>]      # 预览页面计划，不调用模型
atlas web [--port 4321] [--web-dist <dir>]   # 别名：serve
atlas export <目录>                    # 导出 wiki markdown
atlas check                            # 只读完整性检查（入口页、正文长度、相对链接）
atlas init-config [--example] [--force]      # 别名：config
atlas project <子命令>                 # 别名：projects，管理项目注册表
atlas mcp                              # MCP stdio 服务
```

全局参数：`--path <仓库根>`（默认自动探测）、`--project <id>`（多项目场景指定目标）。

`--ci` 下仅「真问题」才 exit 1：断链、正文过短、缺入口页、模型大面积失败回退。

## MCP 接入

`atlas mcp` 在 stdio 上提供以下工具，可挂进任意支持 MCP 的编辑器：

| 工具                                                          | 作用                                            |
| ------------------------------------------------------------- | ----------------------------------------------- |
| `atlas_search`                                                | 检索知识库，返回页路径、行范围与召回正文        |
| `atlas_read_page` / `atlas_list_pages`                        | 读写 wiki 页面清单                              |
| `atlas_write_page` / `atlas_patch_page` / `atlas_delete_page` | 写页、按行/文本局部改写、删页（可顺带 reindex） |
| `atlas_get_chunks`                                            | 查看某页切出的知识块                            |
| `atlas_list_entities` / `atlas_graph_neighborhood`            | 实体列表与邻域关系                              |
| `atlas_plan` / `atlas_check` / `atlas_reindex`                | 计划预览、完整性检查、重建索引                  |
| `atlas_update`                                                | 触发 init/update 管线（会调用模型，耗时数分钟） |
| `atlas_status` / `atlas_repo_info` / `atlas_list_projects`    | 运行记录、仓库概览、项目注册表                  |

## Web UI 与 API

`atlas web` 是唯一入口（`serve` 为兼容别名），前端默认已编入二进制，`--web-dist` 可指向外部目录。

| 页面                   | 说明                                             |
| ---------------------- | ------------------------------------------------ |
| `/`                    | 项目卡片                                         |
| `/reader?p=<rel_path>` | Wiki 阅读（深链可分享）                          |
| `/chat`                | 知识库对话                                       |
| `/graph`               | 知识图谱                                         |
| `/runs`                | 运行记录                                         |
| `/settings`            | 设置（LLM / 语言 / chunk / 输出位置 / 项目注册） |

主要接口：`/api/health` · `/api/projects` · `/api/pages` · `/api/pages/<rel_path>` · `/api/kb/search` · `POST /api/kb/chat`（SSE）· `/api/graph/nodes` · `/api/runs` · `/api/config` · `POST /api/run/update`

## 生成行为（成本与增量）

- **并发：** 撰页 worker 默认 5 个（`[llm] concurrency` / `ATLAS_CONCURRENCY`）。
- **工具：** 模型可调用只读工具 `list_files` / `list_tree` / `read_file` / `grep` / `find_defs` / `git_log` 按需采证，不把整仓正文塞进 prompt。轮数上限见 `[llm] max_tool_rounds`（`0` = 关闭工具，纯证据生成）。
- **增量：** 页指纹（focus + 证据 + 模块文件清单）未变 → 复用正文并跳过模型；正文未变 → 复用 chunk；文档树里消失的旧页会连 chunk 一起清理。
- **分块：** `[kb.chunk] mode` 默认 `hybrid`：由模型决定边界并写摘要，chunk = 摘要 + 分块内容；无模型时退化为结构切分。
- **提示词缓存：** 请求按「稳定前缀在前」组织（系统提示跨页逐字节一致 → 仓库共享证据 → 页级要求），深度重写复用首稿前缀。汇总行打印缓存命中率。
- **无密钥时：** 直接进入模板模式（一条 advisory note），不逐页重试。

## 压缩与体积

省 token 和省磁盘是两件事，这里都做了实测。

### 工具输出压缩（省 token）

模型读的是文本，所以压缩只能是**语义与字符双重保真**的改写 —— 不能用 gzip 那类需要解码器的编码。管道在 `crates/atlas-core/src/tools/compress.rs`，共四个 pass：

| pass             | 去掉什么                                   | 用在哪         |
| ---------------- | ------------------------------------------ | -------------- |
| 空白归一         | CRLF、行尾空白                             | 所有工具输出   |
| 空行折叠         | 连续 ≥3 个空行留 1 个                      | 所有工具输出   |
| **按块缩进外提** | 整块公共缩进（块首尾各一行记号）           | 仅 `read_file` |
| **重复行折叠**   | 连续 ≥4 个完全相同的行 → 保留首行 + `(xN)` | 仅 `read_file` |

缩进外提是唯一「语言相关」的一步，闸门有三道：**扩展名白名单**（不在表里的一律不动，Python / YAML / Markdown 等缩进即语法的语言因此天然安全）、**跨行字符串守卫**（Go 反引号、JS 模板串、Java/C# 三引号里的行首空格是数据，逐行标出并绕开）、**收益门槛**（省下的不够写记号就不压）。记号自解释、且不占行号：

```
// begin: 8 spaces omitted per line
12|     if sql == "" {
// end
```

补回 N 个前导空格即逐字节复原，所以 `path:line` 引用不受影响。实测本仓 85 万字符的 rs/ts/tsx/json：**按块外提省 6.2%**（对照：整窗外提 2.6%、按等缩进段分块 0.9%）。记账单位是**字符**不是字节（中文一字 3 字节 ≈ 1 token，用字节会虚高约 3 倍）。

两条管道而非一条：`read_file` 每行都带原始行号，折叠空行会让行号错位，所以它走「逐行裁剪 → 缩进外提 → 重复行折叠」；其余工具输出是自由文本，走 `compact`（行尾归一 + 空行折叠）。

### 工具结果缓存（省重复读盘与重复推理）

- **调用级**：键 = 工具名 + 规范化参数（键序无关），同一调用第二次直接返回。
- **文件快照**：`read_file` 每个文件一次运行只读一次盘，之后**任意行范围**都从快照切片 —— 窗口平移、范围重叠、二次精读全都命中。

### 二进制体积（省磁盘）

发布产物走专门的 `dist` profile，实测 **12.03 MB → 6.35 MB**：

| 措施                                 | 效果                               |
| ------------------------------------ | ---------------------------------- |
| `panic = "abort"`                    | 12.03 → 9.19 MB（-24%）            |
| `opt-level = "z"` + `lto = "fat"`    | → 6.35 MB（累计 -47%）             |
| `strip = true` + `codegen-units = 1` | 去符号表、给 LTO 让路              |
| 前端 gzip 后内嵌                     | 整个 UI 约 1.6 MB（压缩前约 5 MB） |

`opt-level = "z"` 对这类以 IO/网络为主、算力不在热点的程序几乎不掉性能。全仓未使用 `catch_unwind`，所以 `panic = "abort"` 不改变行为。最终的 Windows zip 约 **4.1 MB**。

## 配置

完整示例见 [`atlas.toml.example`](atlas.toml.example)。常用键：

| 段           | 键                                                                                           | 说明                                                       |
| ------------ | -------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| `[llm]`      | `provider` / `model` / `base_url` / `api_key`                                                | 供应商与模型；`openai-compatible` / `anthropic` / `ollama` |
|              | `temperature` / `max_output_tokens` / `concurrency` / `timeout_secs` / `retries`             | 生成参数                                                   |
|              | `max_tool_rounds`                                                                            | 读取预算（×3 次只读调用）；`0` 关闭工具                    |
|              | `depth_pass`                                                                                 | 首稿偏浅时让模型按缺口再扩写一次                           |
| `[privacy]`  | `redact_paths` / `max_file_bytes`                                                            | 永不进上下文的路径与文件大小上限                           |
| `[graph]`    | `llm_extract` / `max_neighborhood_depth` / `authoritative_min_confidence`                    | 图谱构建与下钻                                             |
| `[kb]`       | `search` / `embed_model` / `max_context_bytes`                                               | 检索后端（默认 `fts5`）与上下文预算                        |
| `[kb.chunk]` | `mode` / `target_tokens` / `max_tokens` / `min_tokens` / `summary_tokens` / `overlap_tokens` | 切块策略                                                   |

环境变量覆盖：`ATLAS_PROVIDER` / `ATLAS_MODEL` / `ATLAS_BASE_URL` / `ATLAS_API_KEY` / `ATLAS_CONCURRENCY` / `ATLAS_TOOL_ROUNDS` / `ATLAS_MAX_OUTPUT_TOKENS` 等。

## 项目结构

```
crates/
  atlas-cli        CLI 入口与命令分发
  atlas-core       配置 / 路径 / 锁 / markdown / 生成管线 / 只读仓库工具
  atlas-analyze    扫描、符号抽取、大纲、忽略规则
  atlas-llm        LLM 客户端（OpenAI 兼容 / Anthropic）、SSE 累积、工具回合、方言解析
  atlas-kb         切块（结构 / 模型 / 混合）与持久化
  atlas-store      SQLite：页面、chunk、FTS5、实体关系、运行记录
  atlas-claims     Claims 抽取与校验
  atlas-server     REST API + Web UI 托管 + MCP 服务
web/               React 19 + Material Design 3 前端
atlas/             生成的 wiki（默认输出位置）
data/              生成的 SQLite 库与锁文件
```

## 安全提示

`atlas web` **没有鉴权**，且默认绑定 `0.0.0.0`（所有网卡），以便同局域网的手机或其他机器直接打开。这意味着同一网络里任何人都能：

- 调用 `POST /api/run/update`、`POST /api/kb/chat` —— **会真实调用 LLM，花你的额度**；
- 调用 `POST /api/config` —— 改写 `atlas.toml`。

家里 / 自己的热点没问题，**办公室或公共 WiFi 下不要开**。CORS 只放行 `localhost` 来源，能挡住其它网站在你浏览器里跨源读取本机 API，但挡不住局域网内的直接请求。

另外 `[privacy] redact_paths` 里的文件（`.env`、`*.pem`、`id_rsa*` 等）永不进入模型上下文，但仍请自行确认密钥没有散落在会被读取的普通文件里。

## 开发

```bash
cargo clippy --workspace --all-targets -- -D warnings   # CI 门禁之一
cargo test --workspace                                   # 全量测试
bash scripts/smoke-binary.sh target/dist/atlas.exe       # 验证单二进制自包含
bash scripts/package-release.sh <triple> <二进制>        # 本地打包
```

推 `v*` 标签即触发 [Release 工作流](.github/workflows/release.yml)：四个平台并行构建 → 冒烟验证 → 打包 → 创建 GitHub Release 并附 `SHA256SUMS`。手动触发同一工作流只做干跑（上传为 Actions artifact，不建 Release）。

CI 不检查 `cargo fmt`：仓库里存在大量历史格式差异，加门禁需要先整体格式化。`clippy -D warnings` 与全量测试是绿的。

## 许可证

MIT，见 [LICENSE](LICENSE)。

## 文档

产品与工程完整设计见 [`docs/DESIGN.md`](docs/DESIGN.md)。

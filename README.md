# Code Atlas

Living wiki · knowledge graph · knowledge base for code repositories.

## Stack

- **Rust** workspace (`crates/*`) + **SQLite**
- **React 19** + **Material Design 3** (`web/`)
- LLM: OpenAI-compatible / Anthropic / Ollama / host-agent

## Quick start

```bash
# 1) build CLI + web（前端 gzip 后嵌入二进制；改前端后需重新 build CLI）
cd web && npm install && npm run build && cd ..
cargo build -p atlas-cli --release

# 2) optional config（也可跳过，用默认值 + 环境变量）
./target/release/atlas init-config       # 生成 atlas.toml
# 密钥：atlas.toml [llm].api_key，或环境变量（环境变量优先）
export OPENAI_API_KEY=...          # or ANTHROPIC_API_KEY / Ollama 本机可不设
export ATLAS_MODEL=...             # optional

# 3) generate wiki + KB
./target/release/atlas init

# 4) search / web UI
./target/release/atlas search "auth"      # 命中含召回正文与行号
./target/release/atlas web                # 统一入口：REST API + 前端
# 兼容旧名：
./target/release/atlas serve
```

## Web UI 入口

| 入口                                            | 说明                                                                                             |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| **`atlas web`**                                 | CLI 统一入口：REST API + 静态前端（`serve` 为兼容别名）；前端默认编入二进制，`--web-dist` 可覆盖 |
| **`atlas mcp`**                                 | MCP stdio：检索/读页/写页/行替换/删页/chunk/图谱/校验/reindex/update                             |
| **`http://127.0.0.1:4321/`**                    | 项目卡片（`/`）：名称、语言、页数/chunk、最近更新与状态，点卡片进入 Wiki                         |
| **`http://localhost:4321/reader?p=<rel_path>`** | Wiki 文档阅读（默认 `quickstart.md`，深链可分享）                                                |
| **`http://127.0.0.1:4321/chat`**                | 知识库对话：回答 + **召回内容**（页路径/行号/片段）                                              |
| **`http://127.0.0.1:4321/settings`**            | 设置页（LLM / 语言 / chunk / 输出位置）                                                          |
| **`web/`**                                      | React 19 + MUI MD3 源码；`npm run dev` 可代理到 4321                                             |

主要 API：`/api/health` · `/api/projects` · `/api/pages` · `/api/pages/<rel_path>`（仅限 `atlas/` 下的 md）· `/api/kb/search`（含 `body`/行号）· `POST /api/kb/chat`（含 `contexts[]`）· `/api/graph/nodes` · `/api/runs` · `/api/config` · `POST /api/run/update`

## 生成行为

- **并发：** 撰页 worker 默认 5 个（`[llm] concurrency` / `ATLAS_CONCURRENCY`）。
- **工具：** 模型可调用只读工具 `list_files` / `read_file` / `grep` 按需采证（轮数上限 `[llm] max_tool_rounds`，默认 6，`0` 关闭），不再把整仓正文塞进 prompt。
- **增量：** 页指纹（focus + 证据 + 模块文件清单）未变 → 复用正文并跳过模型；正文未变 → 复用 chunk；文档树里消失的旧页会连 chunk 一起清理。
- **分块：** `[kb.chunk] mode` 默认 `hybrid`（等价 `llm-semantic`）：由模型决定边界并写摘要，chunk = **简要 + 分块内容**；无模型时退化为结构切分。
- **无密钥时：** 直接进入模板模式（一条 advisory note），不会逐页重试；`--ci` 下仅「真问题」（断链、正文过短、缺入口页、模型大面积失败回退）才 exit 1。

## Docs

See `docs/DESIGN.md` for the full product / engineering spec.

<!-- ATLAS:START -->
## Atlas

生成文档 Wiki 位于 `atlas/`（入口 `atlas/quickstart.md`）。

- 检索：`atlas search "..."`
- 更新：`atlas update`
- 本地 UI：`atlas web`
<!-- ATLAS:END -->








## 代码组织约定

- **单文件 ≤ 400 行**；超过就按职责拆成同目录兄弟模块，`lib.rs` / `mod.rs` 只保留模块声明、`pub use` 重导出与少量接线（目标 ≤ 150 行）。
- **一个模块一个职责**，例如 `crates/atlas-core/src/pipeline/` 按阶段拆分（`plan` / `prepare` / `generate` / `write` / `graph` / `maintenance`），支撑逻辑单独成模块（`brief` / `outline` / `evidence` / `prompt` / `template` / `plan_modules`）。
- **拆文件只搬运、不重写**：公开路径保持不变，用 `pub use` 重新导出（`atlas_core::AtlasConfig`、`atlas_core::pipeline::preview_plan` 等）。
- 跨模块复用的项标 `pub(crate)` / `pub(super)`；结构体私有字段拆出后同样需要放宽可见性。
- 前端沿用同一规则：页面（`web/src/pages/*.tsx`）只留页面壳，可复用逻辑与子组件放 `web/src/components/<feature>/`。

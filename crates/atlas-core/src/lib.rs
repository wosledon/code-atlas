//! Atlas 核心：配置模型、路径解析、工作区脚手架与文档生成流水线。
//!
//! 模块划分：
//! - [`config`]：`atlas.toml` 配置结构与各分节默认值。
//! - [`paths`]：仓库根解析、仓库标识与哈希工具。
//! - [`scaffold`]：示例配置与 `AGENTS.md` 指针的生成。
//! - [`pipeline`]：整条「扫描 → 规划 → 生成 → 落盘 → 建图」流水线。
//! - [`tools`]：生成阶段暴露给模型的只读仓库工具。
//! - [`git`] / [`lock`] / [`markdown`]：Git 状态、运行锁与页面渲染。

pub mod git;
pub mod lock;
pub mod markdown;
pub mod pipeline;
pub mod tools;

mod config;
mod paths;
mod scaffold;

pub use config::{
    AnalyzeSection, AtlasConfig, ChunkSection, GraphSection, KbSection, LlmSection, OutputSection,
    PrivacySection,
};
pub use paths::{repo_slug, resolve_repo_root, sha256_hex};
pub use scaffold::{ensure_agents_pointer, write_config_file, write_example_config};

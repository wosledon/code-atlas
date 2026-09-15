use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSection {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default = "default_temp")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_output_tokens: u32,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_retries")]
    pub retries: u32,
    /// How many tool-calling rounds a page generation may use before the model
    /// is forced to answer. 0 disables the file/read tools.
    #[serde(default = "default_tool_rounds")]
    pub max_tool_rounds: usize,
    /// After writing a page, re-check it against the depth bar and ask the model
    /// to expand it once when it is too thin.
    #[serde(default = "default_depth_pass")]
    pub depth_pass: bool,
}

fn default_provider() -> String {
    "openai-compatible".into()
}

fn default_model() -> String {
    "qwen2.5-coder:32b".into()
}

fn default_temp() -> f64 {
    0.2
}

fn default_max_tokens() -> u32 {
    16384
}

fn default_concurrency() -> usize {
    5
}

fn default_tool_rounds() -> usize {
    6
}

fn default_depth_pass() -> bool {
    true
}

fn default_timeout() -> u64 {
    180
}

fn default_retries() -> u32 {
    3
}

impl Default for LlmSection {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: default_model(),
            base_url: String::new(),
            temperature: default_temp(),
            max_output_tokens: default_max_tokens(),
            concurrency: default_concurrency(),
            timeout_secs: default_timeout(),
            retries: default_retries(),
            max_tool_rounds: default_tool_rounds(),
            depth_pass: default_depth_pass(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySection {
    #[serde(default = "default_redact")]
    pub redact_paths: Vec<String>,
    #[serde(default = "default_max_file")]
    pub max_file_bytes: usize,
}

fn default_redact() -> Vec<String> {
    vec![
        "**/.env".into(),
        "**/*.pem".into(),
        "**/id_rsa*".into(),
        "**/credentials*.json".into(),
    ]
}

fn default_max_file() -> usize {
    262_144
}

impl Default for PrivacySection {
    fn default() -> Self {
        Self {
            redact_paths: default_redact(),
            max_file_bytes: default_max_file(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSection {
    #[serde(default = "default_true")]
    pub llm_extract: bool,
    #[serde(default = "default_depth")]
    pub max_neighborhood_depth: u32,
    #[serde(default = "default_conf")]
    pub authoritative_min_confidence: f64,
}

fn default_true() -> bool {
    true
}

fn default_depth() -> u32 {
    3
}

fn default_conf() -> f64 {
    0.7
}

impl Default for GraphSection {
    fn default() -> Self {
        Self {
            llm_extract: true,
            max_neighborhood_depth: 3,
            authoritative_min_confidence: 0.7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbSection {
    #[serde(default = "default_search")]
    pub search: String,
    #[serde(default)]
    pub embed_model: String,
    #[serde(default = "default_ctx")]
    pub max_context_bytes: usize,
    #[serde(default)]
    pub chunk: ChunkSection,
}

fn default_search() -> String {
    "fts5".into()
}

fn default_ctx() -> usize {
    24_576
}

impl Default for KbSection {
    fn default() -> Self {
        Self {
            search: default_search(),
            embed_model: String::new(),
            max_context_bytes: default_ctx(),
            chunk: ChunkSection::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSection {
    #[serde(default = "default_chunk_mode")]
    pub mode: String,
    #[serde(default = "default_target_tokens")]
    pub target_tokens: usize,
    #[serde(default)]
    pub chunk_model: String,
}

fn default_chunk_mode() -> String {
    "hybrid".into()
}

fn default_target_tokens() -> usize {
    512
}

impl Default for ChunkSection {
    fn default() -> Self {
        Self {
            mode: default_chunk_mode(),
            target_tokens: default_target_tokens(),
            chunk_model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSection {
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_atlas_root")]
    pub atlas_root: String,
    #[serde(default)]
    pub external_root: String,
    #[serde(default = "default_lang")]
    pub language: String,
}

fn default_strategy() -> String {
    "in-repo".into()
}

fn default_atlas_root() -> String {
    "atlas".into()
}

fn default_lang() -> String {
    "zh-CN".into()
}

impl Default for OutputSection {
    fn default() -> Self {
        Self {
            strategy: default_strategy(),
            atlas_root: default_atlas_root(),
            external_root: String::new(),
            language: default_lang(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeSection {
    #[serde(default = "default_langs")]
    pub languages: Vec<String>,
}

fn default_langs() -> Vec<String> {
    vec!["rust".into(), "typescript".into(), "javascript".into(), "python".into()]
}

impl Default for AnalyzeSection {
    fn default() -> Self {
        Self {
            languages: default_langs(),
        }
    }
}

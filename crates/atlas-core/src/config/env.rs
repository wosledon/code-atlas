use std::str::FromStr;

use super::AtlasConfig;

/// Environment overrides win over `atlas.toml` so CI (and the offline stub
/// tests) can switch provider / model / concurrency without editing the repo.
pub(super) fn apply_env(cfg: &mut AtlasConfig) {
    if let Ok(p) = std::env::var("ATLAS_PROVIDER") {
        cfg.llm.provider = p;
    }
    if let Ok(m) = std::env::var("ATLAS_MODEL") {
        cfg.llm.model = m;
    }
    if let Ok(u) = std::env::var("ATLAS_BASE_URL") {
        cfg.llm.base_url = u;
    }
    parse_into("ATLAS_TEMPERATURE", &mut cfg.llm.temperature);
    parse_into("ATLAS_MAX_OUTPUT_TOKENS", &mut cfg.llm.max_output_tokens);
    parse_into("ATLAS_TIMEOUT_SECS", &mut cfg.llm.timeout_secs);
    parse_into("ATLAS_CONCURRENCY", &mut cfg.llm.concurrency);
    parse_into("ATLAS_TOOL_ROUNDS", &mut cfg.llm.max_tool_rounds);
    if let Ok(t) = std::env::var("ATLAS_DEPTH_PASS") {
        cfg.llm.depth_pass = !matches!(t.trim(), "0" | "false" | "off" | "no");
    }
    if let Ok(lang) = std::env::var("ATLAS_OUTPUT_LANGUAGE") {
        cfg.output.language = lang;
    }
}

/// Ignore an unparsable value (keep the configured one) instead of failing the run.
fn parse_into<T: FromStr>(key: &str, slot: &mut T) {
    parse_or_keep(std::env::var(key).ok().as_deref(), slot);
}

fn parse_or_keep<T: FromStr>(raw: Option<&str>, slot: &mut T) {
    if let Some(v) = raw.and_then(|r| r.trim().parse().ok()) {
        *slot = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usable_values_replace_and_bad_ones_are_ignored() {
        let mut n = 4u32;
        parse_or_keep(Some(" 7 "), &mut n);
        assert_eq!(n, 7);
        parse_or_keep(Some("not-a-number"), &mut n);
        assert_eq!(n, 7, "unparsable value must keep the configured one");
        parse_or_keep(None, &mut n);
        assert_eq!(n, 7);
    }
}

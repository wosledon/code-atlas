use super::maintenance::first_heading;

#[test]
fn first_heading_skips_front_matter_and_fences() {
    assert_eq!(
        first_heading("---\ntitle: x\n---\n\n# 整体架构\n\n正文"),
        Some("整体架构".to_string())
    );
    assert_eq!(first_heading("intro\n\n# API Reference"), Some("API Reference".to_string()));
    assert_eq!(first_heading("```sh\n# not a title\n```\n\n# Real"), Some("Real".to_string()));
    assert_eq!(first_heading("## only h2"), None);
    assert_eq!(first_heading(""), None);
}

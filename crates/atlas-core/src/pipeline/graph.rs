use super::plan_modules::derive_modules;
use super::run::RunContext;
use super::*;

/// Seed the knowledge graph with the modules found by the scan plus the key
/// symbols, tools and config files that the wiki should reason about.
pub(super) fn seed_graph(run: &RunContext<'_>) -> Result<()> {
    let store = &run.session.store;
    let run_id = run.session.run_id.as_str();
    let scan: &RepoScan = run.scan;

    // entities from top-level modules
    let modules = derive_modules(scan);
    upsert_module_entities(store, run_id, &modules)?;
    // richer graph: crates + key source files as symbols
    for f in scan.files.iter().filter(|f| f.language.is_some()).take(40) {
        let name = f.rel.rsplit('/').next().unwrap_or(&f.rel).to_string();
        let key = format!("symbol:{}", f.rel);
        store.upsert_entity(
            "symbol",
            &name,
            &key,
            &serde_json::json!({ "path": f.rel, "language": f.language }),
            "active",
            run_id,
        )?;
    }
    if let Some((crate_name, _)) = modules.first() {
        let module_id = store.upsert_entity(
            "module",
            crate_name,
            &format!("module:{crate_name}"),
            &serde_json::json!({}),
            "active",
            run_id,
        )?;
        for f in scan.files.iter().filter(|f| f.rel.starts_with("crates/")).take(30) {
            let key = format!("symbol:{}", f.rel);
            let sid = store.upsert_entity(
                "symbol",
                f.rel.rsplit('/').next().unwrap_or(&f.rel),
                &key,
                &serde_json::json!({ "path": f.rel }),
                "active",
                run_id,
            )?;
            store.upsert_relation(&module_id, &sid, "part_of", &serde_json::json!({}), 1.0, run_id)?;
        }
        // tool / config entities
        let cli_id = store.upsert_entity(
            "module",
            "atlas-cli",
            "module:atlas-cli",
            &serde_json::json!({ "path": "crates/atlas-cli" }),
            "active",
            run_id,
        )?;
        let srv_id = store.upsert_entity(
            "module",
            "atlas-server",
            "module:atlas-server",
            &serde_json::json!({ "path": "crates/atlas-server" }),
            "active",
            run_id,
        )?;
        let web_id = store.upsert_entity(
            "module",
            "web",
            "module:web",
            &serde_json::json!({ "path": "web" }),
            "active",
            run_id,
        )?;
        store.upsert_relation(&cli_id, &srv_id, "depends_on", &serde_json::json!({}), 1.0, run_id)?;
        store.upsert_relation(&srv_id, &web_id, "depends_on", &serde_json::json!({}), 0.9, run_id)?;
        let cfg_id = store.upsert_entity(
            "config",
            "atlas.toml",
            "config:atlas.toml",
            &serde_json::json!({ "path": "atlas.toml.example" }),
            "active",
            run_id,
        )?;
        store.upsert_relation(&cli_id, &cfg_id, "configured_by", &serde_json::json!({}), 1.0, run_id)?;
    }
    Ok(())
}

use anyhow::Result;
use atlas_store::{ChunkRow, Store};
use sha2::{Digest, Sha256};

use super::spec::ChunkSpec;

/// Replace a page's chunks in one transaction. The id is derived from
/// (page, ordinal, body) so re-indexing the same content keeps ids stable.
pub fn store_chunks(
    store: &Store,
    page_path: &str,
    run_id: &str,
    specs: &[ChunkSpec],
    source: &str,
) -> Result<()> {
    let rows: Vec<ChunkRow> = specs
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut h = Sha256::new();
            h.update(page_path.as_bytes());
            h.update(i.to_string().as_bytes());
            h.update(c.body.as_bytes());
            let id = format!("ck_{}", &hex::encode(h.finalize())[..12]);
            ChunkRow {
                id,
                page_path: page_path.to_string(),
                ord: i as i64,
                title: c.title.clone(),
                summary: c.summary.clone(),
                body: c.body.clone(),
                start_line: c.start_line as i64,
                end_line: c.end_line as i64,
                source: source.to_string(),
            }
        })
        .collect();
    store.replace_chunks_for_page(page_path, run_id, &rows)?;
    Ok(())
}

pub fn upsert_module_entities(store: &Store, run_id: &str, modules: &[(String, String)]) -> Result<()> {
    // modules: (canonical_name, path)
    for (name, path) in modules {
        let key = format!("module:{name}");
        store.upsert_entity(
            "module",
            name,
            &key,
            &serde_json::json!({ "path": path }),
            "active",
            run_id,
        )?;
    }
    Ok(())
}

pub fn link_page_to_module(store: &Store, run_id: &str, page: &str, module_name: &str) -> Result<()> {
    let page_key = format!("page:{}", page);
    let page_id = store.upsert_entity(
        "page",
        page,
        &page_key,
        &serde_json::json!({ "path": page }),
        "active",
        run_id,
    )?;
    let module_key = format!("module:{module_name}");
    let module_id = store.upsert_entity(
        "module",
        module_name,
        &module_key,
        &serde_json::json!({}),
        "active",
        run_id,
    )?;
    store.upsert_relation(&page_id, &module_id, "describes", &serde_json::json!({}), 1.0, run_id)?;
    Ok(())
}

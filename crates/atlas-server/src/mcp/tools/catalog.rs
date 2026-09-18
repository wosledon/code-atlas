use serde_json::{json, Value};

pub(crate) fn tool_defs() -> Value {
    json!([
        tool("atlas_search", "Search the knowledge base for one project. Hits include page_path, line range and recalled body text.", json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 50, "description": "default 10"},
                "project": {"type": "string", "description": "project id; default = launch repo"}
            },
            "required": ["query"]
        })),
        tool("atlas_read_page", "Read one generated wiki markdown page (path under that project's atlas root).", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "e.g. quickstart.md"},
                "project": {"type": "string"}
            },
            "required": ["path"]
        })),
        tool("atlas_list_pages", "List generated wiki markdown pages for a project.", json!({
            "type": "object",
            "properties": {"project": {"type": "string"}}
        })),
        tool("atlas_get_chunks", "Return KB chunks indexed for one page (ord, lines, body).", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "page path, e.g. quickstart.md"},
                "project": {"type": "string"}
            },
            "required": ["path"]
        })),
        tool("atlas_status", "Show recent generation runs for a project.", json!({
            "type": "object",
            "properties": {
                "last": {"type": "boolean", "description": "only the most recent run"},
                "project": {"type": "string", "description": "project id; default = launch repo"}
            }
        })),
        tool("atlas_repo_info", "Repo / wiki overview for a project. Also lists registered projects.", json!({
            "type": "object",
            "properties": {
                "project": {"type": "string", "description": "project id; default = launch repo"}
            }
        })),
        tool("atlas_list_projects", "List projects this MCP can update: launch repo plus atlas.projects.json entries.", json!({"type": "object", "properties": {}})),
        tool("atlas_plan", "Preview the documentation page plan without calling an LLM.", json!({
            "type": "object",
            "properties": {
                "instruction": {"type": "string"},
                "project": {"type": "string"}
            }
        })),
        tool("atlas_check", "Read-only integrity check (entry pages, link targets).", json!({
            "type": "object",
            "properties": {"project": {"type": "string"}}
        })),
        tool("atlas_reindex", "Rebuild the SQLite index from markdown on disk for one project.", json!({
            "type": "object",
            "properties": {"project": {"type": "string"}}
        })),
        tool("atlas_update", "Run init/update pipeline for one project (may call the LLM; takes minutes). Documentation updates are project-scoped.", json!({
            "type": "object",
            "properties": {
                "mode": {"type": "string", "enum": ["update", "init"], "description": "default update"},
                "instruction": {"type": "string"},
                "project": {"type": "string", "description": "project id from atlas_list_projects; default = launch repo"}
            }
        })),
        tool("atlas_write_page", "Create or overwrite one wiki markdown page under a project's atlas root. Body may include YAML front matter (`---` ... `---`); if omitted, type/title/description arguments are used. Optionally reindexes the KB.", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "relative path under atlas root, e.g. 03-模块详解/foo.md"},
                "body": {"type": "string", "description": "markdown body (with or without front matter)"},
                "type": {"type": "string", "description": "page type when body has no front matter, e.g. Module"},
                "title": {"type": "string"},
                "description": {"type": "string"},
                "reindex": {"type": "boolean", "description": "rebuild KB index after write (default true)"},
                "project": {"type": "string"}
            },
            "required": ["path", "body"]
        })),
        tool("atlas_delete_page", "Delete one wiki markdown page under a project's atlas root and reindex.", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "reindex": {"type": "boolean", "description": "default true"},
                "project": {"type": "string"}
            },
            "required": ["path"]
        })),
        tool("atlas_patch_page", "Line- or text-level edit of one wiki page. Either find/replace, or replace a 1-based inclusive line range with text.", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "find": {"type": "string", "description": "literal string to find"},
                "replace": {"type": "string", "description": "replacement for find"},
                "replace_all": {"type": "boolean", "description": "replace every occurrence (default false = first only)"},
                "start_line": {"type": "integer", "description": "1-based first line to replace"},
                "end_line": {"type": "integer", "description": "1-based last line to replace (inclusive)"},
                "text": {"type": "string", "description": "new text for the line range (may contain newlines)"},
                "reindex": {"type": "boolean", "description": "default true"},
                "project": {"type": "string"}
            },
            "required": ["path"]
        })),
        tool("atlas_list_entities", "List knowledge-graph entities for a project, optionally filtered by kind.", json!({
            "type": "object",
            "properties": {
                "kind": {"type": "string"},
                "limit": {"type": "integer", "default": 50},
                "project": {"type": "string"}
            }
        })),
        tool("atlas_graph_neighborhood", "Relations touching one entity id (src/dst/rel triples).", json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "limit": {"type": "integer", "default": 50},
                "project": {"type": "string"}
            },
            "required": ["id"]
        }))
    ])
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": schema})
}

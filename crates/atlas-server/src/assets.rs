use super::*;
use crate::auth::{authorized, deny};

pub(crate) async fn health(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return deny();
    }
    Json(json!({
        "ok": true,
        "atlas_root": state.atlas_root.display().to_string(),
        "language": state.cfg.output.language,
        "provider": state.cfg.llm.provider,
        "model": state.cfg.llm.model,
    }))
    .into_response()
}


pub(crate) async fn embedded_index() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Code Atlas</title>
<style>
:root { color-scheme: light dark; font-family: "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif; }
body { margin: 0; background: #f7f8fa; color: #1c232b; }
main { max-width: 960px; margin: 40px auto; padding: 0 24px; }
h1 { font-weight: 600; letter-spacing: -0.01em; }
input, button { font: inherit; padding: 10px 12px; border-radius: 8px; border: 1px solid #c9d1db; }
button { background: #1a6fb5; color: #fff; border: none; cursor: pointer; margin-left: 8px; }
pre { background: #eef1f5; padding: 16px; border-radius: 12px; overflow: auto; }
a { color: #1a6fb5; }
@media (prefers-color-scheme: dark) {
  body { background: #121418; color: #e8ecf2; }
  input { background: #1e232b; color: #e8ecf2; border-color: #4a5563; }
  pre { background: #1e232b; }
}
</style>
</head>
<body>
<main>
  <h1>Code Atlas</h1>
  <p>本地知识库 / 图谱 / Wiki。完整 React + MD3 界面见 <code>web/</code> 构建产物；此页为内置回退 UI。</p>
  <p>
    <input id="token" placeholder="token (from URL ?t=)" size="28" />
    <button onclick="saveToken()">保存</button>
    <button onclick="load()">刷新</button>
  </p>
  <p>
    <input id="q" placeholder="搜索知识库…" size="36" />
    <button onclick="search()">搜索</button>
  </p>
  <pre id="out">加载中…</pre>
</main>
<script>
function token() {
  const u = new URLSearchParams(location.search).get('t');
  if (u) { localStorage.setItem('atlas_token', u); return u; }
  return localStorage.getItem('atlas_token') || '';
}
function saveToken() {
  localStorage.setItem('atlas_token', document.getElementById('token').value.trim());
  load();
}
async function api(path) {
  const r = await fetch(path, { headers: { Authorization: 'Bearer ' + token() } });
  return r.json();
}
async function load() {
  const health = await api('/api/health').catch(e => ({ error: String(e) }));
  const runs = await api('/api/runs').catch(() => []);
  document.getElementById('out').textContent = JSON.stringify({ health, runs: runs.slice?.(0, 5) ?? runs }, null, 2);
}
async function search() {
  const q = document.getElementById('q').value;
  const res = await api('/api/kb/search?q=' + encodeURIComponent(q));
  document.getElementById('out').textContent = JSON.stringify(res, null, 2);
}
document.getElementById('token').value = token();
load();
</script>
</body>
</html>"#,
    )
}

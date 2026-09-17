//! SPA static assets: on-disk override, compile-time embed, or minimal fallback.

use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use std::path::Path;

/// True when `web/dist` was compiled into this binary.
pub fn has_embedded_ui() -> bool {
    #[cfg(embedded_ui)]
    {
        true
    }
    #[cfg(not(embedded_ui))]
    {
        false
    }
}

pub(crate) enum UiSource {
    Disk(std::path::PathBuf),
    Embedded,
    Fallback,
}

pub(crate) fn pick_ui_source(web_dist: Option<&Path>) -> UiSource {
    if let Some(d) = web_dist.filter(|p| p.join("index.html").exists()) {
        return UiSource::Disk(d.to_path_buf());
    }
    if has_embedded_ui() {
        return UiSource::Embedded;
    }
    UiSource::Fallback
}

pub(crate) async fn serve_embedded(
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
) -> Response {
    #[cfg(embedded_ui)]
    {
        use axum::body::Bytes;
        use rust_embed::RustEmbed;

        #[derive(RustEmbed)]
        #[folder = "web-ui-embed"]
        struct Assets;

        let path = uri.path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };
        let spa_fallback = path != "index.html" && !path.starts_with("assets/");
        let lookup = if spa_fallback { "index.html" } else { path };
        let file = Assets::get(lookup).or_else(|| Assets::get("index.html"));
        return match file {
            Some(f) => {
                let accepts_gzip = headers
                    .get(header::ACCEPT_ENCODING)
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.contains("gzip"))
                    .unwrap_or(false);

                let mime_path = if spa_fallback { "index.html" } else { path };
                let mime = mime_guess::from_path(mime_path).first_or_octet_stream();
                let mut resp = if accepts_gzip {
                    let mut r = Response::new(Body::from(f.data.into_owned()));
                    r.headers_mut().insert(
                        header::CONTENT_ENCODING,
                        header::HeaderValue::from_static("gzip"),
                    );
                    r.headers_mut()
                        .insert(header::VARY, header::HeaderValue::from_static("accept-encoding"));
                    r
                } else {
                    use std::io::Read;
                    let mut dec = flate2::read::GzDecoder::new(&f.data[..]);
                    let mut buf = Vec::new();
                    let raw = if dec.read_to_end(&mut buf).is_ok() {
                        Bytes::from(buf)
                    } else {
                        Bytes::from(f.data.into_owned())
                    };
                    Response::new(Body::from(raw))
                };
                resp.headers_mut().insert(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_str(mime.as_ref()).unwrap_or_else(|_| {
                        header::HeaderValue::from_static("application/octet-stream")
                    }),
                );
                // Hashed Vite assets are immutable; index.html must revalidate.
                let cache = if mime_path == "index.html" || !mime_path.starts_with("assets/") {
                    "no-cache"
                } else {
                    "public, max-age=31536000, immutable"
                };
                if let Ok(v) = header::HeaderValue::from_str(cache) {
                    resp.headers_mut().insert(header::CACHE_CONTROL, v);
                }
                resp
            }
            None => (StatusCode::NOT_FOUND, "not found").into_response(),
        };
    }
    #[cfg(not(embedded_ui))]
    {
        let _ = (uri, headers);
        fallback_page().await
    }
}

pub(crate) async fn fallback_page() -> Html<&'static str> {
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
  <p>本地知识库 / 图谱 / Wiki。完整 React + MD3 界面未编入本二进制；此页为内置回退 UI。</p>
  <p>构建完整 UI：<code>cd web && npm run build</code> 后重新 <code>cargo build -p atlas-cli</code>，或用 <code>--web-dist</code> 指向 dist。</p>
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

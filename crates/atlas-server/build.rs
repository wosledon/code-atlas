use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dist = manifest.join("../../web/dist");
    let dist = dist.canonicalize().unwrap_or(dist);
    let out = manifest.join("web-ui-embed");

    println!("cargo:rerun-if-changed={}", dist.display());
    println!("cargo:rerun-if-changed={}", dist.join("index.html").display());

    if !dist.join("index.html").exists() {
        // No frontend build: leave embed folder empty; runtime uses fallback UI.
        let _ = fs::remove_dir_all(&out);
        return;
    }

    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).expect("create web-ui-embed");
    gzip_tree(&dist, &dist, &out).expect("gzip web/dist into web-ui-embed");
    println!("cargo:rustc-cfg=embedded_ui");
}

fn gzip_tree(root: &Path, dir: &Path, out_root: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            gzip_tree(root, &path, out_root)?;
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let dest = out_root.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut input = fs::File::open(&path)?;
        let mut raw = Vec::new();
        input.read_to_end(&mut raw)?;
        let mut encoder =
            flate2::write::GzEncoder::new(fs::File::create(&dest)?, flate2::Compression::best());
        encoder.write_all(&raw)?;
        encoder.finish()?;
    }
    Ok(())
}

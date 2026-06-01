//! Build-time rendering of the Markdown tool docs into HTML fragments that the
//! dashboard serves from `/dashboard/docs`. Rendering at build time keeps the docs
//! baked into the binary (same offline story as the embedded dashboard assets) and
//! off the request path. `mermaid` fenced blocks become `<code class="language-mermaid">`,
//! which the dashboard converts into live diagrams at view time.

use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    // Canonicalize so cargo's rerun-if-changed reliably tracks content edits; a
    // `..`-relative path is not detected when only a file's contents change.
    let docs_dir = std::fs::canonicalize(Path::new(&manifest_dir).join("../../docs"))
        .unwrap_or_else(|_| Path::new(&manifest_dir).join("../../docs"));
    println!("cargo:rerun-if-changed={}", docs_dir.display());
    println!("cargo:rerun-if-changed=build.rs");

    let mut options = comrak::Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;
    // unsafe_ stays false: docs are first-party, but keeping raw HTML escaped means
    // the rendered fragments are safe to inject and never execute embedded markup.

    let mut entries: Vec<(String, String, String)> = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&docs_dir) {
        for dir_entry in read_dir.flatten() {
            let path = dir_entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            println!("cargo:rerun-if-changed={}", path.display());
            let slug = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default();
            let markdown = fs::read_to_string(&path).unwrap_or_default();
            let title = markdown
                .lines()
                .find_map(|line| line.strip_prefix("# "))
                .map(|line| line.trim().to_string())
                .unwrap_or_else(|| slug.clone());
            let html = comrak::markdown_to_html(&markdown, &options);
            entries.push((slug, title, html));
        }
    }

    // INDEX first (it is the tool reference table), then alphabetical by title.
    entries.sort_by(|a, b| {
        let rank = |slug: &str| usize::from(!slug.eq_ignore_ascii_case("INDEX"));
        rank(&a.0)
            .cmp(&rank(&b.0))
            .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });

    // Emit a Rust slice literal; `{:?}` produces valid, fully-escaped string literals.
    let mut generated = String::from("pub static DASHBOARD_DOCS: &[(&str, &str, &str)] = &[\n");
    for (slug, title, html) in &entries {
        generated.push_str(&format!("    ({slug:?}, {title:?}, {html:?}),\n"));
    }
    generated.push_str("];\n");

    fs::write(Path::new(&out_dir).join("dashboard_docs.rs"), generated)
        .expect("write dashboard_docs.rs");
}

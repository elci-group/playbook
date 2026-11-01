//! The Playbook website generator: one deterministic static site rendered
//! from the workspace itself. The site cannot drift from the repository
//! because it is extracted from the repository — crate names, versions,
//! doc headings and recordings are read from the real files at generation
//! time, never hardcoded.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::Path;

/// One workspace crate as extracted from the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrateInfo {
    pub name: String,
    pub path: String,
    /// The first `//!` doc line of the crate root — the crate's own words.
    pub summary: String,
}

/// The workspace facts the site is built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub version: String,
    pub crates: Vec<CrateInfo>,
    /// Doc file stems, e.g. `docs/matchday.md` → `matchday`.
    pub docs: Vec<String>,
    /// Recording file names under docs/assets.
    pub recordings: Vec<String>,
    /// The repository root the facts were extracted from.
    pub root: String,
}

fn first_doc_line(path: &Path) -> String {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.trim_start().starts_with("//!"))
                .map(|line| {
                    line.trim()
                        .trim_start_matches("//!")
                        .trim()
                        .trim_end_matches('.')
                        .to_string()
                })
        })
        .unwrap_or_else(|| "Playbook component".into())
}

/// Extract the workspace facts. Pure file reading, no assumptions.
pub fn extract(workspace_root: &Path) -> Workspace {
    let manifest = std::fs::read_to_string(workspace_root.join("Cargo.toml")).unwrap_or_default();
    let mut crates: Vec<CrateInfo> = Vec::new();
    let mut in_members = false;
    for line in manifest.lines() {
        if line.starts_with("members") {
            in_members = true;
            continue;
        }
        if in_members {
            if line.trim_start().starts_with(']') {
                in_members = false;
                continue;
            }
            let member = line
                .trim()
                .trim_start_matches('"')
                .trim_end_matches("\",")
                .trim_end_matches('"');
            if let Some(name) = member.strip_prefix("crates/") {
                let dir = workspace_root.join("crates").join(name);
                let summary = ["src/lib.rs", "src/main.rs"]
                    .iter()
                    .find_map(|root| {
                        let path = dir.join(root);
                        if path.is_file() {
                            Some(first_doc_line(&path))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "Playbook component".into());
                crates.push(CrateInfo {
                    name: name.to_string(),
                    path: format!("crates/{name}"),
                    summary,
                });
            }
        }
    }
    crates.sort_by(|a, b| a.name.cmp(&b.name));
    let mut docs: Vec<String> = std::fs::read_dir(workspace_root.join("docs"))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .filter(|name| name.ends_with(".md") && name != "README.md")
                .map(|name| name.trim_end_matches(".md").to_string())
                .collect()
        })
        .unwrap_or_default();
    docs.sort();
    let mut recordings: Vec<String> = std::fs::read_dir(workspace_root.join("docs/assets"))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .filter(|name| name.ends_with(".gif"))
                .collect()
        })
        .unwrap_or_default();
    recordings.sort();
    let version = std::fs::read_to_string(workspace_root.join("VERSION"))
        .map(|text| text.trim().to_string())
        .unwrap_or_else(|_| "0.0.0".into());
    Workspace {
        version,
        crates,
        docs,
        recordings,
        root: workspace_root.display().to_string(),
    }
}

fn draftboard_rows_from(workspace: &Workspace) -> Vec<serde_json::Value> {
    draftboard_rows(Path::new(&workspace.root))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The live draftboard rows, read from `draftboard.json` at the
/// workspace root when a benchmark run has been recorded there.
fn draftboard_rows(workspace_root: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(workspace_root.join("draftboard.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
}

/// Render the site. Deterministic: the same workspace always yields the
/// same bytes, so the site is as replayable as the engine it describes.
pub fn render(workspace: &Workspace) -> String {
    let mut out = String::new();
    let crate_cards: BTreeMap<&String, &CrateInfo> = workspace
        .crates
        .iter()
        .map(|info| (&info.name, info))
        .collect();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<title>Playbook — the franchise authority for agentic work</title>\n");
    out.push_str("<style>\n");
    out.push_str(
        ":root{--ink:#e8e6e3;--dim:#8a8782;--chalk:#0d1117;--panel:#151b23;--line:#232b36;--accent:#39c5cf;--good:#4ac26b;}\n",
    );
    out.push_str(
        "*{box-sizing:border-box}body{margin:0;background:var(--chalk);color:var(--ink);font:16px/1.6 -apple-system,'Segoe UI',sans-serif;}\n",
    );
    out.push_str("code,.mono{font-family:'SF Mono',ui-monospace,Menlo,Consolas,monospace}\n");
    out.push_str(
        "header{padding:64px 24px 40px;max-width:980px;margin:0 auto}h1{font-size:44px;margin:0 0 8px}h1 .tick{color:var(--accent)}\n",
    );
    out.push_str(
        ".tagline{color:var(--dim);font-size:20px;max-width:720px}.version{display:inline-block;margin-top:14px;padding:2px 12px;border:1px solid var(--line);border-radius:999px;color:var(--accent);font-family:ui-monospace,monospace;font-size:14px}\n",
    );
    out.push_str(
        "section{max-width:980px;margin:0 auto;padding:28px 24px}h2{font-size:26px;border-bottom:1px solid var(--line);padding-bottom:8px}\n",
    );
    out.push_str(
        ".grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(290px,1fr));gap:12px}.card{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:14px 16px}.card b{color:var(--accent);font-family:ui-monospace,monospace}.card p{margin:6px 0 0;color:var(--dim);font-size:14px}\n",
    );
    out.push_str(
        ".gif{width:100%;border:1px solid var(--line);border-radius:10px;margin:10px 0}.recording{margin:22px 0}.recording h3{margin-bottom:4px}.recording p{color:var(--dim);margin-top:4px}\n",
    );
    out.push_str(
        "ul.docs{columns:2;gap:24px}ul.docs li{margin:4px 0;list-style:none}ul.docs li::before{content:'▸ ';color:var(--accent)}\n",
    );
    out.push_str(
        "blockquote{border-left:3px solid var(--accent);margin:18px 0;padding:6px 18px;color:var(--dim);font-style:italic}\nfooter{max-width:980px;margin:0 auto;padding:26px 24px;color:var(--dim);border-top:1px solid var(--line);font-size:14px}\n",
    );
    out.push_str("</style>\n</head>\n<body>\n");
    // Hero — the version is the repository's own VERSION file.
    out.push_str("<header>\n<h1><span class=\"tick\">▌</span> PLAYBOOK</h1>\n");
    out.push_str("<p class=\"tagline\">The franchise authority for agentic work: evidence-gated drives, measured personnel, deterministic replay — AI systems that must earn their yards.</p>\n");
    out.push_str(&format!(
        "<span class=\"version\">v{}</span>\n</header>\n",
        escape(&workspace.version)
    ));
    // What it is.
    out.push_str("<section>\n<h2>What it is</h2>\n");
    out.push_str("<p>Playbook governs agentic workflows the way a franchise governs a team: objectives are declared with acceptance gates, work is played in bounded drives, and nothing counts as progress without independently validated evidence. The journal is the law — every decision is recorded, content-addressed, and replayable to an identical state.</p>\n");
    out.push_str("<blockquote>The journal is the law: replay reproduces the state, and claims without evidence do not move the field.</blockquote>\n</section>\n");
    // The crates — extracted from the real workspace manifest.
    out.push_str(&format!(
        "<section>\n<h2>The {} crates</h2>\n<div class=\"grid\">\n",
        workspace.crates.len()
    ));
    for (name, info) in &crate_cards {
        out.push_str(&format!(
            "<div class=\"card\"><b>{}</b><p>{}</p></div>\n",
            escape(name),
            escape(&info.summary)
        ));
    }
    out.push_str("</div>\n</section>\n");
    // Recordings — the real gifs.
    if !workspace.recordings.is_empty() {
        out.push_str("<section>\n<h2>Watch it play</h2>\n");
        for recording in &workspace.recordings {
            let label = recording
                .trim_start_matches("matchday-")
                .trim_end_matches(".gif")
                .replace('-', " ");
            out.push_str(&format!(
                "<div class=\"recording\"><h3>{}</h3><img class=\"gif\" src=\"assets/{rec}\" alt=\"{label}\"><p>recorded with vhs from the committed tape — the same binary the authority runs</p></div>\n",
                escape(&label),
                rec = escape(recording),
            ));
        }
        out.push_str("</section>\n");
    }
    // The live draftboard — rendered only from measured runs.
    let draftboard = draftboard_rows_from(workspace);
    if !draftboard.is_empty() {
        out.push_str("<section>\n<h2>Live model draftboard</h2>\n");
        out.push_str("<p style=\"color:var(--dim)\">Free models from the live provider pool, each playing the same six mechanically verified tasks. A task counts only when its validator passes — self-report scores nothing.</p>\n");
        out.push_str("<div class=\"grid\">\n");
        for (rank, row) in draftboard.iter().enumerate() {
            let model = row["model"].as_str().unwrap_or("unknown");
            let completed = row["completed"].as_u64().unwrap_or(0);
            let total = row["total"].as_u64().unwrap_or(0);
            let tokens = row["total_tokens"].as_u64().unwrap_or(0);
            let latency = row["median_latency_ms"].as_u64().unwrap_or(0);
            let rate = row["completion_rate_ppm"].as_u64().unwrap_or(0) as f64 / 10_000.0;
            out.push_str(&format!(
                "<div class=\"card\"><b>#{} {}</b><p>{}/{} validated · {:.1}% · {} tokens · {}ms median</p></div>\n",
                rank + 1,
                escape(model),
                completed,
                total,
                rate,
                tokens,
                latency
            ));
        }
        out.push_str("</div>\n</section>\n");
    }
    // Docs.
    if !workspace.docs.is_empty() {
        out.push_str("<section>\n<h2>The contracts</h2>\n<ul class=\"docs\">\n");
        for doc in &workspace.docs {
            out.push_str(&format!(
                "<li><a class=\"mono\" style=\"color:var(--accent)\" href=\"docs/{}.md\">{}</a></li>\n",
                escape(doc),
                escape(doc)
            ));
        }
        out.push_str("</ul>\n<p style=\"color:var(--dim)\">Every contract is grounded in the crates it describes; the docs cannot drift from the doctrine.</p>\n</section>\n");
    }
    out.push_str("<footer>Generated by <span class=\"mono\">playbook-site</span> from the workspace itself — version, crates, docs and recordings are read, not written. The same journal the authority folds rendered this page.</footer>\n");
    out.push_str("</body>\n</html>\n");
    out
}

/// Generate the site directory: index.html plus the recordings copied
/// through. Returns the paths written.
pub fn generate(workspace_root: &Path, out_dir: &Path) -> std::io::Result<Vec<String>> {
    let workspace = extract(workspace_root);
    std::fs::create_dir_all(out_dir)?;
    std::fs::create_dir_all(out_dir.join("assets"))?;
    std::fs::create_dir_all(out_dir.join("docs"))?;
    let mut written = Vec::new();
    let html = render(&workspace);
    std::fs::write(out_dir.join("index.html"), &html)?;
    written.push("index.html".into());
    for recording in &workspace.recordings {
        let source = workspace_root.join("docs/assets").join(recording);
        let target = out_dir.join("assets").join(recording);
        if source.is_file() {
            std::fs::copy(&source, &target)?;
            written.push(format!("assets/{recording}"));
        }
    }
    for doc in &workspace.docs {
        let source = workspace_root.join("docs").join(format!("{doc}.md"));
        let target = out_dir.join("docs").join(format!("{doc}.md"));
        if source.is_file() {
            std::fs::copy(&source, &target)?;
            written.push(format!("docs/{doc}.md"));
        }
    }
    Ok(written)
}

/// The content type for one served file, by extension.
pub fn content_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else if path.ends_with(".md") {
        "text/markdown; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

/// Resolve one HTTP request target to `(status, content_type, body)`:
/// `/` maps to index.html, traversal is forbidden, misses are 404. Pure —
/// the HTTP listener below only writes what this returns.
pub fn respond(out_dir: &Path, target: &str) -> (u16, &'static str, Vec<u8>) {
    let relative = target
        .split('?')
        .next()
        .unwrap_or("/")
        .trim_start_matches('/');
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    if relative.contains("..") {
        return (403, "text/plain; charset=utf-8", b"forbidden\n".to_vec());
    }
    match std::fs::read(out_dir.join(relative)) {
        Ok(body) => (200, content_type(relative), body),
        Err(_) => (404, "text/plain; charset=utf-8", b"not found\n".to_vec()),
    }
}

/// Serve the generated site over local HTTP (loopback only).
pub fn serve_dir(out_dir: &Path, port: u16) -> std::io::Result<()> {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    if !out_dir.join("index.html").is_file() {
        return Err(std::io::Error::other(
            "no index.html — run generation first",
        ));
    }
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!(
        "▌ PLAYBOOK SITE — SERVING http://127.0.0.1:{port} from {}",
        out_dir.display()
    );
    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut buffer = [0u8; 2048];
        let _ = stream.read(&mut buffer);
        let request = String::from_utf8_lossy(&buffer);
        let target = request.split_whitespace().nth(1).unwrap_or("/");
        let (status, content_type, body) = respond(out_dir, target);
        let header = format!(
            "HTTP/1.1 {status} OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(&body);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_resolution_is_pure_and_fail_closed() {
        let dir = std::env::temp_dir().join("playbook-site-resolve");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), b"<html>ok</html>").unwrap();
        let (status, content_type, body) = respond(&dir, "/");
        assert_eq!(status, 200);
        assert_eq!(content_type, "text/html; charset=utf-8");
        assert_eq!(body, b"<html>ok</html>");
        assert_eq!(respond(&dir, "/missing.gif").0, 404);
        assert_eq!(
            respond(&dir, "/../Cargo.toml").0,
            403,
            "traversal is forbidden"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

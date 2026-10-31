//! `playbook-site` — render and serve the Playbook website.

fn main() {
    let mut out = None;
    let mut workspace = None;
    let mut port: u16 = 8088;
    let mut serve = false;
    let mut all: Vec<String> = std::env::args().skip(1).collect();
    if all.first().map(String::as_str) == Some("serve") {
        serve = true;
        all.remove(0);
    }
    let mut args = all.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().map(std::path::PathBuf::from),
            "--workspace" => workspace = args.next().map(std::path::PathBuf::from),
            "--port" => {
                if let Some(value) = args.next().and_then(|v| v.parse::<u16>().ok()) {
                    port = value;
                }
            }
            "--help" | "-h" | "help" => {
                println!("playbook-site — the Playbook website generator and server\n");
                println!("usage:");
                println!("  playbook-site --workspace <repo-root> --out <dir>");
                println!("  playbook-site serve --out <dir> [--port 8088]");
                println!();
                println!("Generation renders index.html from the workspace itself: crates,");
                println!("version, docs and recordings are extracted, never hardcoded.");
                println!("Serve is a dependency-free local HTTP listener over the output.");
                return;
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }
    let Some(out) = out else {
        eprintln!("--out is required");
        std::process::exit(2);
    };
    if serve {
        if let Err(error) = playbook_site::serve_dir(&out, port) {
            eprintln!("serve failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    let Some(workspace) = workspace else {
        eprintln!("--workspace is required");
        std::process::exit(2);
    };
    match playbook_site::generate(&workspace, &out) {
        Ok(written) => {
            println!("▌ PLAYBOOK SITE — GENERATED");
            for path in written {
                println!("  ✓ {path}");
            }
        }
        Err(error) => {
            eprintln!("generation failed: {error}");
            std::process::exit(1);
        }
    }
}

//! `playbook-site` — render the Playbook website from the workspace.

fn main() {
    let mut out = None;
    let mut workspace = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().map(std::path::PathBuf::from),
            "--workspace" => workspace = args.next().map(std::path::PathBuf::from),
            "--help" | "-h" | "help" => {
                println!("playbook-site — the Playbook website generator\n");
                println!("usage: playbook-site --workspace <repo-root> --out <dir>");
                println!("  renders index.html from the workspace itself: crates, version,");
                println!("  docs and recordings are extracted, never hardcoded.");
                return;
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }
    let Some(workspace) = workspace else {
        eprintln!("--workspace is required");
        std::process::exit(2);
    };
    let Some(out) = out else {
        eprintln!("--out is required");
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

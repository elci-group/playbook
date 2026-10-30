//! Entry point for the continuous drive service.

use std::process::Command;

const RESTART_GUARD: &str = "PLAYBOOK_THEOSIS_RESTARTED";

#[tracing::instrument]
fn main() {
    if std::env::args().any(|arg| arg == "--version") {
        println!("playbook-server {}", env!("PLAYBOOK_VERSION"));
        return;
    }
    let mut config = playbook_server::ServerConfig::default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut automatic_update = false;
    let mut update_remote = std::env::var("PLAYBOOK_UPDATE_REMOTE").ok();
    let mut update_ref = "main".to_string();
    let mut index = 0;
    while index < args.len() {
        let value = |_flag: &str| -> Option<String> {
            let next = args.get(index + 1)?;
            Some(next.to_string())
        };
        let takes_value = match args[index].as_str() {
            "--auto-update" => {
                automatic_update = true;
                false
            }
            "--update-remote" => match value("--update-remote") {
                Some(remote) => {
                    update_remote = Some(remote);
                    true
                }
                None => return usage(),
            },
            "--update-ref" => match value("--update-ref") {
                Some(reference) => {
                    update_ref = reference;
                    true
                }
                None => return usage(),
            },
            "--bind" => match value("--bind") {
                Some(bind) => {
                    config.bind = bind;
                    true
                }
                None => return usage(),
            },
            "--port" => match value("--port").and_then(|port| port.parse().ok()) {
                Some(port) => {
                    config.port = port;
                    true
                }
                None => return usage(),
            },
            "--journal-dir" => match value("--journal-dir") {
                Some(dir) => {
                    config.journal_dir = dir.into();
                    true
                }
                None => return usage(),
            },
            "--otlp" => match value("--otlp") {
                Some(url) => {
                    config.otlp = Some(url);
                    true
                }
                None => return usage(),
            },
            other => {
                eprintln!("unknown argument {other}");
                return usage();
            }
        };
        index += if takes_value { 2 } else { 1 };
    }
    if automatic_update && std::env::var_os(RESTART_GUARD).is_none() {
        let Some(remote) = update_remote else {
            eprintln!("--auto-update requires --update-remote or PLAYBOOK_UPDATE_REMOTE");
            std::process::exit(2);
        };
        if let Err(error) = automatic_update_check(&remote, &update_ref, &args) {
            eprintln!("Theosis update check failed: {error}");
            std::process::exit(1);
        }
    }
    let bound = match playbook_server::bind(config) {
        Ok(bound) => bound,
        Err(error) => {
            eprintln!("bind failed: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = bound.run() {
        eprintln!("serve failed: {error}");
        std::process::exit(1);
    }
}

#[tracing::instrument]
fn automatic_update_check(remote: &str, reference: &str, args: &[String]) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let installed_before = server_version(&executable)?;
    let install_dir = executable
        .parent()
        .ok_or("server executable has no parent directory")?;
    let theosis = std::env::var_os("THEOSIS_BIN").unwrap_or_else(|| "theosis".into());
    let output = Command::new(theosis)
        .args(["update", "--remote", remote, "--ref", reference, "--binary"])
        .arg(&executable)
        .arg("--install-dir")
        .arg(install_dir)
        .arg("--json")
        .output()
        .map_err(|error| format!("could not start Theosis: {error}"))?;
    let report = String::from_utf8_lossy(&output.stdout);
    if output.status.code() == Some(10) || report.contains("\"decision\":\"LOCAL_NEWER\"") {
        return Ok(());
    }
    if !output.status.success() {
        return Err(format!(
            "Theosis exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let installed_after = server_version(&executable)?;
    if installed_after != installed_before {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let error = Command::new(executable)
                .args(args)
                .env(RESTART_GUARD, "1")
                .exec();
            return Err(format!("could not restart updated server: {error}"));
        }
        #[cfg(not(unix))]
        {
            let status = Command::new(executable)
                .args(args)
                .env(RESTART_GUARD, "1")
                .status()
                .map_err(|error| format!("could not restart updated server: {error}"))?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }
    Ok(())
}

#[tracing::instrument]
fn server_version(executable: &std::path::Path) -> Result<String, String> {
    let output = Command::new(executable)
        .arg("--version")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("server --version failed".into());
    }
    let output = String::from_utf8_lossy(&output.stdout);
    let version = output
        .split_whitespace()
        .find(|part| is_semver(part))
        .ok_or("server did not report a semantic version")?;
    Ok(version.to_string())
}

#[tracing::instrument]
fn is_semver(value: &str) -> bool {
    let mut components = value.split('.');
    components
        .by_ref()
        .all(|component| !component.is_empty() && component.bytes().all(|b| b.is_ascii_digit()))
        && components.count() == 0
        && value.split('.').count() == 3
}

#[tracing::instrument]
fn usage() {
    eprintln!(
        "usage: playbook-server [--bind ADDR] [--port PORT] [--journal-dir DIR] [--otlp URL] [--auto-update --update-remote URL [--update-ref REF]]"
    );
    std::process::exit(2);
}

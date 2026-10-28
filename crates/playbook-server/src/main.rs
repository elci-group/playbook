//! Entry point for the continuous drive service.

fn main() {
    let mut config = playbook_server::ServerConfig::default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        let value = |_flag: &str| -> Option<String> {
            let next = args.get(index + 1)?;
            Some(next.to_string())
        };
        match args[index].as_str() {
            "--bind" => match value("--bind") {
                Some(bind) => config.bind = bind,
                None => return usage(),
            },
            "--port" => match value("--port").and_then(|port| port.parse().ok()) {
                Some(port) => config.port = port,
                None => return usage(),
            },
            "--journal-dir" => match value("--journal-dir") {
                Some(dir) => config.journal_dir = dir.into(),
                None => return usage(),
            },
            "--otlp" => match value("--otlp") {
                Some(url) => config.otlp = Some(url),
                None => return usage(),
            },
            other => {
                eprintln!("unknown argument {other}");
                return usage();
            }
        }
        index += 2;
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

fn usage() {
    eprintln!(
        "usage: playbook-server [--bind ADDR] [--port PORT] [--journal-dir DIR] [--otlp URL]"
    );
    std::process::exit(2);
}

//! `value-lab` — the research CLI. Playbook reports; the lab decides.

use std::path::Path;

use value_lab::{
    classify, corpus_for_study, emit_ablations, emit_corpus, emit_economics, emit_failures,
    emit_replication, emit_report, emit_scoreboard, read_results, scoreboard, FrozenThresholds,
    Scoreboard,
};

const USAGE: &str = "value-lab — the independent research evaluator (Project VALUE)

usage:
  value-lab protocol                          print the frozen research protocol
  value-lab corpus [--out <file.jsonl>]       emit the study corpus (VALUE_CORPUS.jsonl)
  value-lab scoreboard --results <f.jsonl>    render the primary scoreboard
  value-lab verdict --results <f.jsonl>       classify VALUE-PROVEN / CONDITIONAL / NEUTRAL / NEGATIVE
  value-lab report --results <f.jsonl>        render VALUE_REPORT (the §38 statement, honestly filled)
  value-lab ablations --with <f> --without <f> --mechanism <name>
  value-lab failures --results <f.jsonl>      render VALUE_FAILURES
  value-lab economics --results <f.jsonl>     render VALUE_ECONOMICS
  value-lab replication                       print VALUE_REPLICATION
  value-lab help";

fn parse_flag(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn load_results(args: &[String]) -> Vec<value_lab::RunRecord> {
    let path = parse_flag(args, "--results").expect("--results requires a file path");
    read_results(Path::new(&path)).unwrap_or_else(|error| {
        eprintln!("cannot read results: {error}");
        std::process::exit(1);
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("protocol") => println!("{}", value_lab::emit_protocol()),
        Some("corpus") => {
            let text = match emit_corpus(&corpus_for_study()) {
                Ok(text) => text,
                Err(error) => {
                    eprintln!("corpus serialisation failed: {error}");
                    std::process::exit(1);
                }
            };
            match parse_flag(&args, "--out") {
                Some(path) => {
                    std::fs::write(&path, text)
                        .unwrap_or_else(|error| eprintln!("cannot write {path}: {error}"));
                    println!("wrote {path}");
                }
                None => print!("{text}"),
            }
        }
        Some("scoreboard") => {
            let runs = load_results(&args);
            let verdict = classify(&runs, &FrozenThresholds::registered());
            let board = Scoreboard {
                rows: scoreboard(&runs),
                verdict,
            };
            print!("{}", emit_scoreboard(&board));
        }
        Some("verdict") => {
            let runs = load_results(&args);
            let verdict = classify(&runs, &FrozenThresholds::registered());
            println!("{}", verdict.key());
        }
        Some("report") => {
            let runs = load_results(&args);
            let verdict = classify(&runs, &FrozenThresholds::registered());
            let board = Scoreboard {
                rows: scoreboard(&runs),
                verdict,
            };
            let protocol = value_lab::emit_protocol();
            let hash = playbook_core::sha256_hex(protocol.as_bytes());
            print!("{}", emit_report(&board, &hash));
        }
        Some("ablations") => {
            let with = parse_flag(&args, "--with").expect("--with requires a results file");
            let without =
                parse_flag(&args, "--without").expect("--without requires a results file");
            let mechanism = parse_flag(&args, "--mechanism").unwrap_or_else(|| "mechanism".into());
            let with_runs = match read_results(Path::new(&with)) {
                Ok(runs) => runs,
                Err(error) => {
                    eprintln!("cannot read {with}: {error}");
                    std::process::exit(1);
                }
            };
            let without_runs = match read_results(Path::new(&without)) {
                Ok(runs) => runs,
                Err(error) => {
                    eprintln!("cannot read {without}: {error}");
                    std::process::exit(1);
                }
            };
            let weights = value_lab::UtilityWeights::registered();
            let with_values: Vec<i64> = with_runs.iter().map(|run| run.utility(&weights)).collect();
            let without_values: Vec<i64> = without_runs
                .iter()
                .map(|run| run.utility(&weights))
                .collect();
            let rows = vec![value_lab::ablation_table(
                &with_values,
                &without_values,
                &mechanism,
            )];
            print!("{}", emit_ablations(&rows));
        }
        Some("failures") => print!("{}", emit_failures(&load_results(&args))),
        Some("economics") => print!("{}", emit_economics(&load_results(&args))),
        Some("replication") => {
            let protocol = value_lab::emit_protocol();
            let hash = playbook_core::sha256_hex(protocol.as_bytes());
            println!("{}", emit_replication(&hash));
        }
        Some("--help") | Some("-h") | Some("help") | None => println!("{USAGE}"),
        Some(other) => {
            eprintln!("unknown subcommand: {other}");
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

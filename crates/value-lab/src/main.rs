//! `value-lab` — the research CLI. Playbook reports; the lab decides.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("protocol") => {
            println!("{}", value_lab::emit_protocol());
        }
        Some("scoreboard") | Some("verdict") => {
            eprintln!(
                "scoreboard/verdict read VALUE_RESULTS.jsonl — not yet wired to a results file"
            );
            std::process::exit(2);
        }
        Some("--help") | Some("-h") | Some("help") | None => {
            println!("value-lab — the independent research evaluator (Project VALUE)\n");
            println!("usage: value-lab <protocol|scoreboard|verdict|help>");
            println!("  protocol   prints the frozen research protocol (hypotheses, thresholds)");
            println!("  scoreboard renders the primary scoreboard from VALUE_RESULTS.jsonl");
            println!("  verdict    classifies VALUE-PROVEN / CONDITIONAL / NEUTRAL / NEGATIVE");
        }
        Some(other) => {
            eprintln!("unknown subcommand: {other}");
            std::process::exit(2);
        }
    }
}

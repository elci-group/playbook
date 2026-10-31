//! `playbook-tui` — the Matchday surface. A projection of authoritative
//! Playbook state, never an interpretation of it.
//!
//! ```text
//! playbook-tui --replay journal.jsonl                # film room, step through
//! playbook-tui --replay journal.jsonl --frame 42     # one frame
//! playbook-tui --replay journal.jsonl --form broadcast
//! playbook-tui --live journal.jsonl                  # follow a live journal
//! ```
//!
//! Modes: Manager (control), Broadcast (observe), Film Room (understand).

use std::io::{BufRead, Write};
use std::path::Path;
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;

use playbook_policy::Policy;
use playbook_presentation::{frames, Frame};

use playbook_tui::{
    render_broadcast, render_heatmap, render_kick_decision, render_lastdown, render_manager,
    render_player_depth, render_three, FormKey, BROADCAST_WIDTH, MANAGER_WIDTH,
};

const USAGE: &str = "\
playbook-tui — the Matchday surface

usage:
  playbook-tui --replay <journal.jsonl> [--frame N] [--form F] [--width W]
  playbook-tui --live <journal.jsonl> [--interval-ms MS] [--form F] [--width W]
  playbook-tui --help

forms:
  manager      the dugout (default)
  broadcast    the live telemetry view
  glance       one line per frame
  tactical     the spatial block
  analytical   the forensic rows
  heat         the possession heat map
  basic        form3-style key=value lines (no box drawing)

film room keys (with --replay and no --frame):
  ENTER / n    next frame          p    previous frame
  f            next form           F    previous form
  k            kick decision       d    lastdown decision
  r            roster depth        h    heat map
  q            quit";

struct Options {
    replay: Option<String>,
    live: Option<String>,
    frame: Option<usize>,
    form: String,
    width: usize,
    interval_ms: u64,
    help: bool,
}

fn parse_options() -> Options {
    let mut options = Options {
        replay: None,
        live: None,
        frame: None,
        form: "manager".into(),
        width: BROADCAST_WIDTH,
        interval_ms: 1_000,
        help: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || -> Option<String> { args.next() };
        match arg.as_str() {
            "--help" | "-h" | "help" => options.help = true,
            "--replay" => options.replay = value(),
            "--live" => options.live = value(),
            "--frame" => {
                options.frame = value().and_then(|v| v.parse::<usize>().ok());
            }
            "--form" => {
                if let Some(form) = value() {
                    options.form = form;
                }
            }
            "--width" => {
                if let Some(width) = value().and_then(|v| v.parse::<usize>().ok()) {
                    options.width = width;
                }
            }
            "--interval-ms" => {
                if let Some(ms) = value().and_then(|v| v.parse::<u64>().ok()) {
                    options.interval_ms = ms;
                }
            }
            other => {
                eprintln!("unknown argument: {other}");
                exit(2);
            }
        }
    }
    options
}

fn load_film(path: &Path) -> Vec<Frame> {
    let recovery = playbook_journal::Journal::open(path).unwrap_or_else(|error| {
        eprintln!("cannot open journal {}: {error}", path.display());
        exit(1);
    });
    frames(&recovery.events, &Policy::builtin()).unwrap_or_else(|error| {
        eprintln!("journal does not fold cleanly: {error}");
        exit(1);
    })
}

fn draw(frame: &Frame, options: &Options) -> String {
    let snapshot = &frame.snapshot;
    match options.form.as_str() {
        "manager" => render_manager(snapshot, options.width.min(MANAGER_WIDTH.max(40))),
        "broadcast" => render_broadcast(snapshot, options.width),
        "glance" => frame.forms.glance.clone(),
        "tactical" => format!("{}\n{}", frame.forms.glance, frame.forms.tactical),
        "analytical" => format!(
            "{}\n{}",
            frame.forms.glance,
            frame
                .forms
                .analytical
                .iter()
                .map(|(label, value)| format!(
                    "{:<18} {}",
                    format!("{label}:").to_uppercase(),
                    value
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        "heat" => {
            let kinds: Vec<&str> = Vec::new();
            render_heatmap(snapshot, &kinds)
        }
        "basic" => format!(
            "frame={} kind={} glance=\"{}\"",
            frame.index, frame.kind, frame.forms.glance
        ),
        other => {
            eprintln!("unknown form: {other}");
            exit(2);
        }
    }
}

fn film_room(film: &[Frame], options: &Options) {
    if film.is_empty() {
        println!("the journal has no events");
        return;
    }
    if let Some(target) = options.frame {
        let frame = &film[target.saturating_sub(1).min(film.len() - 1)];
        println!("{}", draw(frame, options));
        return;
    }
    let mut cursor = 0usize;
    let mut form = match FormKey::parse(&options.form) {
        Some(form) => form,
        None => FormKey::Tactical,
    };
    let mut mode = options.form.clone();
    let stdin = std::io::stdin();
    loop {
        let frame = &film[cursor];
        if mode == "three" {
            print!(
                "{}",
                render_three(frame.index, frame.kind, &frame.forms, form)
            );
        } else {
            print!("{}", draw(frame, options));
        }
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        match line.trim() {
            "q" | "quit" => break,
            "p" | "b" => cursor = cursor.saturating_sub(1),
            "f" => {
                if mode == "three" {
                    form = form.next();
                } else {
                    mode = "three".into();
                    form = FormKey::Glance;
                }
            }
            "F" => mode = options.form.clone(),
            "k" => println!(
                "{}",
                render_kick_decision(&frame.snapshot.field, options.width)
            ),
            "d" => {
                if let Some(disposition) = frame.snapshot.scorebug.disposition {
                    println!("{}", render_lastdown(disposition, options.width));
                } else {
                    println!("no lastdown decision yet");
                }
            }
            "r" => println!("{}", render_player_depth(&frame.snapshot.roster)),
            "h" => println!("{}", render_heatmap(&frame.snapshot, &[frame.kind])),
            "g" => cursor = film.len() - 1,
            _ => cursor = (cursor + 1).min(film.len() - 1),
        }
    }
}

fn live(path: &Path, options: &Options) {
    let mut seen = 0usize;
    loop {
        let recovery = playbook_journal::Journal::open(path).unwrap_or_else(|error| {
            eprintln!("cannot open journal {}: {error}", path.display());
            exit(1);
        });
        let film = frames(&recovery.events, &Policy::builtin()).unwrap_or_else(|error| {
            eprintln!("journal does not fold cleanly: {error}");
            exit(1);
        });
        if film.len() > seen {
            seen = film.len();
            print!("\x1b[2J\x1b[H");
            if let Some(frame) = film.last() {
                println!("{}", draw(frame, options));
            }
            let _ = std::io::stdout().flush();
        }
        sleep(Duration::from_millis(options.interval_ms));
    }
}

fn main() {
    let options = parse_options();
    if options.help {
        println!("{USAGE}");
        return;
    }
    if let Some(replay) = options.replay.clone() {
        let film = load_film(Path::new(&replay));
        film_room(&film, &options);
    } else if let Some(live_path) = options.live.clone() {
        live(Path::new(&live_path), &options);
    } else {
        eprintln!("nothing to show: pass --replay or --live");
        exit(2);
    }
}

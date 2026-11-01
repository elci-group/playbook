//! `site-foreman` — run the model-authored, Playbook-governed site build.

use playbook_sim::{gate_one, open_drive, FixturePlane};
use site_foreman::{
    assemble, author_section, select_builders, AuthoredSection, Brief, InterventionLog,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let Some(key_file) = flag("--key-file") else {
        eprintln!("--key-file is required");
        std::process::exit(2);
    };
    let out_dir = flag("--out").unwrap_or_else(|| "site".into());
    let repo = flag("--workspace").unwrap_or_else(|| ".".into());
    let key = std::fs::read_to_string(&key_file)
        .map(|t| t.trim().to_string())
        .expect("key file");

    // The facts are extracted from the workspace; the builders only ever
    // see what the repo actually contains.
    let workspace = playbook_site::extract(std::path::Path::new(&repo));
    let draftboard: Vec<serde_json::Value> = serde_json::from_str(
        &std::fs::read_to_string(format!("{repo}/draftboard.json"))
            .expect("draftboard.json — run the live draftboard first"),
    )
    .expect("draftboard parses");
    let builders = select_builders(&draftboard, 2);
    println!("▌ SITE FOREMAN — builders by measured prior: {builders:?}");

    let mut log = InterventionLog::new();
    log.record(
        "operator",
        "chunked the crate grid to eight cards per response",
        "even at a 2000-token envelope, a reasoning model's hidden reasoning plus a long transcription overflowed — finish_reason 'length', validator caught the truncation",
        "work decomposition is the orchestrator's lever: the acceptance standard (every crate rendered) never moved; only the assignment size did",
    );
    log.record(
        "operator",
        "wrote the site-foreman orchestration crate itself",
        "Playbook's tool plane is fixture-backed; dispatching open-ended authoring to live models needed a harness that does not exist yet",
        "the single largest intervention of the whole exercise: the platform governed the work but could not yet BE the agent loop",
    );
    if std::path::Path::new("/tmp/interventions-run1.json").is_file() {
        log.record(
            "operator",
            "runs 1–2 rejected every section; two harness defects found and fixed",
            "run 1: markdown fences + literal heading match. run 2: max_tokens=200 starved the reasoning models — finish_reason was 'length' and the visible HTML was decapitated before the heading appeared",
            "the gates refused truncated work and the foreman never faked a pass — but the operator had to own the diagnosis both times. raising the envelope to 2000 tokens lowered no standard: the full fragment is now demanded and validated",
        );
    }

    // The briefs: extracted facts only.
    let facts = |list: &[playbook_site::CrateInfo]| {
        list.iter()
            .map(|c| format!("{} — {}", c.name, c.summary))
            .collect::<Vec<_>>()
            .join("\n")
    };
    // Eight cards per response: at 2000 tokens a reasoning model's hidden
    // reasoning plus a 19-card transcription overflows any envelope — the
    // validator caught the truncation every time. Small chunks are inside
    // every budget.
    let chunk_size = 8;
    let mut briefs: Vec<Brief> = Vec::new();
    briefs.push(Brief {
        id: "hero-what-is",
        instruction: "Author an HTML <section> introducing 'Playbook' for the site hero area: an <h2> 'What it is' plus two <p> paragraphs explaining that Playbook governs agentic workflows like a franchise governs a team: objectives declared with acceptance gates, work in bounded drives, no progress without independently validated evidence, a content-addressed replayable journal.".into(),
        must_contain: vec!["What it is".into(), "evidence".into()],
    });
    let chunks: Vec<&[playbook_site::CrateInfo]> = workspace.crates.chunks(chunk_size).collect();
    for (index, chunk) in chunks.iter().enumerate() {
        let heading = if index == 0 {
            "Author an HTML <section> with an <h2> 'The crates' and a <div class=\"grid\"> of <div class=\"card\"> items, one per crate: <b>NAME</b> then <p>SUMMARY</p>."
        } else {
            "Author an HTML <div class=\"grid\"> of <div class=\"card\"> items continuing a crate listing: one per crate, <b>NAME</b> then <p>SUMMARY</p>."
        };
        briefs.push(Brief {
            id: match index {
                0 => "crates-grid-1",
                _n => "crates-grid-more",
            },
            instruction: format!(
                "{heading} Include EVERY crate in this list, all {} of them, in order:\n{}",
                chunk.len(),
                facts(chunk)
            ),
            must_contain: chunk.iter().map(|c| c.name.clone()).collect(),
        });
    }
    briefs.push(Brief {
        id: "draftboard",
        instruction: format!(
            "Author an HTML <section> with an <h2> 'Live model draftboard' and a short <p> explaining these are free models ranked by measured completion of validated tasks, then a simple <table> with one row per model: model name, tasks completed, total, median latency in ms. Measured data:\n{}",
            serde_json::to_string_pretty(&draftboard).unwrap_or_default()
        ),
        must_contain: vec!["Live model draftboard".into()],
    });
    briefs.push(Brief {
        id: "contracts",
        instruction: format!(
            "Author an HTML <section> with an <h2> 'The contracts' and a <ul class=\"docs\"> of <li> items, one per contract document name (plain names): {}. Add a closing <p> saying every contract is grounded in the crates it describes.",
            workspace.docs.join(", ")
        ),
        must_contain: vec!["The contracts".into(), workspace.docs.first().cloned().unwrap_or_default()],
    });
    let mut sections: Vec<AuthoredSection> = Vec::new();
    for brief in &briefs {
        println!("[foreman] commissioning section: {}", brief.id);
        sections.push(author_section(&mut log, &key, &builders, brief));
    }

    // The skeleton is the foreman's, honestly recorded.
    let skeleton_head = std::fs::read_to_string("site/index.html")
        .ok()
        .map(|old| {
            let end = old.find("<header>").unwrap_or(0);
            format!("{}\n", &old[..end])
        })
        .unwrap_or_else(|| "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Playbook</title>\n<style>body{background:#0d1117;color:#e8e6e3;font:16px/1.6 sans-serif;max-width:980px;margin:0 auto;padding:24px}h1,h2{color:#39c5cf}a{color:#39c5cf}.card{background:#151b23;border:1px solid #232b36;border-radius:10px;padding:14px;margin:8px 0}.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(290px,1fr));gap:12px}.mono{font-family:monospace}</style>\n</head>\n<body>\n".into());
    log.record(
        "foreman",
        "assembled the page on an operator-authored CSS skeleton",
        "the builders authored content sections; the page chrome predates this run",
        "fair scope split: models own content, the skeleton is a prior commit's artifact",
    );

    let page = assemble(&skeleton_head, &sections, &log, &workspace.version);
    std::fs::write(format!("{out_dir}/index.html"), &page).expect("write index");
    let audit = serde_json::to_string_pretty(log.entries()).expect("audit serialises");
    std::fs::write(format!("{out_dir}/interventions.json"), &audit).expect("write audit");
    println!(
        "[foreman] {} sections assembled, {} interventions recorded",
        sections.len(),
        log.entries().len()
    );

    // The authority: gates over the assembled artifact, sealed real evidence.
    let (mut drive, _) = open_drive(
        FixturePlane::ready(),
        vec![
            gate_one(
                "sections_validated",
                playbook_core::EvidenceClass::TestReport,
            ),
            gate_one("content_complete", playbook_core::EvidenceClass::TestReport),
            gate_one(
                "assembly_deterministic",
                playbook_core::EvidenceClass::TestReport,
            ),
        ],
    )
    .unwrap();
    let all_valid = sections.iter().all(|s| s.provenance.starts_with("model"));
    let complete = {
        let index = std::fs::read_to_string(format!("{out_dir}/index.html")).unwrap();
        workspace.crates.iter().all(|c| index.contains(&c.name))
    };
    let deterministic = {
        let again = assemble(&skeleton_head, &sections, &log, &workspace.version);
        again == page
    };
    let seal = |gate_id: &str, passed: u64, failed: u64, at: u64| {
        playbook_core::Evidence::seal(
            playbook_core::EvidenceClass::TestReport,
            serde_json::json!({
                "command": "site-foreman build",
                "repository_hash": "playbook-workspace",
                "test_suite": gate_id,
                "exit_code": if failed == 0 { 0 } else { 1 },
                "passed": passed, "failed": failed, "ignored": 0,
                "timestamp": at.to_string(),
                "gate_id": gate_id
            }),
            at,
        )
        .unwrap()
    };
    drive
        .accept_evidence(seal(
            "sections_validated",
            u64::from(all_valid),
            u64::from(!all_valid),
            1,
        ))
        .unwrap();
    drive
        .accept_evidence(seal(
            "content_complete",
            u64::from(complete),
            u64::from(!complete),
            2,
        ))
        .unwrap();
    drive
        .accept_evidence(seal(
            "assembly_deterministic",
            u64::from(deterministic),
            u64::from(!deterministic),
            3,
        ))
        .unwrap();
    let completed = drive.claim_complete(4).unwrap();
    println!(
        "[authority] gates satisfied, completion claimed: {completed} (all-model content: {all_valid}, content complete: {complete}, deterministic: {deterministic})"
    );
    let replayed = playbook_sim::fold(
        drive.log(),
        &playbook_sim::Policy::builtin(),
        playbook_sim::ApplyMode::Resimulate,
    )
    .unwrap();
    assert_eq!(&replayed, drive.state(), "the governed build replays");
    println!("[authority] journal replay: identical state — the record stands");
}

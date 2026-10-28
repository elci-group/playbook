use playbook_journal::Journal;
use playbook_sim::{roster, run_mvp_with_journal, Authority, DriveEvent, Policy};

#[test]
fn a_crashed_invocation_recovers_from_disk_and_reconciles() {
    let full_path =
        std::env::temp_dir().join(format!("playbook-crash-full-{}.jsonl", std::process::id()));
    let crash_path = std::env::temp_dir().join(format!(
        "playbook-crash-prefix-{}.jsonl",
        std::process::id()
    ));
    let report = run_mvp_with_journal(&full_path).unwrap();
    assert_eq!(report.journal_replay_match, Some(true));

    let recovery = Journal::open(&full_path).unwrap();
    let invoke_end = recovery
        .events
        .iter()
        .position(|event| matches!(event, DriveEvent::ToolInvoked { .. }))
        .unwrap()
        + 1;

    let mut crashed = Journal::create(&crash_path).unwrap();
    for event in &recovery.events[..invoke_end] {
        crashed.record(event).unwrap();
    }
    drop(crashed);

    let opened = Journal::open(&crash_path).unwrap();
    assert_eq!(opened.events.len(), invoke_end);
    assert!(!opened.repaired_tail);
    assert!(matches!(
        opened.events.last(),
        Some(DriveEvent::ToolInvoked { .. })
    ));

    let mut resumed = Authority::resume(opened.events, Policy::builtin(), roster()).unwrap();
    assert!(resumed.state().open_execution().is_some());
    assert!(resumed.state().unresolved_effects().is_empty());
    let key = resumed
        .state()
        .open_execution()
        .unwrap()
        .idempotency_key
        .clone()
        .unwrap();
    resumed.reconcile(&key, false).unwrap();
    assert!(resumed.state().open_execution().is_none());

    std::fs::remove_file(&full_path).unwrap();
    std::fs::remove_file(&crash_path).unwrap();
}

#[test]
fn a_torn_crash_tail_is_reported_and_repaired() {
    let full_path =
        std::env::temp_dir().join(format!("playbook-torn-full-{}.jsonl", std::process::id()));
    let torn_path =
        std::env::temp_dir().join(format!("playbook-torn-crash-{}.jsonl", std::process::id()));
    let report = run_mvp_with_journal(&full_path).unwrap();
    assert_eq!(report.journal_replay_match, Some(true));

    let recovery = Journal::open(&full_path).unwrap();
    let invoke_end = recovery
        .events
        .iter()
        .position(|event| matches!(event, DriveEvent::ToolInvoked { .. }))
        .unwrap()
        + 1;
    let prefix: Vec<DriveEvent> = recovery.events[..invoke_end].to_vec();

    let mut crashed = Journal::create(&torn_path).unwrap();
    for event in &prefix {
        crashed.record(event).unwrap();
    }
    drop(crashed);

    let intact = std::fs::read(&torn_path).unwrap();
    let mut torn = std::fs::OpenOptions::new()
        .append(true)
        .open(&torn_path)
        .unwrap();
    use std::io::Write as _;
    torn.write_all(format!("{{\"seq\":{},\"digest\":\"ab", prefix.len() + 1).as_bytes())
        .unwrap();
    drop(torn);

    let opened = Journal::open(&torn_path).unwrap();
    assert!(opened.repaired_tail);
    assert_eq!(opened.events, prefix);
    assert_eq!(std::fs::read(&torn_path).unwrap(), intact);

    std::fs::remove_file(&full_path).unwrap();
    std::fs::remove_file(&torn_path).unwrap();
}

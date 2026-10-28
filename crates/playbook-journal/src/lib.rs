//! Durable write-ahead journal for drive events.
//!
//! Each record is one newline-terminated JSON line
//! `{"seq":N,"digest":"<sha256>","event":{...}}` where the digest covers the
//! canonical serialization of the event and `seq` starts at 1 and increases by
//! exactly one per record. Recovery keeps the longest valid prefix and
//! truncates everything after the first short, unparseable, out-of-sequence,
//! or digest-mismatched record.

#![forbid(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use playbook_core::{sha256_hex, to_canonical_bytes, PlaybookError};
use playbook_events::DriveEvent;
use playbook_state::EventSink;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct JournalRecord {
    seq: u64,
    digest: String,
    event: DriveEvent,
}

pub struct Journal {
    file: File,
    offsets: Vec<u64>,
}

/// The result of opening an existing journal.
pub struct Recovery {
    pub journal: Journal,
    pub events: Vec<DriveEvent>,
    pub repaired_tail: bool,
}

fn io_error(err: std::io::Error) -> PlaybookError {
    PlaybookError::Io(err.to_string())
}

fn digest_of(event: &DriveEvent) -> Result<String, PlaybookError> {
    Ok(sha256_hex(&to_canonical_bytes(event)?))
}

fn parse_record(line: &[u8], expected_seq: u64) -> Result<Option<JournalRecord>, PlaybookError> {
    let record: JournalRecord = match serde_json::from_slice(line) {
        Ok(record) => record,
        Err(_) => return Ok(None),
    };
    if record.seq != expected_seq {
        return Ok(None);
    }
    if record.digest != digest_of(&record.event)? {
        return Ok(None);
    }
    Ok(Some(record))
}

impl Journal {
    pub fn create(path: &Path) -> Result<Self, PlaybookError> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        Ok(Self {
            file,
            offsets: Vec::new(),
        })
    }

    pub fn open(path: &Path) -> Result<Recovery, PlaybookError> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(io_error)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(io_error)?;

        let mut offsets: Vec<u64> = Vec::new();
        let mut events: Vec<DriveEvent> = Vec::new();
        let mut start = 0usize;
        while let Some(rel) = bytes[start..].iter().position(|byte| *byte == b'\n') {
            let line = &bytes[start..start + rel];
            let record = match parse_record(line, (offsets.len() + 1) as u64)? {
                Some(record) => record,
                None => break,
            };
            offsets.push((start + rel + 1) as u64);
            events.push(record.event);
            start += rel + 1;
        }

        let repaired_tail = start != bytes.len();
        if repaired_tail {
            file.set_len(start as u64).map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
        }
        file.seek(SeekFrom::Start(start as u64)).map_err(io_error)?;
        Ok(Recovery {
            journal: Self { file, offsets },
            events,
            repaired_tail,
        })
    }

    pub fn record(&mut self, event: &DriveEvent) -> Result<(), PlaybookError> {
        let record = JournalRecord {
            seq: (self.offsets.len() + 1) as u64,
            digest: digest_of(event)?,
            event: event.clone(),
        };
        let mut line = serde_json::to_vec(&record).map_err(PlaybookError::json)?;
        line.push(b'\n');
        self.file.write_all(&line).map_err(io_error)?;
        self.file.sync_all().map_err(io_error)?;
        let end = self.offsets.last().copied().unwrap_or(0) + line.len() as u64;
        self.offsets.push(end);
        Ok(())
    }
}

impl EventSink for Journal {
    fn record(&mut self, event: &DriveEvent) -> Result<(), PlaybookError> {
        Journal::record(self, event)
    }

    fn mark(&self) -> u64 {
        self.offsets.len() as u64
    }

    fn rollback(&mut self, mark: u64) -> Result<(), PlaybookError> {
        if mark as usize > self.offsets.len() {
            return Err(PlaybookError::ReplayDivergence(
                "journal mark out of range".into(),
            ));
        }
        let end = if mark == 0 {
            0
        } else {
            self.offsets[(mark - 1) as usize]
        };
        self.file.set_len(end).map_err(io_error)?;
        self.file.sync_all().map_err(io_error)?;
        self.file.seek(SeekFrom::Start(end)).map_err(io_error)?;
        self.offsets.truncate(mark as usize);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use playbook_core::{
        DriveId, EvidenceClass, Gate, GateId, PlaybookError, TaskContract, TaskIntegrity,
    };
    use playbook_events::DriveEvent;
    use playbook_policy::Policy;
    use playbook_state::{fold, started_budget, ApplyMode, Authority, EventSink};

    use super::{digest_of, Journal, JournalRecord};

    fn event_log() -> Vec<DriveEvent> {
        let mut authority = Authority::start(
            DriveId::new("journal-drive").unwrap(),
            TaskContract {
                statement: "compile the implementation".into(),
                workload_class: "engineering".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            started_budget(1_000, 1_000),
            Policy::builtin(),
            Vec::new(),
            1,
        )
        .unwrap();
        authority
            .declare_gate(
                Gate::declare(
                    GateId::new("implementation").unwrap(),
                    "compiler accepts the implementation",
                    1_000_000,
                    Vec::new(),
                    vec![EvidenceClass::CompilerResult],
                    true,
                )
                .unwrap(),
            )
            .unwrap();
        authority.log().to_vec()
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "playbook-journal-{}-{name}.jsonl",
            std::process::id()
        ))
    }

    fn journal_line(event: &DriveEvent, seq: u64) -> Vec<u8> {
        let record = JournalRecord {
            seq,
            digest: digest_of(event).unwrap(),
            event: event.clone(),
        };
        let mut line = serde_json::to_vec(&record).unwrap();
        line.push(b'\n');
        line
    }

    #[test]
    fn reopen_reproduces_the_recorded_events() {
        let events = event_log();
        let path = temp_path("reopen");
        let mut sink: Box<dyn EventSink> = Box::new(Journal::create(&path).unwrap());
        for event in &events {
            sink.record(event).unwrap();
        }
        drop(sink);

        let recovery = Journal::open(&path).unwrap();
        assert_eq!(recovery.events, events);
        assert!(!recovery.repaired_tail);
        assert_eq!(recovery.journal.mark(), events.len() as u64);
        assert_eq!(
            fold(&recovery.events, &Policy::builtin(), ApplyMode::Audit).unwrap(),
            fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap()
        );
    }

    #[test]
    fn torn_tail_is_truncated_on_open() {
        let events = event_log();
        let path = temp_path("torn");
        let mut sink: Box<dyn EventSink> = Box::new(Journal::create(&path).unwrap());
        for event in events.iter().take(3) {
            sink.record(event).unwrap();
        }
        drop(sink);

        let expected = fs::read(&path).unwrap();
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"seq\":4,\"digest\":\"abc").unwrap();
        drop(file);

        let recovery = Journal::open(&path).unwrap();
        assert_eq!(recovery.events, &events[..3]);
        assert!(recovery.repaired_tail);
        assert_eq!(fs::read(&path).unwrap(), expected);
        drop(recovery.journal);

        let reopened = Journal::open(&path).unwrap();
        assert_eq!(reopened.events, &events[..3]);
        assert!(!reopened.repaired_tail);
    }

    #[test]
    fn tampered_digest_truncates_at_that_record() {
        let events = event_log();
        let path = temp_path("tampered");
        let mut bytes = journal_line(&events[0], 1);
        let tampered = JournalRecord {
            seq: 2,
            digest: "0".repeat(64),
            event: events[1].clone(),
        };
        bytes.extend_from_slice(&serde_json::to_vec(&tampered).unwrap());
        bytes.push(b'\n');
        fs::write(&path, &bytes).unwrap();

        let recovery = Journal::open(&path).unwrap();
        assert_eq!(recovery.events, vec![events[0].clone()]);
        assert!(recovery.repaired_tail);
        assert_eq!(fs::read(&path).unwrap(), journal_line(&events[0], 1));
    }

    #[test]
    fn sequence_gap_truncates_at_that_record() {
        let events = event_log();
        let path = temp_path("seq-gap");
        let mut bytes = journal_line(&events[0], 1);
        bytes.extend_from_slice(&journal_line(&events[1], 3));
        fs::write(&path, &bytes).unwrap();

        let recovery = Journal::open(&path).unwrap();
        assert_eq!(recovery.events, vec![events[0].clone()]);
        assert!(recovery.repaired_tail);
        assert_eq!(fs::read(&path).unwrap(), journal_line(&events[0], 1));
    }

    #[test]
    fn rollback_drops_records_after_the_mark() {
        let events = event_log();
        let path = temp_path("rollback");
        let mut sink: Box<dyn EventSink> = Box::new(Journal::create(&path).unwrap());
        for event in events.iter().take(3) {
            sink.record(event).unwrap();
        }
        let mark = sink.mark();
        for event in events.iter().take(5).skip(3) {
            sink.record(event).unwrap();
        }
        sink.rollback(mark).unwrap();
        drop(sink);

        let mut recovery = Journal::open(&path).unwrap();
        assert_eq!(recovery.events, &events[..3]);
        recovery.journal.rollback(0).unwrap();
        drop(recovery.journal);

        let recovery_empty = Journal::open(&path).unwrap();
        assert!(recovery_empty.events.is_empty());
    }

    #[test]
    fn rollback_rejects_unknown_marks() {
        let events = event_log();
        let path = temp_path("unknown-mark");
        let mut sink: Box<dyn EventSink> = Box::new(Journal::create(&path).unwrap());
        sink.record(&events[0]).unwrap();
        assert_eq!(
            sink.rollback(sink.mark() + 1).unwrap_err(),
            PlaybookError::ReplayDivergence("journal mark out of range".into())
        );
    }
}

//! Persisted call history: each answered call's transcript is appended to
//! `~/.copilot/voice-call/history.jsonl` (one JSON record per line) so the user
//! can review past conversations. The file is capped to the most recent
//! [`MAX_RECORDS`] calls.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// History file location relative to the home directory.
const HISTORY_REL: &str = ".copilot/voice-call/history.jsonl";
/// Keep at most this many recent calls.
const MAX_RECORDS: usize = 200;

/// One line of a call transcript.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptEntry {
    /// "agent" or "you".
    pub who: String,
    pub text: String,
}

/// A single completed call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallRecord {
    /// Epoch milliseconds when the call was answered.
    pub started_at: i64,
    /// Epoch milliseconds when the call ended.
    pub ended_at: i64,
    /// Why the agent called (if provided).
    pub reason: Option<String>,
    /// How the call ended (e.g. "You ended the call.").
    pub outcome: Option<String>,
    pub entries: Vec<TranscriptEntry>,
}

/// Absolute path to the history file.
fn history_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
    Ok(home.join(HISTORY_REL))
}

/// Parse a JSONL blob into records, skipping blank or malformed lines.
fn parse_records(raw: &str) -> Vec<CallRecord> {
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<CallRecord>(l).ok())
        .collect()
}

/// Read all records from `path` (empty when the file does not exist).
fn read_all(path: &Path) -> Result<Vec<CallRecord>> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(parse_records(&raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Serialize records back to JSONL (one compact object per line).
fn to_jsonl(records: &[CallRecord]) -> Result<String> {
    let mut out = String::new();
    for r in records {
        out.push_str(&serde_json::to_string(r)?);
        out.push('\n');
    }
    Ok(out)
}

/// Append a record and trim the file to the most recent [`MAX_RECORDS`].
fn append_record(path: &Path, record: &CallRecord) -> Result<()> {
    let mut records = read_all(path)?;
    records.push(record.clone());
    if records.len() > MAX_RECORDS {
        let start = records.len() - MAX_RECORDS;
        records.drain(0..start);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, to_jsonl(&records)?).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Save one completed call to history.
pub fn save(record: CallRecord) -> Result<()> {
    append_record(&history_path()?, &record)
}

/// List recent calls, newest first, limited to `limit` (or all if `None`).
pub fn list(limit: Option<usize>) -> Result<Vec<CallRecord>> {
    let mut records = read_all(&history_path()?)?;
    records.reverse();
    if let Some(n) = limit {
        records.truncate(n);
    }
    Ok(records)
}

/// Delete all saved history.
pub fn clear() -> Result<()> {
    let path = history_path()?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("removing {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(started: i64, text: &str) -> CallRecord {
        CallRecord {
            started_at: started,
            ended_at: started + 1000,
            reason: Some("test".into()),
            outcome: Some("You ended the call.".into()),
            entries: vec![TranscriptEntry {
                who: "agent".into(),
                text: text.into(),
            }],
        }
    }

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "voicehist-{}-{}-{}.jsonl",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn read_missing_is_empty() {
        let p = temp_path("missing");
        assert!(read_all(&p).unwrap().is_empty());
    }

    #[test]
    fn append_then_read_roundtrip() {
        let p = temp_path("roundtrip");
        append_record(&p, &rec(1, "one")).unwrap();
        append_record(&p, &rec(2, "two")).unwrap();
        let all = read_all(&p).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].started_at, 1);
        assert_eq!(all[1].started_at, 2);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn parse_skips_malformed_lines() {
        let raw = format!(
            "{}\nnot json\n\n{}\n",
            serde_json::to_string(&rec(1, "a")).unwrap(),
            serde_json::to_string(&rec(2, "b")).unwrap(),
        );
        let recs = parse_records(&raw);
        assert_eq!(recs.len(), 2);
    }

    #[test]
    fn append_enforces_cap() {
        let p = temp_path("cap");
        for i in 0..(MAX_RECORDS as i64 + 5) {
            append_record(&p, &rec(i, "x")).unwrap();
        }
        let all = read_all(&p).unwrap();
        assert_eq!(all.len(), MAX_RECORDS);
        // Oldest 5 dropped; the first remaining record is #5.
        assert_eq!(all[0].started_at, 5);
        let _ = fs::remove_file(&p);
    }
}

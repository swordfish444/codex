use std::cmp::Reverse;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use codex_file_search as file_search;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::RolloutItem;
use codex_protocol::protocol::RolloutLine;
use serde_json::Value;
use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use time::OffsetDateTime;
use time::PrimitiveDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;
use tokio::fs;
use tokio::io::AsyncBufReadExt;
use uuid::Uuid;

use super::SESSIONS_SUBDIR;

#[derive(Debug, Default, PartialEq)]
pub struct ConversationsPage {
    pub items: Vec<ConversationItem>,
    pub next_cursor: Option<Cursor>,
    pub num_scanned_files: usize,
    pub reached_scan_cap: bool,
}

#[derive(Debug, PartialEq)]
pub struct ConversationItem {
    pub path: PathBuf,
    pub head: Vec<Value>,
}

const MAX_SCAN_FILES: usize = 100;
const HEAD_RECORD_LIMIT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    ts: OffsetDateTime,
    id: Uuid,
}

impl Cursor {
    fn new(ts: OffsetDateTime, id: Uuid) -> Self {
        Self { ts, id }
    }
}

impl serde::Serialize for Cursor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let ts_str = self
            .ts
            .format(&format_description!(
                "[year]-[month]-[day]T[hour]-[minute]-[second]"
            ))
            .map_err(|e| serde::ser::Error::custom(format!("format error: {e}")))?;
        serializer.serialize_str(&format!("{ts_str}|{}", self.id))
    }
}

impl<'de> serde::Deserialize<'de> for Cursor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_cursor(&s).ok_or_else(|| serde::de::Error::custom("invalid cursor"))
    }
}

pub async fn get_conversations(
    codex_home: &Path,
    page_size: usize,
    cursor: Option<&Cursor>,
) -> io::Result<ConversationsPage> {
    let mut root = codex_home.to_path_buf();
    root.push(SESSIONS_SUBDIR);

    if !root.exists() {
        return Ok(ConversationsPage::default());
    }

    let anchor = cursor.cloned();

    traverse_directories_for_paths(root, page_size, anchor).await
}

pub async fn get_conversation(path: &Path) -> io::Result<String> {
    fs::read_to_string(path).await
}

pub async fn find_conversation_path_by_id_str(
    codex_home: &Path,
    id_str: &str,
) -> io::Result<Option<PathBuf>> {
    if Uuid::parse_str(id_str).is_err() {
        return Ok(None);
    }

    let mut root = codex_home.to_path_buf();
    root.push(SESSIONS_SUBDIR);
    if !root.exists() {
        return Ok(None);
    }

    let limit = NonZero::new(1).unwrap();
    let threads = NonZero::new(2).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let exclude: Vec<String> = Vec::new();
    let compute_indices = false;

    let results = file_search::run(
        id_str,
        limit,
        &root,
        exclude,
        threads,
        cancel,
        compute_indices,
    )
    .map_err(|e| io::Error::other(format!("file search failed: {e}")))?;

    Ok(results
        .matches
        .into_iter()
        .next()
        .map(|m| root.join(m.path)))
}

async fn traverse_directories_for_paths(
    root: PathBuf,
    page_size: usize,
    anchor: Option<Cursor>,
) -> io::Result<ConversationsPage> {
    let mut items: Vec<ConversationItem> = Vec::with_capacity(page_size);
    let mut scanned_files = 0usize;
    let mut anchor_passed = anchor.is_none();
    let (anchor_ts, anchor_id) = match anchor {
        Some(c) => (c.ts, c.id),
        None => (OffsetDateTime::UNIX_EPOCH, Uuid::nil()),
    };

    let year_dirs = collect_dirs_desc(&root, |s| s.parse::<u16>().ok()).await?;

    'outer: for (_year, year_path) in year_dirs.iter() {
        if scanned_files >= MAX_SCAN_FILES {
            break;
        }
        let month_dirs = collect_dirs_desc(year_path, |s| s.parse::<u8>().ok()).await?;
        for (_month, month_path) in month_dirs.iter() {
            if scanned_files >= MAX_SCAN_FILES {
                break 'outer;
            }
            let day_dirs = collect_dirs_desc(month_path, |s| s.parse::<u8>().ok()).await?;
            for (_day, day_path) in day_dirs.iter() {
                if scanned_files >= MAX_SCAN_FILES {
                    break 'outer;
                }
                let mut day_files = collect_files(day_path, |name_str, path| {
                    if !name_str.starts_with("rollout-") || !name_str.ends_with(".jsonl") {
                        return None;
                    }

                    parse_timestamp_uuid_from_filename(name_str)
                        .map(|(ts, id)| (ts, id, name_str.to_string(), path.to_path_buf()))
                })
                .await?;
                day_files.sort_by_key(|(ts, sid, _, _)| (Reverse(*ts), Reverse(*sid)));
                for (ts, sid, _name_str, path) in day_files.into_iter() {
                    scanned_files += 1;
                    if scanned_files >= MAX_SCAN_FILES && items.len() >= page_size {
                        break 'outer;
                    }
                    if !anchor_passed {
                        if ts < anchor_ts || (ts == anchor_ts && sid < anchor_id) {
                            anchor_passed = true;
                        } else {
                            continue;
                        }
                    }
                    if items.len() == page_size {
                        break 'outer;
                    }
                    let (head, saw_session_meta, saw_user_event) =
                        read_head_and_flags(&path, HEAD_RECORD_LIMIT)
                            .await
                            .unwrap_or((Vec::new(), false, false));
                    if saw_session_meta && saw_user_event {
                        items.push(ConversationItem { path, head });
                    }
                }
            }
        }
    }

    let next = build_next_cursor(&items);
    Ok(ConversationsPage {
        items,
        next_cursor: next,
        num_scanned_files: scanned_files,
        reached_scan_cap: scanned_files >= MAX_SCAN_FILES,
    })
}

fn build_next_cursor(items: &[ConversationItem]) -> Option<Cursor> {
    let last = items.last()?;
    let file_name = last.path.file_name()?.to_string_lossy();
    let (ts, id) = parse_timestamp_uuid_from_filename(&file_name)?;
    Some(Cursor::new(ts, id))
}

async fn collect_dirs_desc<T, F>(parent: &Path, parse: F) -> io::Result<Vec<(T, PathBuf)>>
where
    T: Ord + Copy,
    F: Fn(&str) -> Option<T>,
{
    let mut dir = fs::read_dir(parent).await?;
    let mut vec: Vec<(T, PathBuf)> = Vec::new();
    while let Some(entry) = dir.next_entry().await? {
        if entry
            .file_type()
            .await
            .map(|ft| ft.is_dir())
            .unwrap_or(false)
            && let Some(s) = entry.file_name().to_str()
            && let Some(v) = parse(s)
        {
            vec.push((v, entry.path()));
        }
    }
    vec.sort_by_key(|(v, _)| Reverse(*v));
    Ok(vec)
}

async fn collect_files<T, F>(parent: &Path, parse: F) -> io::Result<Vec<T>>
where
    F: Fn(&str, &Path) -> Option<T>,
{
    let mut dir = fs::read_dir(parent).await?;
    let mut collected: Vec<T> = Vec::new();
    while let Some(entry) = dir.next_entry().await? {
        if entry
            .file_type()
            .await
            .map(|ft| ft.is_file())
            .unwrap_or(false)
            && let Some(s) = entry.file_name().to_str()
            && let Some(v) = parse(s, &entry.path())
        {
            collected.push(v);
        }
    }
    Ok(collected)
}

fn parse_timestamp_uuid_from_filename(name: &str) -> Option<(OffsetDateTime, Uuid)> {
    let core = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    let (sep_idx, uuid) = core
        .match_indices('-')
        .rev()
        .find_map(|(i, _)| Uuid::parse_str(&core[i + 1..]).ok().map(|u| (i, u)))?;
    let ts_str = &core[..sep_idx];
    let format: &[FormatItem] =
        format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    let ts = PrimitiveDateTime::parse(ts_str, format).ok()?.assume_utc();
    Some((ts, uuid))
}

fn parse_cursor(token: &str) -> Option<Cursor> {
    let (file_ts, uuid_str) = token.split_once('|')?;
    let uuid = Uuid::parse_str(uuid_str).ok()?;
    let format: &[FormatItem] =
        format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    let ts = PrimitiveDateTime::parse(file_ts, format).ok()?.assume_utc();
    Some(Cursor::new(ts, uuid))
}

async fn read_head_and_flags(
    path: &Path,
    max_records: usize,
) -> io::Result<(Vec<Value>, bool, bool)> {
    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = reader.lines();
    let mut head: Vec<Value> = Vec::new();
    let mut saw_session_meta = false;
    let mut saw_user_event = false;

    while head.len() < max_records {
        let line_opt = lines.next_line().await?;
        let Some(line) = line_opt else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed: Result<RolloutLine, _> = serde_json::from_str(trimmed);
        let Ok(rollout_line) = parsed else { continue };

        match rollout_line.item {
            RolloutItem::SessionMeta(session_meta_line) => {
                if let Ok(val) = serde_json::to_value(session_meta_line) {
                    head.push(val);
                    saw_session_meta = true;
                }
            }
            RolloutItem::ResponseItem(item) => {
                if let Ok(val) = serde_json::to_value(item) {
                    head.push(val);
                }
            }
            RolloutItem::TurnContext(_) | RolloutItem::Compacted(_) => {}
            RolloutItem::EventMsg(ev) => {
                if matches!(ev, EventMsg::UserMessage(_)) {
                    saw_user_event = true;
                }
            }
        }
    }

    Ok((head, saw_session_meta, saw_user_event))
}

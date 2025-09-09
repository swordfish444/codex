use codex_protocol::mcp_protocol::ConversationId;
use time::OffsetDateTime;
use time::PrimitiveDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;
use uuid::Uuid;

/// Timestamp format used in rollout filenames: YYYY-MM-DDThh-mm-ss
pub const FILENAME_TS_FMT: &[FormatItem] =
    format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");

/// Timestamp format used for JSONL records (UTC, second precision): YYYY-MM-DDThh:mm:ssZ
pub const RECORD_TS_FMT: &[FormatItem] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");

/// Pagination cursor identifying a file by timestamp and UUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub(crate) ts: OffsetDateTime,
    pub(crate) id: Uuid,
}

impl Cursor {
    pub fn new(ts: OffsetDateTime, id: Uuid) -> Self {
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
            .format(FILENAME_TS_FMT)
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

/// Parses a pagination cursor token in the form "<file_ts>|<uuid>".
pub fn parse_cursor(token: &str) -> Option<Cursor> {
    let (file_ts, uuid_str) = token.split_once('|')?;

    let Ok(uuid) = Uuid::parse_str(uuid_str) else {
        return None;
    };

    let ts = PrimitiveDateTime::parse(file_ts, FILENAME_TS_FMT)
        .ok()?
        .assume_utc();

    Some(Cursor::new(ts, uuid))
}

/// Parse timestamp and UUID from a rollout filename.
/// Expected format: rollout-YYYY-MM-DDThh-mm-ss-<uuid>.jsonl
pub fn parse_timestamp_uuid_from_filename(name: &str) -> Option<(OffsetDateTime, Uuid)> {
    let core = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;

    // Scan from the right for a '-' such that the suffix parses as a UUID.
    let (sep_idx, uuid) = core
        .match_indices('-')
        .rev()
        .find_map(|(i, _)| Uuid::parse_str(&core[i + 1..]).ok().map(|u| (i, u)))?;

    let ts_str = &core[..sep_idx];
    let ts = PrimitiveDateTime::parse(ts_str, FILENAME_TS_FMT)
        .ok()?
        .assume_utc();
    Some((ts, uuid))
}

/// Build a rollout filename for a given timestamp and conversation id.
pub fn build_rollout_filename(ts: OffsetDateTime, conversation_id: ConversationId) -> String {
    let date_str = ts
        .format(FILENAME_TS_FMT)
        .unwrap_or_else(|e| panic!("failed to format timestamp: {e}"));
    format!("rollout-{date_str}-{conversation_id}.jsonl")
}

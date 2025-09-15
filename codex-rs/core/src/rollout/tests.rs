#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs::File;
use std::fs::{self};
use std::io::Write;
use std::path::Path;

use tempfile::TempDir;
use time::OffsetDateTime;
use time::PrimitiveDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;
use uuid::Uuid;

use crate::rollout::list::ConversationItem;
use crate::rollout::list::ConversationsPage;
use crate::rollout::list::Cursor;
use crate::rollout::list::get_conversation;
use crate::rollout::list::get_conversations;
use crate::rollout::recorder::RolloutRecorder;

fn write_session_file(
    root: &Path,
    ts_str: &str,
    uuid: Uuid,
    num_records: usize,
    cwd: &Path,
) -> std::io::Result<(OffsetDateTime, Uuid)> {
    let format: &[FormatItem] =
        format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    let dt = PrimitiveDateTime::parse(ts_str, format)
        .unwrap()
        .assume_utc();
    let dir = root
        .join("sessions")
        .join(format!("{:04}", dt.year()))
        .join(format!("{:02}", u8::from(dt.month())))
        .join(format!("{:02}", dt.day()));
    fs::create_dir_all(&dir)?;

    let filename = format!("rollout-{ts_str}-{uuid}.jsonl");
    let file_path = dir.join(filename);
    let mut file = File::create(file_path)?;

    let cwd_str = cwd.to_string_lossy();
    let meta = serde_json::json!({
        "timestamp": ts_str,
        "type": "session_meta",
        "payload": {
            "id": uuid,
            "timestamp": ts_str,
            "instructions": null,
            "cwd": cwd_str,
            "originator": "test_originator",
            "cli_version": "test_version"
        }
    });
    writeln!(file, "{meta}")?;

    // Include at least one user message event to satisfy listing filters
    let user_event = serde_json::json!({
        "timestamp": ts_str,
        "type": "event_msg",
        "payload": {
            "type": "user_message",
            "message": "Hello from user",
            "kind": "plain"
        }
    });
    writeln!(file, "{user_event}")?;

    for i in 0..num_records {
        let rec = serde_json::json!({
            "record_type": "response",
            "index": i
        });
        writeln!(file, "{rec}")?;
    }
    Ok((dt, uuid))
}

#[tokio::test]
async fn test_list_conversations_latest_first() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    // Fixed UUIDs for deterministic expectations
    let u1 = Uuid::from_u128(1);
    let u2 = Uuid::from_u128(2);
    let u3 = Uuid::from_u128(3);

    let cwd = Path::new(".");
    // Create three sessions across three days
    write_session_file(home, "2025-01-01T12-00-00", u1, 3, cwd).unwrap();
    write_session_file(home, "2025-01-02T12-00-00", u2, 3, cwd).unwrap();
    write_session_file(home, "2025-01-03T12-00-00", u3, 3, cwd).unwrap();

    let page = get_conversations(home, cwd, 10, None).await.unwrap();

    // Build expected objects
    let p1 = home
        .join("sessions")
        .join("2025")
        .join("01")
        .join("03")
        .join(format!("rollout-2025-01-03T12-00-00-{u3}.jsonl"));
    let p2 = home
        .join("sessions")
        .join("2025")
        .join("01")
        .join("02")
        .join(format!("rollout-2025-01-02T12-00-00-{u2}.jsonl"));
    let p3 = home
        .join("sessions")
        .join("2025")
        .join("01")
        .join("01")
        .join(format!("rollout-2025-01-01T12-00-00-{u1}.jsonl"));

    let head_3 = vec![serde_json::json!({
        "id": u3,
        "timestamp": "2025-01-03T12-00-00",
        "instructions": null,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version"
    })];
    let head_2 = vec![serde_json::json!({
        "id": u2,
        "timestamp": "2025-01-02T12-00-00",
        "instructions": null,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version"
    })];
    let head_1 = vec![serde_json::json!({
        "id": u1,
        "timestamp": "2025-01-01T12-00-00",
        "instructions": null,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version"
    })];

    let expected_cursor: Cursor =
        serde_json::from_str(&format!("\"2025-01-01T12-00-00|{u1}\"")).unwrap();

    let expected = ConversationsPage {
        items: vec![
            ConversationItem {
                path: p1,
                head: head_3,
            },
            ConversationItem {
                path: p2,
                head: head_2,
            },
            ConversationItem {
                path: p3,
                head: head_1,
            },
        ],
        next_cursor: Some(expected_cursor),
        num_scanned_files: 3,
        reached_scan_cap: false,
    };

    assert_eq!(page, expected);
}

#[tokio::test]
async fn test_list_conversations_filters_by_cwd() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let match_cwd = Path::new("/match");
    let other_cwd = Path::new("/other");

    let id_match = Uuid::from_u128(11);
    let id_other = Uuid::from_u128(22);

    write_session_file(home, "2025-02-01T00-00-00", id_match, 1, match_cwd).unwrap();
    write_session_file(home, "2025-02-02T00-00-00", id_other, 1, other_cwd).unwrap();

    let page_match = get_conversations(home, match_cwd, 10, None).await.unwrap();
    assert_eq!(page_match.items.len(), 1);
    let head_id = page_match.items[0]
        .head
        .first()
        .and_then(|meta| meta.get("id"))
        .and_then(|id| id.as_str())
        .expect("id str");
    assert_eq!(head_id, id_match.to_string());

    let page_other = get_conversations(home, other_cwd, 10, None).await.unwrap();
    assert_eq!(page_other.items.len(), 1);
    let other_head_id = page_other.items[0]
        .head
        .first()
        .and_then(|meta| meta.get("id"))
        .and_then(|id| id.as_str())
        .expect("id str");
    assert_eq!(other_head_id, id_other.to_string());

    let none_page = get_conversations(home, Path::new("/missing"), 10, None)
        .await
        .unwrap();
    assert!(none_page.items.is_empty());
}

#[tokio::test]
async fn test_pagination_cursor() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    // Fixed UUIDs for deterministic expectations
    let u1 = Uuid::from_u128(11);
    let u2 = Uuid::from_u128(22);
    let u3 = Uuid::from_u128(33);
    let u4 = Uuid::from_u128(44);
    let u5 = Uuid::from_u128(55);

    // Oldest to newest
    write_session_file(home, "2025-03-01T09-00-00", u1, 1, Path::new(".")).unwrap();
    write_session_file(home, "2025-03-02T09-00-00", u2, 1, Path::new(".")).unwrap();
    write_session_file(home, "2025-03-03T09-00-00", u3, 1, Path::new(".")).unwrap();
    write_session_file(home, "2025-03-04T09-00-00", u4, 1, Path::new(".")).unwrap();
    write_session_file(home, "2025-03-05T09-00-00", u5, 1, Path::new(".")).unwrap();

    let page1 = get_conversations(home, Path::new("."), 2, None)
        .await
        .unwrap();
    let p5 = home
        .join("sessions")
        .join("2025")
        .join("03")
        .join("05")
        .join(format!("rollout-2025-03-05T09-00-00-{u5}.jsonl"));
    let p4 = home
        .join("sessions")
        .join("2025")
        .join("03")
        .join("04")
        .join(format!("rollout-2025-03-04T09-00-00-{u4}.jsonl"));
    let head_5 = vec![serde_json::json!({
        "id": u5,
        "timestamp": "2025-03-05T09-00-00",
        "instructions": null,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version"
    })];
    let head_4 = vec![serde_json::json!({
        "id": u4,
        "timestamp": "2025-03-04T09-00-00",
        "instructions": null,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version"
    })];
    let expected_cursor1: Cursor =
        serde_json::from_str(&format!("\"2025-03-04T09-00-00|{u4}\"")).unwrap();
    let expected_page1 = ConversationsPage {
        items: vec![
            ConversationItem {
                path: p5,
                head: head_5,
            },
            ConversationItem {
                path: p4,
                head: head_4,
            },
        ],
        next_cursor: Some(expected_cursor1.clone()),
        num_scanned_files: 3, // scanned 05, 04, and peeked at 03 before breaking
        reached_scan_cap: false,
    };
    assert_eq!(page1, expected_page1);

    let page2 = get_conversations(home, Path::new("."), 2, page1.next_cursor.as_ref())
        .await
        .unwrap();
    let p3 = home
        .join("sessions")
        .join("2025")
        .join("03")
        .join("03")
        .join(format!("rollout-2025-03-03T09-00-00-{u3}.jsonl"));
    let p2 = home
        .join("sessions")
        .join("2025")
        .join("03")
        .join("02")
        .join(format!("rollout-2025-03-02T09-00-00-{u2}.jsonl"));
    let head_3 = vec![serde_json::json!({
        "id": u3,
        "timestamp": "2025-03-03T09-00-00",
        "instructions": null,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version"
    })];
    let head_2 = vec![serde_json::json!({
        "id": u2,
        "timestamp": "2025-03-02T09-00-00",
        "instructions": null,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version"
    })];
    let expected_cursor2: Cursor =
        serde_json::from_str(&format!("\"2025-03-02T09-00-00|{u2}\"")).unwrap();
    let expected_page2 = ConversationsPage {
        items: vec![
            ConversationItem {
                path: p3,
                head: head_3,
            },
            ConversationItem {
                path: p2,
                head: head_2,
            },
        ],
        next_cursor: Some(expected_cursor2.clone()),
        num_scanned_files: 5, // scanned 05, 04 (anchor), 03, 02, and peeked at 01
        reached_scan_cap: false,
    };
    assert_eq!(page2, expected_page2);

    let page3 = get_conversations(home, Path::new("."), 2, page2.next_cursor.as_ref())
        .await
        .unwrap();
    let p1 = home
        .join("sessions")
        .join("2025")
        .join("03")
        .join("01")
        .join(format!("rollout-2025-03-01T09-00-00-{u1}.jsonl"));
    let head_1 = vec![serde_json::json!({
        "id": u1,
        "timestamp": "2025-03-01T09-00-00",
        "instructions": null,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version"
    })];
    let expected_cursor3: Cursor =
        serde_json::from_str(&format!("\"2025-03-01T09-00-00|{u1}\"")).unwrap();
    let expected_page3 = ConversationsPage {
        items: vec![ConversationItem {
            path: p1,
            head: head_1,
        }],
        next_cursor: Some(expected_cursor3),
        num_scanned_files: 5, // scanned 05, 04 (anchor), 03, 02 (anchor), 01
        reached_scan_cap: false,
    };
    assert_eq!(page3, expected_page3);
}

#[tokio::test]
async fn test_get_conversation_contents() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let uuid = Uuid::new_v4();
    let ts = "2025-04-01T10-30-00";
    write_session_file(home, ts, uuid, 2, Path::new(".")).unwrap();

    let page = get_conversations(home, Path::new("."), 1, None)
        .await
        .unwrap();
    let path = &page.items[0].path;

    let content = get_conversation(path, Path::new(".")).await.unwrap();

    // Page equality (single item)
    let expected_path = home
        .join("sessions")
        .join("2025")
        .join("04")
        .join("01")
        .join(format!("rollout-2025-04-01T10-30-00-{uuid}.jsonl"));
    let expected_head = vec![serde_json::json!({
        "id": uuid,
        "timestamp": ts,
        "instructions": null,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version"
    })];
    let expected_cursor: Cursor = serde_json::from_str(&format!("\"{ts}|{uuid}\"")).unwrap();
    let expected_page = ConversationsPage {
        items: vec![ConversationItem {
            path: expected_path,
            head: expected_head,
        }],
        next_cursor: Some(expected_cursor),
        num_scanned_files: 1,
        reached_scan_cap: false,
    };
    assert_eq!(page, expected_page);

    // Entire file contents equality
    let meta = serde_json::json!({"timestamp": ts, "type": "session_meta", "payload": {"id": uuid, "timestamp": ts, "instructions": null, "cwd": ".", "originator": "test_originator", "cli_version": "test_version"}});
    let user_event = serde_json::json!({
        "timestamp": ts,
        "type": "event_msg",
        "payload": {"type": "user_message", "message": "Hello from user", "kind": "plain"}
    });
    let rec0 = serde_json::json!({"record_type": "response", "index": 0});
    let rec1 = serde_json::json!({"record_type": "response", "index": 1});
    let expected_content = format!("{meta}\n{user_event}\n{rec0}\n{rec1}\n");
    assert_eq!(content, expected_content);
}

#[tokio::test]
async fn test_stable_ordering_same_second_pagination() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let ts = "2025-07-01T00-00-00";
    let u1 = Uuid::from_u128(1);
    let u2 = Uuid::from_u128(2);
    let u3 = Uuid::from_u128(3);

    write_session_file(home, ts, u1, 0, Path::new(".")).unwrap();
    write_session_file(home, ts, u2, 0, Path::new(".")).unwrap();
    write_session_file(home, ts, u3, 0, Path::new(".")).unwrap();

    let page1 = get_conversations(home, Path::new("."), 2, None)
        .await
        .unwrap();

    let p3 = home
        .join("sessions")
        .join("2025")
        .join("07")
        .join("01")
        .join(format!("rollout-2025-07-01T00-00-00-{u3}.jsonl"));
    let p2 = home
        .join("sessions")
        .join("2025")
        .join("07")
        .join("01")
        .join(format!("rollout-2025-07-01T00-00-00-{u2}.jsonl"));
    let head = |u: Uuid| -> Vec<serde_json::Value> {
        vec![serde_json::json!({
            "id": u,
            "timestamp": ts,
            "instructions": null,
            "cwd": ".",
            "originator": "test_originator",
            "cli_version": "test_version"
        })]
    };
    let expected_cursor1: Cursor = serde_json::from_str(&format!("\"{ts}|{u2}\"")).unwrap();
    let expected_page1 = ConversationsPage {
        items: vec![
            ConversationItem {
                path: p3,
                head: head(u3),
            },
            ConversationItem {
                path: p2,
                head: head(u2),
            },
        ],
        next_cursor: Some(expected_cursor1.clone()),
        num_scanned_files: 3, // scanned u3, u2, peeked u1
        reached_scan_cap: false,
    };
    assert_eq!(page1, expected_page1);

    let page2 = get_conversations(home, Path::new("."), 2, page1.next_cursor.as_ref())
        .await
        .unwrap();
    let p1 = home
        .join("sessions")
        .join("2025")
        .join("07")
        .join("01")
        .join(format!("rollout-2025-07-01T00-00-00-{u1}.jsonl"));
    let expected_cursor2: Cursor = serde_json::from_str(&format!("\"{ts}|{u1}\"")).unwrap();
    let expected_page2 = ConversationsPage {
        items: vec![ConversationItem {
            path: p1,
            head: head(u1),
        }],
        next_cursor: Some(expected_cursor2),
        num_scanned_files: 3, // scanned u3, u2 (anchor), u1
        reached_scan_cap: false,
    };
    assert_eq!(page2, expected_page2);
}

#[tokio::test]
async fn test_get_rollout_history_enforces_cwd() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let ts = "2025-08-01T00-00-00";
    let uuid = Uuid::new_v4();
    let match_cwd = Path::new("/history-match");

    write_session_file(home, ts, uuid, 1, match_cwd).unwrap();

    let path = home
        .join("sessions")
        .join("2025")
        .join("08")
        .join("01")
        .join(format!("rollout-{ts}-{uuid}.jsonl"));

    RolloutRecorder::get_rollout_history(&path, match_cwd)
        .await
        .expect("matching cwd should succeed");

    let err = RolloutRecorder::get_rollout_history(&path, Path::new("/history-other"))
        .await
        .expect_err("mismatched cwd should error");
    assert!(err.to_string().contains("does not match"));
}

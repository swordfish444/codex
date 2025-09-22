use std::path::PathBuf;

use core_test_support::agent_harness;
use core_test_support::agent_harness::HarnessOutputs;
use pretty_assertions::assert_eq;
use serde_json::Value;

const HARNESS_FIXTURE_ROOT: &str = "tests/fixtures/harness";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn harness_fixtures_match_expectations() {
    for fixture in fixture_names() {
        run_fixture_test(&fixture).await;
    }
}

async fn run_fixture_test(fixture: &str) {
    let harness = agent_harness::run_fixture(fixture_dir(fixture))
        .await
        .unwrap_or_else(|err| panic!("run agent harness for fixture {fixture}: {err}"));

    assert_harness_outputs_match(fixture, &harness.actual, &harness.expected);
}

#[track_caller]
fn assert_harness_outputs_match(fixture: &str, actual: &HarnessOutputs, expected: &HarnessOutputs) {
    assert_value_matches(fixture, &actual.request, &expected.request, "request");
    assert_eq!(
        actual.events.len(),
        expected.events.len(),
        "event count mismatch in fixture {fixture}: expected {} events but got {}",
        expected.events.len(),
        actual.events.len()
    );
    for (index, expected_event) in expected.events.iter().enumerate() {
        let path = format!("events[{index}]");
        let actual_event = actual
            .events
            .get(index)
            .unwrap_or_else(|| panic!("missing actual event at {path} for fixture {fixture}"));
        assert_value_matches(fixture, actual_event, expected_event, &path);
    }
}

#[track_caller]
fn assert_value_matches(fixture: &str, actual: &Value, expected: &Value, path: &str) {
    match expected {
        Value::Object(expected_map) => {
            let actual_map = actual.as_object().unwrap_or_else(|| {
                panic!("expected object at {path} in fixture {fixture}, got {actual:?}")
            });
            for (key, expected_value) in expected_map {
                let next_path = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{path}.{key}")
                };
                let actual_value = actual_map.get(key).unwrap_or_else(|| {
                    panic!("missing field {next_path} in actual value for fixture {fixture}")
                });
                assert_value_matches(fixture, actual_value, expected_value, &next_path);
            }
        }
        Value::Array(expected_items) => {
            let actual_items = actual.as_array().unwrap_or_else(|| {
                panic!("expected array at {path} in fixture {fixture}, got {actual:?}")
            });
            assert_eq!(
                actual_items.len(),
                expected_items.len(),
                "array length mismatch at {path} in fixture {fixture}"
            );
            for (index, expected_value) in expected_items.iter().enumerate() {
                let next_path = if path.is_empty() {
                    format!("[{index}]")
                } else {
                    format!("{path}[{index}]")
                };
                let actual_value = actual_items.get(index).unwrap_or_else(|| {
                    panic!("missing array element at {next_path} for fixture {fixture}")
                });
                assert_value_matches(fixture, actual_value, expected_value, &next_path);
            }
        }
        _ => {
            assert_eq!(actual, expected, "mismatch at {path} in fixture {fixture}");
        }
    }
}
fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(HARNESS_FIXTURE_ROOT)
}

fn fixture_dir(fixture: &str) -> PathBuf {
    fixtures_root().join(fixture)
}

fn fixture_names() -> Vec<String> {
    let dir = fixtures_root();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("read fixture directory {}: {err}", dir.display()))
        .filter_map(|entry| {
            entry.ok().and_then(|e| {
                let path = e.path();
                (path.is_dir()).then(|| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| name.to_string())
                })
            })
        })
        .flatten()
        .collect();
    names.sort();
    names
}

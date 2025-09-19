use std::path::PathBuf;

use core_test_support::agent_harness;
use pretty_assertions::assert_eq;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn single_turn_fixture_matches_expectations() {
    let fixture_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/harness/single_turn");
    let harness = agent_harness::run_fixture(fixture_dir)
        .await
        .expect("run agent harness");

    assert_eq!(harness.actual, harness.expected);
}

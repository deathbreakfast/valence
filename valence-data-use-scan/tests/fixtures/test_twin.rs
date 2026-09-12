//! Test-only twin that must never appear in the UI snapshot when exclusion is on.

async fn _fixture_test_twin() {
    let _ = User::get_used(
        "id",
        &valence,
        use_!("TEST_ONLY_PURPOSE — must stay out of UI snapshot"),
    )
    .await;
}

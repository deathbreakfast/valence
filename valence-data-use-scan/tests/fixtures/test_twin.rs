//! Test twin fixture — must stay out of UI snapshot when exclude_tests is on.

async fn _fixture_test_twin() {
    let _ = User::get(
        "id",
        &valence,
        valence::use_!("**Test:** Fixture **user** load for the data-use scan twin suite so exclude-tests snapshot coverage can assert harness purposes stay out of the operator UI. CI and developers running the suite only."),
    )
    .await;
}

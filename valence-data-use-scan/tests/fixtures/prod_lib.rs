//! Fixture sources embedded by unit tests (also kept as files for review).

async fn _fixture_prod_examples() {
    let _ = User::get_used(
        "id",
        &valence,
        use_!(r#"
Load the user for the session cookie.
"#),
    )
    .await;

    let _ = User::create_used(
        row,
        &valence,
        use_!("Create a user during signup."),
    )
    .await;

    let _ = NamedQueryAll::query_used(
        &valence,
        use_!("List named entities for the admin picker."),
    )
    .await;

    let _ = QueryCore::execute_used(
        builder,
        use_!("Run the Valence graph walk across registered models."),
    )
    .await;

    let _ = User::merge_used(
        id,
        patch,
        &valence,
        use_!("Merge profile fields after account edit."),
    )
    .await;

    let _ = User::delete_now_used(
        id,
        &valence,
        use_!("Hard-delete a draft user that never activated."),
    )
    .await;
}

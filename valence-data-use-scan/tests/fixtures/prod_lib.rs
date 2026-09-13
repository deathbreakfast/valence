//! Fixture sources embedded by unit tests (also kept as files for review).

async fn _fixture_prod_examples() {
    let _ = User::get_used(
        "id",
        &valence,
        valence::use_!(r#"
When your browser presents a **session cookie**, we **load the matching user account** so sign-in can continue. The application uses this only to establish who is signed in for that request—not to render a profile page by itself.
"#),
    )
    .await;

    let _ = User::create_used(
        row,
        &valence,
        valence::use_!("When someone **signs up**, we **create their user account** so they can sign in and use the product. Account operators and the new user rely on this row for identity—not a public directory listing."),
    )
    .await;

    let _ = NamedQueryAll::query_used(
        &valence,
        valence::use_!("On the **admin picker**, we **list named entities** so an operator can choose which record to open. Only people with access to that admin surface use this list."),
    )
    .await;

    let _ = QueryCore::execute_used(
        builder,
        valence::use_!("When Valence runs a **graph walk** across registered models, we **execute that query** so deletion and connection tools can traverse related rows. Platform operators and automation use the result—not end-user profile UIs."),
    )
    .await;

    let _ = User::merge_used(
        id,
        patch,
        &valence,
        valence::use_!("After an **account edit**, we **merge updated profile fields** onto the user record so later requests see what changed. The signed-in user and account flows use this updated row."),
    )
    .await;

    let _ = User::delete_now_used(
        id,
        &valence,
        valence::use_!("When a **draft signup** never activates, we **remove that user account immediately** so incomplete registrations do not linger. Only the cleanup path performing this removal uses the result."),
    )
    .await;
}

//! Template and hedge lint: every phrase has a sad and a happy case, and the
//! older checks keep their results after normalization.

use valence_data_use_scan::{
    inventory_csv_header, inventory_csv_row, lint_purpose, lint_scan_hits, matching_phrases,
    purpose_passes, GapCode, PurposeTier, ScanHit, HEDGE_PHRASES, TEMPLATE_PHRASES,
};

fn sentence(phrase: &str) -> String {
    format!(
        "When an operator opens the **billing console**, we load the plan and invoice list so the page can render them and the operator can review charges. {phrase}."
    )
}

#[test]
fn every_template_phrase_is_banned() {
    assert_eq!(TEMPLATE_PHRASES.len(), 8);
    for phrase in TEMPLATE_PHRASES {
        let gaps = lint_purpose(&sentence(phrase), PurposeTier::S1);
        assert!(gaps.contains(&GapCode::BanTemplate), "{phrase}: {gaps:?}");
        assert!(!gaps.contains(&GapCode::Hedge), "{phrase}: {gaps:?}");
    }
}

#[test]
fn every_hedge_phrase_is_flagged() {
    for phrase in HEDGE_PHRASES {
        let gaps = lint_purpose(&sentence(phrase), PurposeTier::S1);
        assert!(gaps.contains(&GapCode::Hedge), "{phrase}: {gaps:?}");
    }
}

#[test]
fn full_hedge_wordings_are_flagged() {
    for p in [
        "The value is not necessarily displayed on a page unless that feature's UI shows it, so the operator sees the plan name and the invoice total.",
        "The value is not necessarily displayed on a page unless that feature\u{2019}s UI shows it, so the operator sees the plan name and the invoice total.",
        "The value is not **necessarily** shown, so the operator sees the plan name and the invoice total on the console.",
    ] {
        assert!(
            lint_purpose(p, PurposeTier::S1).contains(&GapCode::Hedge),
            "{p}"
        );
    }
}

#[test]
fn bold_split_and_curly_templates_are_banned() {
    for p in [
        "In **images**, we **update this data** so **later** steps see the latest values for this workflow and the page shows the new image name to the operator.",
        "We **save it** so the next step in that feature can **continue with the latest values** and the operator keeps their place in the wizard today.",
        "It is not shown\u{a0}as a  standalone end-user page   by itself, and only the cleanup job reads the rows that remain after a failed upload.",
    ] {
        assert!(
            lint_purpose(p, PurposeTier::S1).contains(&GapCode::BanTemplate),
            "{p}"
        );
    }
}

#[test]
fn specific_purposes_pass() {
    for p in [
        "When an operator opens the **Applications** page, we **load each application's name, image and desired instance count** so the table can list them. Only operators with access to that console see this list, and reconcile reads the same rows to start containers.",
        "When a user **changes their display name**, we **save the new name** so it appears in the page header and in the member list on their next visit. Other members of the workspace see it; it is not sent anywhere else.",
        "When you **cancel a pending invite**, we **delete the invite row** unless the invitee already accepted it, so the link stops working. The invitee never needs to be contacted and nothing is necessarily kept.",
    ] {
        assert!(
            purpose_passes(p, PurposeTier::S1),
            "{p}: {:?}",
            lint_purpose(p, PurposeTier::S1)
        );
    }
}

#[test]
fn near_misses_do_not_hedge() {
    for p in [
        "The invite is deleted unless the user cancels first, so the link stops working and the invitee sees an expired notice on the sign-in page.",
        "Billing checks are necessarily run before the invoice is sent, so the operator sees the total on the confirmation page and can still edit lines.",
    ] {
        assert!(
            !lint_purpose(p, PurposeTier::S1).contains(&GapCode::Hedge),
            "{p}"
        );
    }
}

#[test]
fn two_phrases_yield_one_gap_entry() {
    let p = "We decide what to do next in this workflow, and it is not necessarily displayed on a page unless that feature's UI shows it, so the operator keeps their place.";
    let gaps = lint_purpose(p, PurposeTier::S1);
    assert_eq!(
        gaps.iter().filter(|g| **g == GapCode::BanTemplate).count(),
        1
    );
    assert_eq!(gaps.iter().filter(|g| **g == GapCode::Hedge).count(), 1);
    assert_eq!(matching_phrases(p).len(), 3);
}

#[test]
fn emphasis_does_not_break_clean_purposes() {
    let p = "When your browser presents a **refresh token**, we **load the matching session** so sign-in can continue without asking for a password again. Only the sign-in flow reads this row, and it never reaches a page.";
    assert!(
        purpose_passes(p, PurposeTier::S1),
        "{:?}",
        lint_purpose(p, PurposeTier::S1)
    );
}

#[test]
fn older_checks_are_unchanged() {
    let p = "get User in src/totp/api.rs; Valence persistence for this feature path; typed store; visible to session actor / service path.";
    let gaps = lint_purpose(p, PurposeTier::S3);
    for g in [GapCode::BanTemplate, GapCode::BanPath, GapCode::BanJargon] {
        assert!(gaps.contains(&g), "{g}: {gaps:?}");
    }
    assert!(!gaps.contains(&GapCode::Hedge));

    let ritual = "We load your profile for the account page. Not copied outside Valence for this step. You can edit the display name.";
    assert!(lint_purpose(ritual, PurposeTier::S3).contains(&GapCode::BanRitualNegative));

    let ctx = "While you set up your authenticator, we save a pending factor for your account including a sealed copy of the authenticator secret so enrollment can finish after you confirm with a code from the app.";
    assert!(lint_purpose(ctx, PurposeTier::S3).contains(&GapCode::CodeContext));

    // S0 prefix is checked on the original text, so emphasis stripping must not hide it.
    let no_prefix = "Builds a minimal user fixture for the account wipe suite and asserts removal.";
    assert!(lint_purpose(no_prefix, PurposeTier::S0).contains(&GapCode::TestPrefix));
    let ok = "**Test:** Builds a minimal **user** fixture for lepton-auth `tests/account_wipe`, then asserts wipe removed it. CI and developers running the suite only.";
    assert!(purpose_passes(ok, PurposeTier::S0));
    let stripped = "Test: Builds a minimal user fixture for lepton-auth tests, then asserts wipe removed it. CI only.";
    assert!(lint_purpose(stripped, PurposeTier::S0).contains(&GapCode::TestPrefix));
}

#[test]
fn csv_row_carries_new_codes_and_header_is_stable() {
    assert_eq!(
        inventory_csv_header(),
        "file:line,crate,method,tier,score,gap_codes,purpose"
    );
    let hit = ScanHit {
        purpose: "We decide what to do next in this workflow; not necessarily shown.".to_string(),
        file: "shop/src/a.rs".to_string(),
        line: 3,
        crate_name: "shop".to_string(),
        repository: String::new(),
        target: valence_data_use_scan::TargetKind::Unscoped,
        op: valence_data_use_scan::OpKind::Read,
        method: "get".to_string(),
        connection_field: None,
        connection_kind: None,
        referenced_schema: None,
    };
    let rows = lint_scan_hits(&[hit]);
    let line = inventory_csv_row(&rows[0]);
    assert!(line.contains("BAN_TEMPLATE"), "{line}");
    assert!(line.contains("HEDGE"), "{line}");
    assert!(
        line.starts_with("shop/src/a.rs:3,shop,get,S1,Fail,"),
        "{line}"
    );
}

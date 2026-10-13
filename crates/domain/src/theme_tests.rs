use super::{FunnelId, RenderManifest, Revision, RevisionId, Section, ThemePreset};

#[test]
fn preset_catalog_has_five_stable_ids_and_distinct_tokens() {
    let presets = ThemePreset::all();
    let ids = presets.iter().map(|preset| preset.id()).collect::<Vec<_>>();

    assert_eq!(ids, ["minimal", "bold", "elegant", "playful", "corporate"]);
    assert_eq!(
        presets
            .iter()
            .map(|preset| preset.name())
            .collect::<Vec<_>>(),
        ["Minimal", "Bold", "Elegant", "Playful", "Corporate",]
    );
    let token_json = presets
        .iter()
        .map(|preset| {
            assert!(preset.tokens().len() >= 10);
            preset.tokens_json().expect("preset tokens serialize")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        token_json
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        5
    );
}

#[test]
fn preset_ids_round_trip_through_json_and_unknown_ids_are_rejected() {
    for preset in ThemePreset::all() {
        assert_eq!(ThemePreset::from_id(preset.id()), Some(*preset));
        assert_eq!(
            serde_json::to_string(preset).expect("preset JSON encoding"),
            format!("\"{}\"", preset.id())
        );
    }

    assert_eq!(ThemePreset::from_id("custom"), None);
    assert!(serde_json::from_str::<ThemePreset>("\"custom\"").is_err());
}

#[test]
fn theme_fork_updates_the_manifest_without_mutating_the_source_revision() {
    let funnel_id = FunnelId::new("theme-funnel").expect("valid funnel ID");
    let original_id = RevisionId::new("theme-revision-before").expect("valid revision ID");
    let original = Revision::new(
        original_id.clone(),
        funnel_id,
        Some(1),
        vec![Section::new(Some("hero".to_owned()), "hero", 0).expect("valid hero section")],
        RenderManifest::new(
            original_id,
            "theme-v1",
            "renderer-v1",
            "assets-sha256",
            "config-sha256",
        )
        .expect("valid manifest"),
    )
    .expect("valid source revision");
    let next_id = RevisionId::new("theme-revision-after").expect("valid revision ID");

    let updated = original
        .fork_with_theme(next_id.clone(), "sha256-v1:corporate", "renderer-v2")
        .expect("theme selection creates a valid revision");

    assert_eq!(original.id().as_str(), "theme-revision-before");
    assert_eq!(original.render_manifest().theme_version(), "theme-v1");
    assert_eq!(updated.id(), &next_id);
    assert_eq!(updated.render_manifest().revision_id(), &next_id);
    assert_eq!(
        updated.render_manifest().theme_version(),
        "sha256-v1:corporate"
    );
    assert_eq!(updated.render_manifest().renderer_version(), "renderer-v2");
    assert_eq!(updated.funnel_id(), original.funnel_id());
    assert_eq!(updated.schema_version(), original.schema_version());
    assert_eq!(updated.sections(), original.sections());
    assert_eq!(
        updated.render_manifest().assets_manifest_hash(),
        original.render_manifest().assets_manifest_hash()
    );
    assert_eq!(
        updated.render_manifest().config_hash(),
        original.render_manifest().config_hash()
    );
}

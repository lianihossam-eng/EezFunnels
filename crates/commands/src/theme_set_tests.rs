use super::ThemeSetCommand;
use eez_domain::{FunnelId, RenderManifest, Revision, RevisionId, ThemePreset};
use eez_render::Renderer;

fn revision() -> Revision {
    let id = RevisionId::new("theme-command-source").expect("valid source revision ID");
    Revision::new(
        id.clone(),
        FunnelId::new("theme-command-funnel").expect("valid funnel ID"),
        Some(1),
        Vec::new(),
        RenderManifest::new(
            id,
            "theme-v1",
            "renderer-v1",
            "assets-hash",
            eez_render::content_config_hash(&std::collections::BTreeMap::new()),
        )
        .expect("valid render manifest"),
    )
    .expect("valid source revision")
}

#[test]
fn theme_set_parses_stable_json_and_rejects_unknown_theme_ids() {
    let command = ThemeSetCommand::from_json(r#"{"theme_id":"playful"}"#)
        .expect("known theme command parses");

    assert_eq!(ThemeSetCommand::ID, "theme.set");
    assert_eq!(command.theme_id(), ThemePreset::Playful);
    assert!(ThemeSetCommand::from_json(r#"{"theme_id":"custom"}"#).is_err());
    assert!(ThemeSetCommand::from_json(r#"{"theme_id":"minimal","other":true}"#).is_err());
}

#[test]
fn theme_set_returns_a_new_renderable_revision_without_mutating_the_source() {
    let source = revision();
    let source_id = source.id().clone();
    let command = ThemeSetCommand::from_json(r#"{"theme_id":"corporate"}"#)
        .expect("known theme command parses");
    let next_id = RevisionId::new("theme-command-result").expect("valid next revision ID");
    let result = command
        .execute(&source, next_id.clone())
        .expect("theme set creates a new render snapshot");

    assert_eq!(source.id(), &source_id);
    assert_eq!(source.render_manifest().theme_version(), "theme-v1");
    assert_eq!(result.revision().id(), &next_id);
    assert_eq!(
        result.revision().render_manifest().theme_version(),
        result.tokens().version()
    );
    assert_eq!(
        result.revision().render_manifest().renderer_version(),
        eez_render::RENDERER_VERSION
    );

    let html = Renderer::new()
        .render(
            result.revision(),
            result.tokens(),
            &std::collections::BTreeMap::new(),
        )
        .expect("selected preset renders");
    assert!(html.contains("--color-primary:#2563eb"));
    assert!(html.contains("background-color:var(--color-background)"));
}

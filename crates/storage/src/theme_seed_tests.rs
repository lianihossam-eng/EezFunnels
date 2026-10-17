use eez_domain::ThemePreset;

const THEME_MIGRATION: &str = include_str!("../../../migrations/0003_theme_presets.sql");
const ACCESSIBLE_PLAYFUL_THEME_MIGRATION: &str =
    include_str!("../../../migrations/0010_accessible_playful_theme.sql");

#[test]
fn theme_seed_migration_uses_the_domain_catalog_token_json() {
    for preset in ThemePreset::all() {
        let tokens_json = preset.tokens_json().expect("preset JSON serialization");
        if preset.id() == "playful" {
            assert!(THEME_MIGRATION.contains("WHERE id = 'playful';"));
            continue;
        }
        let statement = format!(
            "UPDATE themes SET tokens_json = '{tokens_json}', updated_at = CURRENT_TIMESTAMP WHERE id = '{}';",
            preset.id()
        );

        assert!(
            THEME_MIGRATION.contains(&statement),
            "missing seed for {}",
            preset.id()
        );
    }
}

#[test]
fn playful_accent_migration_preserves_customized_theme_rows() {
    assert!(ACCESSIBLE_PLAYFUL_THEME_MIGRATION
        .contains(r#"json_set(tokens_json, '$."color-primary"', '#c026d3')"#));
    assert!(ACCESSIBLE_PLAYFUL_THEME_MIGRATION
        .contains(r#"json_extract(tokens_json, '$."color-primary"') = '#d946ef'"#));
    let tokens = ThemePreset::Playful
        .tokens_json()
        .expect("playful preset JSON serialization");
    let value: serde_json::Value = serde_json::from_str(&tokens).expect("valid preset JSON");
    assert_eq!(value["color-primary"], "#c026d3");
}

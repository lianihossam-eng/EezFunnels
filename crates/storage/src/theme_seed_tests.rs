use eez_domain::ThemePreset;

const THEME_MIGRATION: &str = include_str!("../../../migrations/0003_theme_presets.sql");

#[test]
fn theme_seed_migration_uses_the_domain_catalog_token_json() {
    for preset in ThemePreset::all() {
        let tokens_json = preset.tokens_json().expect("preset JSON serialization");
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

use eez_domain::document_v2::{ActionV2, BlockV2, BlueprintId, DocumentV2, Edit, IdKind, IdSource};
use eez_domain::ThemePreset;

const MINIMAL: &str = include_str!("fixtures/document_v2_minimal.json");

struct TestIds(u32);

impl IdSource for TestIds {
    fn next_id(&mut self, _kind: IdKind) -> Result<String, eez_domain::document_v2::DocumentError> {
        self.0 += 1;
        Ok(format!("copy-{}", self.0))
    }
}

fn decode(source: &str) -> DocumentV2 {
    DocumentV2::decode(source.as_bytes(), "funnel-01").expect("fixture is a valid V2 document")
}

#[test]
fn v2_decoder_rejects_duplicate_unknown_and_conflicting_fields() {
    let duplicate = MINIMAL.replace(
        "\"document_version\":2",
        "\"document_version\":2,\"document_version\":2",
    );
    assert!(DocumentV2::decode(duplicate.as_bytes(), "funnel-01").is_err());

    let unknown = MINIMAL.replace(
        "\"funnel_id\":\"funnel-01\"",
        "\"funnel_id\":\"funnel-01\",\"legacy_format_version\":2",
    );
    assert!(DocumentV2::decode(unknown.as_bytes(), "funnel-01").is_err());

    assert!(DocumentV2::decode(MINIMAL.as_bytes(), "another-funnel").is_err());
    for version in [1, 3] {
        let mismatched = MINIMAL.replace(
            "\"document_version\":2",
            &format!("\"document_version\":{version}"),
        );
        assert!(DocumentV2::decode(mismatched.as_bytes(), "funnel-01").is_err());
    }
}

#[test]
fn canonical_bytes_are_independent_of_input_whitespace_and_property_order() {
    let document = decode(MINIMAL);
    let reordered = MINIMAL
        .replace(
            "\"document_type\":\"eezfunnel.funnel-document\",\"document_version\":2,\"schema_version\":2",
            "\"schema_version\":2, \"document_version\":2, \"document_type\":\"eezfunnel.funnel-document\"",
        )
        .replace(",", ",\n  ");
    let other = decode(&reordered);

    assert_eq!(document.canonical_bytes(), other.canonical_bytes());
    assert_eq!(
        std::str::from_utf8(document.canonical_bytes()).expect("canonical JSON is UTF-8"),
        r#"{"blueprint_id":"lead_gen","document_type":"eezfunnel.funnel-document","document_version":2,"entry_page_id":"page-home","forms":[],"funnel_id":"funnel-01","pages":[{"id":"page-home","sections":[{"blocks":[{"id":"block-title","kind":"heading","level":1,"text":"Hello"},{"id":"block-copy","kind":"text","text":"Body copy"}],"id":"section-hero","type":"hero"}],"slug":"","title":"Home"}],"schema_version":2,"theme":{"overrides":{},"preset_id":"minimal"}}"#
    );
}

#[test]
fn document_edits_are_atomic_and_duplicate_every_copied_node_id() {
    let original = decode(MINIMAL);
    let page = original.pages().first().expect("entry page").id().clone();
    let section = original.pages()[0].sections()[0].id().clone();
    let mut ids = TestIds(0);

    let edited = original
        .apply(
            Edit::DuplicateSection {
                page: page.clone(),
                section,
            },
            &mut ids,
        )
        .expect("valid section duplication");

    assert_eq!(original.pages()[0].sections().len(), 1);
    let sections = edited.document.pages()[0].sections();
    assert_eq!(sections.len(), 2);
    assert_ne!(sections[0].id(), sections[1].id());
    let original_blocks = sections[0].blocks();
    let copied_blocks = sections[1].blocks();
    assert_eq!(original_blocks.len(), copied_blocks.len());
    for (original, copied) in original_blocks.iter().zip(copied_blocks) {
        assert_ne!(original.id(), copied.id());
    }
    assert_eq!(edited.focus.section_id(), Some(sections[1].id()));
}

#[test]
fn typed_edits_round_trip_as_closed_versioned_operations() {
    let edit = Edit::AddSection {
        page: eez_domain::document_v2::PageId::parse("page-home").expect("valid page id"),
        after: None,
        section_type: eez_domain::document_v2::SectionKind::Features,
    };
    let encoded = serde_json::to_value(&edit).expect("typed edit serializes");
    assert_eq!(encoded["operation"], "add_section");
    assert_eq!(
        serde_json::from_value::<Edit>(encoded.clone()).expect("typed edit deserializes"),
        edit
    );

    let mut unknown = encoded;
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Edit>(unknown).is_err());
}

#[test]
fn typed_theme_preset_edits_preserve_document_content_and_change_resolution() {
    let original = decode(MINIMAL);
    let mut ids = TestIds(0);
    let changed = original
        .apply(
            Edit::SetThemePreset {
                preset: ThemePreset::Elegant,
            },
            &mut ids,
        )
        .expect("preset edit validates");
    assert_eq!(changed.document.theme().preset_id(), ThemePreset::Elegant);
    assert_eq!(changed.document.pages(), original.pages());
    assert_ne!(
        changed.document.resolved_theme_profile(),
        original.resolved_theme_profile()
    );
}

#[test]
fn explicit_v2_starter_creates_a_native_editable_document_without_v1_conversion() {
    let document = DocumentV2::starter(
        "new-funnel",
        BlueprintId::LeadGen,
        eez_domain::ThemePreset::Minimal,
    )
    .expect("valid native V2 starter");

    assert_eq!(document.funnel_id().as_str(), "new-funnel");
    assert_eq!(document.pages().len(), 1);
    assert_eq!(
        document.pages()[0]
            .sections()
            .iter()
            .map(|section| match section.section_type() {
                eez_domain::document_v2::SectionKind::Hero => "hero",
                eez_domain::document_v2::SectionKind::Features => "features",
                eez_domain::document_v2::SectionKind::Testimonials => "testimonials",
                eez_domain::document_v2::SectionKind::Cta => "cta",
                eez_domain::document_v2::SectionKind::Footer => "footer",
            })
            .collect::<Vec<_>>(),
        ["hero", "features", "cta", "footer"]
    );
    assert_eq!(
        document.pages()[0]
            .sections()
            .iter()
            .map(|section| section.blocks().len())
            .sum::<usize>(),
        8
    );
    assert!(!document.has_images());
    assert!(!document.has_forms());
}

#[test]
fn typed_starters_follow_each_blueprints_section_outline() {
    let cases = [
        (
            BlueprintId::Tripwire,
            ["hero", "features", "cta"].as_slice(),
        ),
        (BlueprintId::Vsl, ["hero", "testimonials", "cta"].as_slice()),
        (
            BlueprintId::Webinar,
            ["hero", "features", "testimonials", "cta"].as_slice(),
        ),
        (
            BlueprintId::Launch,
            ["hero", "features", "testimonials", "cta", "footer"].as_slice(),
        ),
        (
            BlueprintId::LeadGen,
            ["hero", "features", "cta", "footer"].as_slice(),
        ),
    ];

    for (index, (blueprint, expected)) in cases.into_iter().enumerate() {
        let document =
            DocumentV2::starter(&format!("starter-{index}"), blueprint, ThemePreset::Minimal)
                .expect("valid typed starter");
        let actual = document.pages()[0]
            .sections()
            .iter()
            .map(|section| match section.section_type() {
                eez_domain::document_v2::SectionKind::Hero => "hero",
                eez_domain::document_v2::SectionKind::Features => "features",
                eez_domain::document_v2::SectionKind::Testimonials => "testimonials",
                eez_domain::document_v2::SectionKind::Cta => "cta",
                eez_domain::document_v2::SectionKind::Footer => "footer",
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
}

#[test]
fn document_graph_rejects_cycles_and_unreachable_pages() {
    let cycle = MINIMAL.replace(
        "\"blocks\":[{\"id\":\"block-title\",\"kind\":\"heading\",\"level\":1,\"text\":\"Hello\"},{\"id\":\"block-copy\",\"kind\":\"text\",\"text\":\"Body copy\"}]",
        "\"blocks\":[{\"id\":\"block-link\",\"kind\":\"button\",\"label\":\"Next\",\"action\":{\"kind\":\"navigate_page\",\"page_id\":\"page-next\"}}]",
    )
    .replace(
        "\"pages\":[{\"id\":\"page-home\",\"slug\":\"\",\"title\":\"Home\",\"sections\":[{\"id\":\"section-hero\",\"type\":\"hero\",\"blocks\":[{\"id\":\"block-link\",\"kind\":\"button\",\"label\":\"Next\",\"action\":{\"kind\":\"navigate_page\",\"page_id\":\"page-next\"}}]}]}]",
        "\"pages\":[{\"id\":\"page-home\",\"slug\":\"\",\"title\":\"Home\",\"sections\":[{\"id\":\"section-hero\",\"type\":\"hero\",\"blocks\":[{\"id\":\"block-link\",\"kind\":\"button\",\"label\":\"Next\",\"action\":{\"kind\":\"navigate_page\",\"page_id\":\"page-next\"}}]}]},{\"id\":\"page-next\",\"slug\":\"next\",\"title\":\"Next\",\"sections\":[{\"id\":\"section-next\",\"type\":\"hero\",\"blocks\":[{\"id\":\"block-back\",\"kind\":\"button\",\"label\":\"Back\",\"action\":{\"kind\":\"navigate_page\",\"page_id\":\"page-home\"}}]}]}]",
    );
    assert!(DocumentV2::decode(cycle.as_bytes(), "funnel-01").is_err());

    let unreachable = MINIMAL.replace(
        "\"pages\":[{\"id\":\"page-home\",\"slug\":\"\",\"title\":\"Home\",\"sections\":[{\"id\":\"section-hero\",\"type\":\"hero\",\"blocks\":[{\"id\":\"block-title\",\"kind\":\"heading\",\"level\":1,\"text\":\"Hello\"},{\"id\":\"block-copy\",\"kind\":\"text\",\"text\":\"Body copy\"}]}]}]",
        "\"pages\":[{\"id\":\"page-home\",\"slug\":\"\",\"title\":\"Home\",\"sections\":[{\"id\":\"section-hero\",\"type\":\"hero\",\"blocks\":[{\"id\":\"block-title\",\"kind\":\"heading\",\"level\":1,\"text\":\"Hello\"},{\"id\":\"block-copy\",\"kind\":\"text\",\"text\":\"Body copy\"}]}]},{\"id\":\"page-orphan\",\"slug\":\"orphan\",\"title\":\"Orphan\",\"sections\":[]}]",
    );
    assert!(DocumentV2::decode(unreachable.as_bytes(), "funnel-01").is_err());
}

#[test]
fn external_urls_are_validated_then_persisted_in_whatwg_canonical_form() {
    let document = with_external_url("HTTPS://ExAmPlE.COM:443/landing");
    let BlockV2::Button {
        action: ActionV2::ExternalHttps { url, .. },
        ..
    } = &document.pages()[0].sections()[0].blocks()[0]
    else {
        panic!("fixture contains an external button");
    };
    assert_eq!(url, "https://example.com/landing");

    for invalid in [
        "/contact",
        "https:example.com",
        "https:///path",
        "https://user:secret@example.com/",
        "https://example.com\\@evil.example/",
        "https://example.com/a b",
        "https://example.com/%0a",
        "https://example.com/%5C",
    ] {
        assert!(
            with_external_url_result(invalid).is_err(),
            "URL should fail closed: {invalid}"
        );
    }

    let idn = with_external_url("https://münich.example/path");
    let BlockV2::Button {
        action: ActionV2::ExternalHttps { url, .. },
        ..
    } = &idn.pages()[0].sections()[0].blocks()[0]
    else {
        panic!("fixture contains an external button");
    };
    assert_eq!(url, "https://xn--mnich-kva.example/path");
}

#[test]
fn optional_fields_reject_null_and_closed_node_shapes_reject_extra_fields() {
    let null_override = MINIMAL.replace("\"overrides\":{}", "\"overrides\":{\"type_scale\":null}");
    assert!(DocumentV2::decode(null_override.as_bytes(), "funnel-01").is_err());

    let extra_variant_field = MINIMAL.replace(
        "\"text\":\"Hello\"",
        "\"text\":\"Hello\",\"script\":\"alert(1)\"",
    );
    assert!(DocumentV2::decode(extra_variant_field.as_bytes(), "funnel-01").is_err());

    let too_deep = format!("{}{}{}", "{".repeat(8), "null", "}".repeat(8));
    assert!(DocumentV2::decode(too_deep.as_bytes(), "funnel-01").is_err());
}

#[test]
fn responsive_theme_breakpoints_resolve_independently() {
    let mut value: serde_json::Value = serde_json::from_str(MINIMAL).expect("valid JSON fixture");
    value["theme"]["overrides"] = serde_json::json!({
        "colors": {"color-primary": "#123456"},
        "spacing_scale": "airy",
        "responsive": {
            "tablet": {"spacing_scale": "relaxed"},
            "mobile": {"button_shape": "pill"}
        }
    });
    let serialized = serde_json::to_vec(&value).expect("JSON serialization");
    let document = DocumentV2::decode(&serialized, "funnel-01").expect("valid theme overrides");
    let profile = document.resolved_theme_profile();

    assert_eq!(profile.profile_version, "theme-profile-v1");
    assert_eq!(profile.desktop["color-primary"], "#123456");
    assert_eq!(profile.desktop["spacing-section"], "4.5rem");
    assert_eq!(profile.tablet["spacing-section"], "5rem");
    assert_eq!(profile.mobile["spacing-section"], "4.5rem");
    assert_eq!(profile.mobile["radius-button"], "9999px");
}

#[test]
fn typed_form_schema_rejects_reserved_names_and_accepts_the_closed_valid_shape() {
    let valid = with_form("email").expect("closed form shape is valid");
    assert!(valid.has_forms());

    let reserved = with_form("request_consent");
    assert!(reserved.is_err());

    let mut value: serde_json::Value = serde_json::from_str(MINIMAL).expect("valid JSON fixture");
    value["theme"]["overrides"] = serde_json::json!({});
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"block-form", "kind":"form", "form_id":"form-contact"}
    ]);
    value["forms"] = serde_json::json!([{
        "id":"form-contact",
        "fields":[{"id":"field-email", "name":"email", "label":"Email", "kind":"email", "required":true, "validation":null}],
        "purpose":"Contact request",
        "request_consent":{"copy_version":"request-v1", "copy":"I agree to be contacted."},
        "result":{"kind":"inline", "message":"Thanks."}
    }]);
    let serialized = serde_json::to_vec(&value).expect("JSON serialization");
    assert!(DocumentV2::decode(&serialized, "funnel-01").is_err());
}

fn with_external_url(url: &str) -> DocumentV2 {
    with_external_url_result(url).expect("external URL fixture is valid")
}

fn with_external_url_result(
    url: &str,
) -> Result<DocumentV2, eez_domain::document_v2::DocumentError> {
    let mut value: serde_json::Value = serde_json::from_str(MINIMAL).expect("valid JSON fixture");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"block-external", "kind":"button", "label":"Visit", "action":{"kind":"external_https", "url":url}},
        {"id":"block-next", "kind":"button", "label":"Next", "action":{"kind":"navigate_page", "page_id":"page-next"}}
    ]);
    value["pages"]
        .as_array_mut()
        .expect("pages array")
        .push(serde_json::json!({
            "id":"page-next", "slug":"next", "title":"Next", "sections":[]
        }));
    let serialized = serde_json::to_vec(&value).expect("JSON serialization");
    DocumentV2::decode(&serialized, "funnel-01")
}

fn with_form(field_name: &str) -> Result<DocumentV2, eez_domain::document_v2::DocumentError> {
    let mut value: serde_json::Value = serde_json::from_str(MINIMAL).expect("valid JSON fixture");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"block-form", "kind":"form", "form_id":"form-contact"}
    ]);
    value["forms"] = serde_json::json!([{
        "id":"form-contact",
        "fields":[{"id":"field-email", "name":field_name, "label":"Email", "kind":"email", "required":true}],
        "purpose":"Contact request",
        "request_consent":{"copy_version":"request-v1", "copy":"I agree to be contacted."},
        "result":{"kind":"inline", "message":"Thanks."}
    }]);
    let serialized = serde_json::to_vec(&value).expect("JSON serialization");
    DocumentV2::decode(&serialized, "funnel-01")
}

use eez_domain::document_v2::{
    validate_form_submission, ActionV2, BlockV2, BlueprintId, ContentWidth, DocumentV2, Edit,
    FontFamily, FormId, FormSubmissionValue, IdKind, IdSource, ThemeColors, ThemeOverrides,
    TypeScale,
};
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

fn document_with_lead_form() -> DocumentV2 {
    let mut value: serde_json::Value = serde_json::from_str(MINIMAL).expect("minimal document");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"block-form", "kind":"form", "form_id":"form-lead"}
    ]);
    value["forms"] = serde_json::json!([{
        "id":"form-lead",
        "fields":[
            {"id":"field-name", "kind":"text", "name":"name", "label":"Name", "required":true, "validation":{"min_length":2,"max_length":40}},
            {"id":"field-email", "kind":"email", "name":"email", "label":"Email", "required":true},
            {"id":"field-secondary-email", "kind":"email", "name":"secondary_email", "label":"Secondary email", "required":false},
            {"id":"field-phone", "kind":"phone", "name":"phone", "label":"Phone", "required":false},
            {"id":"field-topic", "kind":"select", "name":"topic", "label":"Topic", "required":true, "options":["Demo","Pricing"]},
            {"id":"field-follow-up", "kind":"checkbox", "name":"follow_up", "label":"May I follow up?", "required":false}
        ],
        "purpose":"Contact request",
        "request_consent":{"copy_version":"request-v1", "copy":"I agree to be contacted about this request."},
        "marketing_opt_in":{"copy_version":"marketing-v1", "copy":"Send me occasional product news."},
        "result":{"kind":"inline", "message":"Thanks for getting in touch."}
    }]);
    let encoded = serde_json::to_vec(&value).expect("form document serialization");
    DocumentV2::decode(&encoded, "funnel-01").expect("valid lead form document")
}

#[test]
fn form_fields_expose_the_submission_key_and_owner_facing_label() {
    let document = document_with_lead_form();
    let form = document
        .form(&FormId::parse("form-lead").expect("form id"))
        .expect("lead form");

    assert_eq!(form.fields()[0].submission_key(), "name");
    assert_eq!(form.fields()[0].label(), "Name");
    assert_eq!(form.fields()[2].submission_key(), "secondary_email");
    assert_eq!(form.fields()[2].label(), "Secondary email");
}

#[test]
fn theme_override_edit_updates_resolved_tokens_and_rejects_invalid_colors() {
    let document = DocumentV2::starter("funnel-theme", BlueprintId::LeadGen, ThemePreset::Minimal)
        .expect("starter document");
    let mut ids = TestIds(0);
    let overrides = ThemeOverrides {
        colors: Some(ThemeColors {
            color_primary: Some("#c026d3".to_owned()),
            color_on_primary: Some("#ffffff".to_owned()),
            ..ThemeColors::default()
        }),
        type_scale: Some(TypeScale::Large),
        font_family: Some(FontFamily::Georgia),
        content_width: Some(ContentWidth::Wide),
        ..ThemeOverrides::default()
    };
    let edited = document
        .apply(
            Edit::SetThemeOverrides {
                overrides: Box::new(overrides.clone()),
            },
            &mut ids,
        )
        .expect("valid theme overrides");
    let profile = edited.document.resolved_theme_profile();
    assert_eq!(profile.desktop["color-primary"], "#c026d3");
    assert_eq!(profile.desktop["color-on-primary"], "#ffffff");
    assert_eq!(profile.desktop["font-scale"], "1.125");
    assert_eq!(profile.desktop["font-family"], "Georgia, serif");
    assert_eq!(profile.desktop["content-width"], "80rem");

    let reset = document
        .apply(
            Edit::SetThemeOverrides {
                overrides: Box::new(ThemeOverrides::default()),
            },
            &mut ids,
        )
        .expect("preset defaults can be restored");
    assert_eq!(
        reset.document.resolved_theme_profile().desktop["color-primary"],
        "#334155"
    );

    let invalid = ThemeOverrides {
        colors: Some(ThemeColors {
            color_primary: Some("red".to_owned()),
            ..ThemeColors::default()
        }),
        ..ThemeOverrides::default()
    };
    let error = document
        .apply(
            Edit::SetThemeOverrides {
                overrides: Box::new(invalid),
            },
            &mut ids,
        )
        .expect_err("invalid color must not enter the canonical document");
    assert_eq!(error.code(), "validation/theme_color_invalid");
}

#[test]
fn public_form_submission_validation_keeps_request_and_marketing_choices_separate() {
    let document = document_with_lead_form();
    let form = document
        .form(&FormId::parse("form-lead").expect("form id"))
        .expect("lead form");
    let fields = std::collections::BTreeMap::from([
        ("name".to_owned(), "Ada Lovelace".to_owned()),
        ("email".to_owned(), "ada@example.test".to_owned()),
        ("topic".to_owned(), "Demo".to_owned()),
        ("follow_up".to_owned(), "on".to_owned()),
    ]);

    let submission = validate_form_submission(form, &fields, true, false)
        .expect("valid request without marketing opt-in");

    assert!(!submission.marketing_opt_in());
    assert_eq!(
        submission.values().get("name"),
        Some(&FormSubmissionValue::Text("Ada Lovelace".to_owned()))
    );
    assert_eq!(
        submission.values().get("follow_up"),
        Some(&FormSubmissionValue::Checkbox(true))
    );
    assert!(!submission.values().contains_key("phone"));
}

#[test]
fn public_form_submission_accepts_empty_optional_email_and_phone_values() {
    let document = document_with_lead_form();
    let form = document
        .form(&FormId::parse("form-lead").expect("form id"))
        .expect("lead form");
    let required_fields = std::collections::BTreeMap::from([
        ("name".to_owned(), "Ada Lovelace".to_owned()),
        ("email".to_owned(), "ada@example.test".to_owned()),
        ("topic".to_owned(), "Demo".to_owned()),
    ]);

    for name in ["secondary_email", "phone"] {
        let mut submitted = required_fields.clone();
        submitted.insert(name.to_owned(), String::new());
        let result = validate_form_submission(form, &submitted, true, false)
            .expect("empty optional contact value is valid");
        assert_eq!(
            result.values().get(name),
            Some(&FormSubmissionValue::Text(String::new()))
        );
    }
}

#[test]
fn public_form_submission_validation_rejects_bad_consent_fields_and_values() {
    let document = document_with_lead_form();
    let form = document
        .form(&FormId::parse("form-lead").expect("form id"))
        .expect("lead form");
    let valid_fields = std::collections::BTreeMap::from([
        ("name".to_owned(), "Ada Lovelace".to_owned()),
        ("email".to_owned(), "ada@example.test".to_owned()),
        ("topic".to_owned(), "Demo".to_owned()),
    ]);

    assert!(validate_form_submission(form, &valid_fields, false, false).is_err());
    assert!(validate_form_submission(form, &valid_fields, true, true).is_ok());

    let mut unknown = valid_fields.clone();
    unknown.insert("unexpected".to_owned(), "value".to_owned());
    assert!(validate_form_submission(form, &unknown, true, false).is_err());

    let mut injected_option = valid_fields.clone();
    injected_option.insert("topic".to_owned(), "<script>".to_owned());
    assert!(validate_form_submission(form, &injected_option, true, false).is_err());

    let mut malformed_email = valid_fields.clone();
    malformed_email.insert("email".to_owned(), "ada at example.test".to_owned());
    assert!(validate_form_submission(form, &malformed_email, true, false).is_err());

    let mut short_name = valid_fields;
    short_name.insert("name".to_owned(), "A".to_owned());
    assert!(validate_form_submission(form, &short_name, true, false).is_err());
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

fn decode_page_metadata(
    field: &str,
    value: serde_json::Value,
) -> Result<DocumentV2, eez_domain::document_v2::DocumentError> {
    let mut document: serde_json::Value = serde_json::from_str(MINIMAL).expect("fixture JSON");
    document["pages"][0][field] = value;
    DocumentV2::decode(&serde_json::to_vec(&document).expect("JSON"), "funnel-01")
}

#[test]
fn page_metadata_defaults_and_blank_values_preserve_legacy_canonical_bytes() {
    let legacy = decode(MINIMAL);
    let page = &legacy.pages()[0];
    assert_eq!(page.head_title(), "Home");
    assert_eq!(page.seo_title(), None);
    assert_eq!(page.seo_description(), None);
    assert!(!page.noindex());
    for (field, value) in [
        ("seo_title", serde_json::json!(" \t\n\u{2003}")),
        ("seo_description", serde_json::json!("")),
        ("noindex", serde_json::json!(false)),
    ] {
        let document = decode_page_metadata(field, value).expect("default page metadata");
        assert_eq!(document.canonical_bytes(), legacy.canonical_bytes());
    }
}

#[test]
fn page_metadata_normalizes_unicode_whitespace_and_enforces_scalar_bounds() {
    for (field, maximum) in [("seo_title", 120), ("seo_description", 320)] {
        let document =
            decode_page_metadata(field, serde_json::json!("  Launch\n\t&\u{2003}learn  "))
                .expect("whitespace is normalized");
        let page = &document.pages()[0];
        let actual = if field == "seo_title" {
            page.seo_title()
        } else {
            page.seo_description()
        };
        assert_eq!(actual, Some("Launch & learn"));
        let bounded = decode_page_metadata(field, serde_json::json!("🚀".repeat(maximum)))
            .expect("Unicode scalar limit allows multibyte characters");
        let round_trip = DocumentV2::decode(bounded.canonical_bytes(), "funnel-01")
            .expect("normalized document round-trips");
        assert_eq!(round_trip, bounded);
        let error = decode_page_metadata(field, serde_json::json!("🚀".repeat(maximum + 1)))
            .expect_err("overlong metadata is rejected without truncation");
        assert_eq!(error.code(), "validation/page_metadata_length");
        assert_eq!(error.path(), Some(format!("pages.{field}").as_str()));
        for control in ['\0', '\u{001b}', '\u{007f}'] {
            let error = decode_page_metadata(field, serde_json::json!(format!("a{control}b")))
                .expect_err("non-whitespace control character");
            assert_eq!(error.code(), "validation/page_metadata_control");
        }
    }
}

#[test]
fn page_metadata_rejects_null_wrong_types_and_unknown_fields() {
    for field in ["seo_title", "seo_description", "noindex"] {
        let error = decode_page_metadata(field, serde_json::Value::Null)
            .expect_err("explicit null is not a default");
        assert_eq!(error.code(), "validation/document_invalid");
    }
    for (field, value) in [
        ("seo_title", serde_json::json!(false)),
        ("seo_description", serde_json::json!({})),
        ("noindex", serde_json::json!("false")),
        ("seo", serde_json::json!({})),
    ] {
        assert_eq!(
            decode_page_metadata(field, value)
                .expect_err("closed page wire")
                .code(),
            "validation/document_invalid"
        );
    }
}

#[test]
fn page_metadata_edits_target_one_page_preserve_siblings_and_clear_to_defaults() {
    use eez_domain::document_v2::{FocusTarget, PageId};
    let mut value: serde_json::Value = serde_json::from_str(MINIMAL).expect("fixture");
    value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
        {"id":"next-button","kind":"button","label":"Next","action":{"kind":"navigate_page","page_id":"page-next"}}
    ]);
    value["pages"]
        .as_array_mut()
        .expect("pages")
        .push(serde_json::json!({
            "id":"page-next", "slug":"next", "title":"Next", "sections":[],
            "seo_title":"Existing title", "seo_description":"Existing description", "noindex":true
        }));
    let original = decode(&serde_json::to_string(&value).expect("JSON"));
    let page = PageId::parse("page-next").expect("page ID");
    let mut ids = TestIds(0);
    let title = original
        .apply(
            Edit::SetPageSeoTitle {
                page: page.clone(),
                value: "  New\n title  ".into(),
            },
            &mut ids,
        )
        .expect("title edit");
    assert_eq!(title.focus, FocusTarget::Page(page.clone()));
    let changed = title.document.page(&page).expect("target page");
    assert_eq!(changed.seo_title(), Some("New title"));
    assert_eq!(changed.seo_description(), Some("Existing description"));
    assert!(changed.noindex());
    assert_eq!(title.document.pages()[0], original.pages()[0]);
    let description = title
        .document
        .apply(
            Edit::SetPageSeoDescription {
                page: page.clone(),
                value: "New description".into(),
            },
            &mut ids,
        )
        .expect("description edit");
    assert_eq!(description.focus, FocusTarget::Page(page.clone()));
    assert_eq!(
        description
            .document
            .page(&page)
            .expect("target")
            .head_title(),
        "New title"
    );
    let indexed = description
        .document
        .apply(
            Edit::SetPageNoIndex {
                page: page.clone(),
                noindex: false,
            },
            &mut ids,
        )
        .expect("index edit");
    assert_eq!(indexed.focus, FocusTarget::Page(page.clone()));
    let target = indexed.document.page(&page).expect("target");
    assert!(!target.noindex());
    assert_eq!(target.seo_description(), Some("New description"));
    assert_eq!(target.seo_title(), Some("New title"));
    let cleared = indexed
        .document
        .apply(
            Edit::SetPageSeoTitle {
                page: page.clone(),
                value: String::new(),
            },
            &mut ids,
        )
        .expect("clear title")
        .document
        .apply(
            Edit::SetPageSeoDescription {
                page: page.clone(),
                value: " \t ".into(),
            },
            &mut ids,
        )
        .expect("clear description");
    let target = cleared.document.page(&page).expect("target");
    assert_eq!(target.head_title(), "Next");
    assert_eq!(target.seo_title(), None);
    assert_eq!(target.seo_description(), None);
    assert_eq!(cleared.document.pages()[0], original.pages()[0]);
    for edit in [
        Edit::SetPageSeoTitle {
            page: page.clone(),
            value: "a".repeat(121),
        },
        Edit::SetPageSeoDescription {
            page: page.clone(),
            value: "a".repeat(321),
        },
        Edit::SetPageSeoTitle {
            page: page.clone(),
            value: "bad\0title".into(),
        },
        Edit::SetPageSeoDescription {
            page: page.clone(),
            value: "bad\u{1b}description".into(),
        },
        Edit::SetPageNoIndex {
            page: PageId::parse("missing").expect("ID"),
            noindex: true,
        },
    ] {
        let wire = serde_json::to_vec(&edit).expect("edit JSON");
        let decoded = serde_json::from_slice(&wire).expect("edit round-trip");
        assert!(original.apply(decoded, &mut ids).is_err());
    }
    assert_eq!(
        original.page(&page).expect("original").seo_title(),
        Some("Existing title")
    );
}

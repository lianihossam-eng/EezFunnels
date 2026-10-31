use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserializer, Serialize};
use url::Url;

use super::model::{
    is_valid_stable_id, ActionV2, BlockV2, DocumentData, DocumentError, FormField, FormResult,
    FormSubmissionError, FormSubmissionIssue, FormSubmissionIssueCode, FormSubmissionIssueTarget,
    FormSubmissionValue, FormV2, PageId, TextPresentation, ValidatedFormSubmission,
};
use super::placeholders::target_contains_text;

#[derive(Clone, Debug, Default)]
pub(crate) struct DocumentIndex {
    pub(crate) pages: BTreeMap<PageId, usize>,
    pub(crate) sections: BTreeMap<super::SectionId, (usize, usize)>,
    pub(crate) blocks: BTreeMap<super::BlockId, (usize, usize, usize)>,
    pub(crate) forms: BTreeMap<super::FormId, usize>,
    pub(crate) fields: BTreeMap<super::FieldId, (usize, usize)>,
}

pub(crate) fn check_document_depth(raw: &[u8], maximum: usize) -> Result<(), DocumentError> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in raw {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth
                    .checked_add(1)
                    .ok_or_else(|| DocumentError::new("validation/document_too_deep"))?;
                if depth > maximum {
                    return Err(DocumentError::new("validation/document_too_deep"));
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn check_unique_object_keys(raw: &[u8]) -> Result<(), DocumentError> {
    let mut deserializer = serde_json::Deserializer::from_slice(raw);
    UniqueJsonValue
        .deserialize(&mut deserializer)
        .map_err(|_| DocumentError::new("validation/document_invalid"))?;
    deserializer
        .end()
        .map_err(|_| DocumentError::new("validation/document_invalid"))
}

struct UniqueJsonValue;

impl<'de> DeserializeSeed<'de> for UniqueJsonValue {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonVisitor)
    }
}

struct UniqueJsonVisitor;

impl<'de> Visitor<'de> for UniqueJsonVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_string<E>(self, _value: String) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element_seed(UniqueJsonValue)?.is_some() {}
        Ok(())
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = HashSet::new();
        while let Some(key) = object.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate JSON object key"));
            }
            object.next_value_seed(UniqueJsonValue)?;
        }
        Ok(())
    }
}

pub(crate) fn validate_data(
    data: &mut DocumentData,
    expected_funnel_id: &str,
) -> Result<(), DocumentError> {
    if data.document_type != super::model::DOCUMENT_TYPE_V2
        || data.document_version != 2
        || data.schema_version != 2
    {
        return Err(DocumentError::new("validation/document_version_mismatch"));
    }
    if data.funnel_id.as_str() != expected_funnel_id {
        return Err(DocumentError::at(
            "validation/document_funnel_mismatch",
            "funnel_id",
        ));
    }
    validate_theme(&data.theme)?;
    if !(1..=50).contains(&data.pages.len()) {
        return Err(DocumentError::at("validation/page_count", "pages"));
    }
    if data.forms.len() > 100 {
        return Err(DocumentError::at("validation/form_count", "forms"));
    }

    let mut page_ids = BTreeMap::new();
    let mut slugs = BTreeSet::new();
    let mut section_count = 0usize;
    let mut block_count = 0usize;
    let mut section_ids = BTreeSet::new();
    let mut block_ids = BTreeSet::new();
    for (page_index, page) in data.pages.iter_mut().enumerate() {
        if page_ids.insert(page.id.clone(), page_index).is_some() {
            return Err(DocumentError::at(
                "validation/page_id_duplicate",
                "pages.id",
            ));
        }
        if page.title.is_empty() || page.title.len() > 8192 {
            return Err(DocumentError::at(
                "validation/page_title_length",
                "pages.title",
            ));
        }
        page.seo_title = normalize_page_metadata(
            page.seo_title.take(),
            super::MAX_PAGE_SEO_TITLE_SCALARS,
            "pages.seo_title",
        )?;
        page.seo_description = normalize_page_metadata(
            page.seo_description.take(),
            super::MAX_PAGE_SEO_DESCRIPTION_SCALARS,
            "pages.seo_description",
        )?;
        if page.id == data.entry_page_id {
            if !page.slug.is_empty() {
                return Err(DocumentError::at(
                    "validation/entry_page_slug",
                    "pages.slug",
                ));
            }
        } else if !is_valid_page_slug(&page.slug) {
            return Err(DocumentError::at(
                "validation/page_slug_invalid",
                "pages.slug",
            ));
        }
        if !slugs.insert(page.slug.clone()) {
            return Err(DocumentError::at(
                "validation/page_slug_duplicate",
                "pages.slug",
            ));
        }
        if page.sections.len() > 100 {
            return Err(DocumentError::at(
                "validation/section_count",
                "pages.sections",
            ));
        }
        section_count = section_count
            .checked_add(page.sections.len())
            .ok_or_else(|| DocumentError::new("validation/section_count"))?;
        for section in &mut page.sections {
            if !section_ids.insert(section.id.clone()) {
                return Err(DocumentError::at(
                    "validation/section_id_duplicate",
                    "pages.sections.id",
                ));
            }
            if section.blocks.len() > 50 {
                return Err(DocumentError::at(
                    "validation/block_count",
                    "pages.sections.blocks",
                ));
            }
            block_count = block_count
                .checked_add(section.blocks.len())
                .ok_or_else(|| DocumentError::new("validation/block_count"))?;
            for block in &mut section.blocks {
                if !block_ids.insert(block.id().clone()) {
                    return Err(DocumentError::at(
                        "validation/block_id_duplicate",
                        "pages.sections.blocks.id",
                    ));
                }
                validate_block(block)?;
            }
        }
    }
    if section_count > 100 {
        return Err(DocumentError::at(
            "validation/section_count",
            "pages.sections",
        ));
    }
    if block_count > 5000 {
        return Err(DocumentError::at(
            "validation/block_count",
            "pages.sections.blocks",
        ));
    }
    if !page_ids.contains_key(&data.entry_page_id) {
        return Err(DocumentError::at(
            "validation/entry_page_missing",
            "entry_page_id",
        ));
    }

    validate_forms_and_graph(data, &page_ids)?;
    validate_generated_placeholders(data)?;
    Ok(())
}

fn validate_generated_placeholders(data: &DocumentData) -> Result<(), DocumentError> {
    let mut seen = BTreeSet::new();
    for marker in &data.generated_placeholders {
        if marker.expected_text.is_empty() || marker.expected_text.len() > 8192 {
            return Err(DocumentError::at(
                "validation/generated_placeholder_invalid",
                "generated_placeholders",
            ));
        }
        let is_targeted_text = target_contains_text(data, &marker.target, &marker.expected_text);
        if !is_targeted_text || !seen.insert(marker.clone()) {
            return Err(DocumentError::at(
                "validation/generated_placeholder_invalid",
                "generated_placeholders",
            ));
        }
    }
    Ok(())
}

pub(crate) fn build_index(data: &DocumentData) -> Result<DocumentIndex, DocumentError> {
    let mut index = DocumentIndex::default();
    for (page_index, page) in data.pages.iter().enumerate() {
        index.pages.insert(page.id.clone(), page_index);
        for (section_index, section) in page.sections.iter().enumerate() {
            index
                .sections
                .insert(section.id.clone(), (page_index, section_index));
            for (block_index, block) in section.blocks.iter().enumerate() {
                index
                    .blocks
                    .insert(block.id().clone(), (page_index, section_index, block_index));
            }
        }
    }
    for (form_index, form) in data.forms.iter().enumerate() {
        index.forms.insert(form.id.clone(), form_index);
        for (field_index, field) in form.fields.iter().enumerate() {
            index
                .fields
                .insert(field.id().clone(), (form_index, field_index));
        }
    }
    Ok(index)
}

pub(crate) fn canonicalize<T: Serialize>(value: &T) -> Result<Vec<u8>, DocumentError> {
    serde_json_canonicalizer::to_vec(value)
        .map_err(|_| DocumentError::new("validation/document_canonicalization_failed"))
}

fn validate_theme(theme: &super::ThemeV2) -> Result<(), DocumentError> {
    validate_overrides(&theme.overrides)?;
    if let Some(responsive) = &theme.overrides.responsive {
        if let Some(tablet) = &responsive.tablet {
            validate_partial_overrides(tablet)?;
        }
        if let Some(mobile) = &responsive.mobile {
            validate_partial_overrides(mobile)?;
        }
    }
    Ok(())
}

fn validate_overrides(theme: &super::ThemeOverrides) -> Result<(), DocumentError> {
    validate_colors(theme.colors.as_ref())
}

fn validate_partial_overrides(theme: &super::ThemePartialOverrides) -> Result<(), DocumentError> {
    validate_colors(theme.colors.as_ref())
}

fn validate_colors(colors: Option<&super::ThemeColors>) -> Result<(), DocumentError> {
    if let Some(colors) = colors {
        for color in [
            colors.color_background.as_deref(),
            colors.color_border.as_deref(),
            colors.color_muted.as_deref(),
            colors.color_on_primary.as_deref(),
            colors.color_primary.as_deref(),
            colors.color_surface.as_deref(),
            colors.color_text.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if !is_hex_color(color) {
                return Err(DocumentError::at(
                    "validation/theme_color_invalid",
                    "theme.overrides.colors",
                ));
            }
        }
    }
    Ok(())
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
}

fn validate_block(block: &mut BlockV2) -> Result<(), DocumentError> {
    match block {
        BlockV2::Heading { level, text, .. } => {
            if !(1..=6).contains(level) {
                return Err(DocumentError::at(
                    "validation/heading_level",
                    "blocks.level",
                ));
            }
            validate_nonempty_text(text, 8192, "blocks.text")?;
        }
        BlockV2::Text { text, .. } => validate_text(text, 8192, "blocks.text")?,
        BlockV2::List { items, .. } => {
            if !(1..=50).contains(&items.len()) {
                return Err(DocumentError::at(
                    "validation/list_item_count",
                    "blocks.items",
                ));
            }
            for item in items {
                validate_nonempty_text(item, 8192, "blocks.items")?;
            }
        }
        BlockV2::Button { label, action, .. } => {
            validate_nonempty_text(label, 8192, "blocks.label")?;
            match action {
                ActionV2::ExternalHttps { url } => *url = canonical_https_url(url)?,
                ActionV2::Telephone { number } => {
                    *number = canonical_action_phone(number)?;
                }
                ActionV2::Email { address } if !valid_submitted_email(address) => {
                    return Err(DocumentError::at(
                        "validation/action_email_invalid",
                        "blocks.action.address",
                    ));
                }
                ActionV2::NavigatePage { .. }
                | ActionV2::NavigateSection { .. }
                | ActionV2::Email { .. } => {}
            }
        }
        BlockV2::Image { alt, .. } => {
            if let super::ImageAlt::Text { value } = alt {
                validate_nonempty_text(value, 8192, "blocks.alt.value")?;
            }
        }
        BlockV2::Form { .. } => {}
    }
    Ok(())
}

fn validate_forms_and_graph(
    data: &mut DocumentData,
    pages: &BTreeMap<PageId, usize>,
) -> Result<(), DocumentError> {
    let mut forms = BTreeMap::new();
    let mut field_ids = BTreeSet::new();
    for (form_index, form) in data.forms.iter_mut().enumerate() {
        if forms.insert(form.id.clone(), form_index).is_some() {
            return Err(DocumentError::at(
                "validation/form_id_duplicate",
                "forms.id",
            ));
        }
        if !(1..=50).contains(&form.fields.len()) {
            return Err(DocumentError::at(
                "validation/form_field_count",
                "forms.fields",
            ));
        }
        validate_nonempty_text(&form.purpose, 1024, "forms.purpose")?;
        if let Some(presentation) = &mut form.presentation {
            if presentation.submit_label.is_none() && presentation.information_url.is_none() {
                return Err(DocumentError::at(
                    "validation/form_presentation_empty",
                    "forms.presentation",
                ));
            }
            if let Some(label) = &presentation.submit_label {
                if label.trim().is_empty() {
                    return Err(DocumentError::at(
                        "validation/form_submit_label_empty",
                        "forms.presentation.submit_label",
                    ));
                }
                validate_nonempty_text(label, 120, "forms.presentation.submit_label")?;
            }
            if let Some(url) = &mut presentation.information_url {
                *url = canonical_information_url(url)?;
            }
        }
        validate_consent_copy(
            form.request_consent.copy_version.as_str(),
            &form.request_consent.copy,
        )?;
        if let Some(consent) = &form.marketing_opt_in {
            validate_consent_copy(consent.copy_version.as_str(), &consent.copy)?;
        }
        match &form.result {
            FormResult::Inline { message } => {
                validate_nonempty_text(message, 1024, "forms.result.message")?
            }
            FormResult::SuccessPage { page_id } if !pages.contains_key(page_id) => {
                return Err(DocumentError::at(
                    "validation/form_result_page_missing",
                    "forms.result.page_id",
                ));
            }
            FormResult::SuccessPage { .. } => {}
        }

        let mut names = BTreeSet::new();
        for field in &form.fields {
            if !field_ids.insert(field.id().clone()) {
                return Err(DocumentError::at(
                    "validation/field_id_duplicate",
                    "forms.fields.id",
                ));
            }
            let (name, label, help, validation, options) = field.values();
            if !valid_field_name(name) {
                return Err(DocumentError::at(
                    "validation/form_field_name",
                    "forms.fields.name",
                ));
            }
            if name == "request_consent" || name == "marketing_opt_in" {
                return Err(DocumentError::at(
                    "validation/form_reserved_field_name",
                    "forms.fields.name",
                ));
            }
            if !names.insert(name) {
                return Err(DocumentError::at(
                    "validation/form_field_name_duplicate",
                    "forms.fields.name",
                ));
            }
            validate_nonempty_text(label, 1024, "forms.fields.label")?;
            if let Some(help) = help {
                validate_nonempty_text(help, 1024, "forms.fields.help")?;
            }
            if let Some(options) = options {
                if !(1..=50).contains(&options.len()) {
                    return Err(DocumentError::at(
                        "validation/form_select_options",
                        "forms.fields.options",
                    ));
                }
                let mut unique = BTreeSet::new();
                for option in options {
                    validate_nonempty_text(option, 1024, "forms.fields.options")?;
                    if !unique.insert(option) {
                        return Err(DocumentError::at(
                            "validation/form_select_option_duplicate",
                            "forms.fields.options",
                        ));
                    }
                }
            }
            if let Some(validation) = validation {
                let cap = field.max_value_length();
                if validation.min_length.is_some_and(|value| value > cap)
                    || validation
                        .max_length
                        .is_some_and(|value| value == 0 || value > cap)
                    || validation
                        .min_length
                        .zip(validation.max_length)
                        .is_some_and(|(minimum, maximum)| minimum > maximum)
                {
                    return Err(DocumentError::at(
                        "validation/form_field_validation",
                        "forms.fields.validation",
                    ));
                }
            }
        }
    }

    let mut graph = vec![BTreeSet::new(); data.pages.len()];
    let mut placed_forms = BTreeSet::new();
    for (source_index, page) in data.pages.iter().enumerate() {
        let mut page_forms = BTreeSet::new();
        let page_sections = page
            .sections
            .iter()
            .map(|section| section.id.clone())
            .collect::<BTreeSet<_>>();
        for section in &page.sections {
            for block in &section.blocks {
                match block {
                    BlockV2::Button {
                        action: ActionV2::NavigatePage { page_id },
                        ..
                    } => {
                        let Some(target_index) = pages.get(page_id) else {
                            return Err(DocumentError::at(
                                "validation/action_page_missing",
                                "blocks.action.page_id",
                            ));
                        };
                        graph[source_index].insert(*target_index);
                    }
                    BlockV2::Button {
                        action: ActionV2::NavigateSection { section_id },
                        ..
                    } if !page_sections.contains(section_id) => {
                        return Err(DocumentError::at(
                            "validation/action_section_missing",
                            "blocks.action.section_id",
                        ));
                    }
                    BlockV2::Form { form_id, .. } => {
                        let Some(form_index) = forms.get(form_id) else {
                            return Err(DocumentError::at(
                                "validation/form_missing",
                                "blocks.form_id",
                            ));
                        };
                        if !page_forms.insert(form_id.clone()) {
                            return Err(DocumentError::at(
                                "validation/form_duplicate_on_page",
                                "pages.sections.blocks.form_id",
                            ));
                        }
                        placed_forms.insert(form_id.clone());
                        if let FormResult::SuccessPage { page_id } = &data.forms[*form_index].result
                        {
                            let Some(target_index) = pages.get(page_id) else {
                                return Err(DocumentError::at(
                                    "validation/form_result_page_missing",
                                    "forms.result.page_id",
                                ));
                            };
                            graph[source_index].insert(*target_index);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if placed_forms.len() != data.forms.len() {
        return Err(DocumentError::at("validation/form_unplaced", "forms"));
    }

    let entry_index = pages
        .get(&data.entry_page_id)
        .copied()
        .ok_or_else(|| DocumentError::at("validation/entry_page_missing", "entry_page_id"))?;
    let mut reachable = BTreeSet::new();
    let mut stack = vec![entry_index];
    while let Some(page_index) = stack.pop() {
        if reachable.insert(page_index) {
            stack.extend(graph[page_index].iter().copied());
        }
    }
    if reachable.len() != data.pages.len() {
        return Err(DocumentError::at("validation/page_unreachable", "pages"));
    }

    let mut indegree = vec![0usize; data.pages.len()];
    for targets in &graph {
        for target in targets {
            indegree[*target] = indegree[*target]
                .checked_add(1)
                .ok_or_else(|| DocumentError::new("validation/page_graph_invalid"))?;
        }
    }
    let mut queue = VecDeque::new();
    for (index, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            queue.push_back(index);
        }
    }
    let mut visited = 0usize;
    while let Some(page_index) = queue.pop_front() {
        visited += 1;
        for target in &graph[page_index] {
            indegree[*target] -= 1;
            if indegree[*target] == 0 {
                queue.push_back(*target);
            }
        }
    }
    if visited != data.pages.len() {
        return Err(DocumentError::at("validation/page_graph_cycle", "pages"));
    }
    Ok(())
}

fn normalize_page_metadata(
    value: Option<String>,
    maximum: usize,
    path: &'static str,
) -> Result<Option<String>, DocumentError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value
        .chars()
        .any(|character| character.is_control() && !character.is_whitespace())
    {
        return Err(DocumentError::at("validation/page_metadata_control", path));
    }
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() > maximum {
        return Err(DocumentError::at("validation/page_metadata_length", path));
    }
    Ok((!normalized.is_empty()).then_some(normalized))
}

fn validate_consent_copy(version: &str, copy: &str) -> Result<(), DocumentError> {
    if !is_valid_stable_id(version) {
        return Err(DocumentError::at(
            "validation/form_copy_version",
            "forms.copy_version",
        ));
    }
    validate_nonempty_text(copy, 1024, "forms.copy")
}

fn validate_block_text(
    value: &str,
    maximum: usize,
    path: &'static str,
) -> Result<(), DocumentError> {
    if value.len() > maximum {
        return Err(DocumentError::at("validation/text_length", path));
    }
    Ok(())
}

fn validate_text(value: &str, maximum: usize, path: &'static str) -> Result<(), DocumentError> {
    validate_block_text(value, maximum, path)
}

fn validate_nonempty_text(
    value: &str,
    maximum: usize,
    path: &'static str,
) -> Result<(), DocumentError> {
    if value.is_empty() {
        return Err(DocumentError::at("validation/text_empty", path));
    }
    validate_block_text(value, maximum, path)
}

pub fn is_valid_page_slug(value: &str) -> bool {
    if value.is_empty() || value.len() > 64 || !value.is_ascii() {
        return false;
    }
    let mut previous_hyphen = true;
    for byte in value.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => previous_hyphen = false,
            b'-' if !previous_hyphen => previous_hyphen = true,
            _ => return false,
        }
    }
    !previous_hyphen
}

pub fn validate_form_submission(
    form: &FormV2,
    submitted_fields: &BTreeMap<String, String>,
    request_consent_accepted: bool,
    marketing_opt_in: bool,
) -> Result<ValidatedFormSubmission, FormSubmissionError> {
    let mut issues = Vec::new();
    if !request_consent_accepted {
        issues.push(FormSubmissionIssue {
            target: FormSubmissionIssueTarget::RequestConsent,
            code: FormSubmissionIssueCode::RequestConsentRequired,
        });
    }
    if marketing_opt_in && form.marketing_opt_in.is_none() {
        issues.push(FormSubmissionIssue {
            target: FormSubmissionIssueTarget::Form,
            code: FormSubmissionIssueCode::MarketingConsentUnavailable,
        });
    }

    if submitted_fields
        .keys()
        .any(|name| !form.fields.iter().any(|field| field.values().0 == name))
    {
        issues.push(FormSubmissionIssue {
            target: FormSubmissionIssueTarget::Form,
            code: FormSubmissionIssueCode::UnknownField,
        });
    }

    let mut values = BTreeMap::new();
    let mut recoverable_values = BTreeMap::new();
    for field in &form.fields {
        let (name, _, _, validation, options) = field.values();
        match field {
            FormField::Text { required, .. }
            | FormField::Email { required, .. }
            | FormField::Phone { required, .. } => {
                let Some(value) = submitted_fields.get(name) else {
                    if *required {
                        push_field_submission_issue(
                            &mut issues,
                            field,
                            FormSubmissionIssueCode::Required,
                        );
                    }
                    continue;
                };
                let multiline = matches!(
                    field,
                    FormField::Text {
                        presentation: TextPresentation::Multiline,
                        ..
                    }
                );
                let normalized_value = normalize_form_text_value(value, multiline);
                if let Some(normalized) = normalized_value.as_ref().filter(|normalized| {
                    normalized.chars().count() <= field.max_value_length() as usize
                }) {
                    recoverable_values.insert(name.to_owned(), normalized.clone());
                }
                if *required
                    && normalized_value
                        .as_deref()
                        .unwrap_or(value)
                        .trim()
                        .is_empty()
                {
                    push_field_submission_issue(
                        &mut issues,
                        field,
                        FormSubmissionIssueCode::Required,
                    );
                    continue;
                }
                let Some(normalized_value) = normalized_value else {
                    push_field_submission_issue(
                        &mut issues,
                        field,
                        FormSubmissionIssueCode::InvalidValue,
                    );
                    continue;
                };
                if !valid_form_text_value(
                    &normalized_value,
                    *required,
                    validation,
                    field.max_value_length(),
                    multiline,
                ) {
                    push_field_submission_issue(
                        &mut issues,
                        field,
                        FormSubmissionIssueCode::InvalidValue,
                    );
                    continue;
                }
                if !normalized_value.is_empty() {
                    if matches!(field, FormField::Email { .. })
                        && !valid_submitted_email(&normalized_value)
                    {
                        push_field_submission_issue(
                            &mut issues,
                            field,
                            FormSubmissionIssueCode::InvalidValue,
                        );
                        continue;
                    }
                    if matches!(field, FormField::Phone { .. })
                        && !valid_submitted_phone(&normalized_value)
                    {
                        push_field_submission_issue(
                            &mut issues,
                            field,
                            FormSubmissionIssueCode::InvalidValue,
                        );
                        continue;
                    }
                }
                values.insert(name.to_owned(), FormSubmissionValue::Text(normalized_value));
            }
            FormField::Checkbox { required, .. } => {
                let checked = match submitted_fields.get(name).map(String::as_str) {
                    None => false,
                    Some("on") => true,
                    Some(_) => {
                        push_field_submission_issue(
                            &mut issues,
                            field,
                            FormSubmissionIssueCode::InvalidValue,
                        );
                        continue;
                    }
                };
                if checked {
                    recoverable_values.insert(name.to_owned(), "on".to_owned());
                }
                if *required && !checked {
                    push_field_submission_issue(
                        &mut issues,
                        field,
                        FormSubmissionIssueCode::Required,
                    );
                    continue;
                }
                values.insert(name.to_owned(), FormSubmissionValue::Checkbox(checked));
            }
            FormField::Select { required, .. } => {
                let selected = submitted_fields.get(name).map(String::as_str);
                let Some(selected) = selected else {
                    if *required {
                        push_field_submission_issue(
                            &mut issues,
                            field,
                            FormSubmissionIssueCode::Required,
                        );
                    }
                    continue;
                };
                if !selected.is_empty()
                    && !options.is_some_and(|allowed| allowed.contains(&selected.to_owned()))
                {
                    push_field_submission_issue(
                        &mut issues,
                        field,
                        FormSubmissionIssueCode::InvalidChoice,
                    );
                    continue;
                }
                if *required && selected.is_empty() {
                    push_field_submission_issue(
                        &mut issues,
                        field,
                        FormSubmissionIssueCode::Required,
                    );
                    continue;
                }
                recoverable_values.insert(name.to_owned(), selected.to_owned());
                values.insert(
                    name.to_owned(),
                    FormSubmissionValue::Select(selected.to_owned()),
                );
            }
        }
    }

    if !issues.is_empty() {
        return Err(FormSubmissionError {
            issues,
            recoverable_values,
            request_consent_accepted,
            marketing_opt_in,
        });
    }

    Ok(ValidatedFormSubmission {
        values,
        marketing_opt_in,
    })
}

fn push_field_submission_issue(
    issues: &mut Vec<FormSubmissionIssue>,
    field: &FormField,
    code: FormSubmissionIssueCode,
) {
    issues.push(FormSubmissionIssue {
        target: FormSubmissionIssueTarget::Field(field.id().as_str().to_owned()),
        code,
    });
}

fn valid_form_text_value(
    value: &str,
    required: bool,
    validation: Option<&super::ValidationRange>,
    maximum: u32,
    allow_line_feed: bool,
) -> bool {
    if value
        .chars()
        .any(|character| character.is_control() && !(allow_line_feed && character == '\n'))
    {
        return false;
    }
    if required && value.trim().is_empty() {
        return false;
    }
    if value.is_empty() && !required {
        return true;
    }
    let length = value.chars().count();
    if length > maximum as usize {
        return false;
    }
    validation.is_none_or(|range| {
        range
            .min_length
            .is_none_or(|minimum| length >= minimum as usize)
            && range
                .max_length
                .is_none_or(|maximum| length <= maximum as usize)
    })
}

fn normalize_form_text_value(value: &str, multiline: bool) -> Option<String> {
    let normalized = if multiline {
        value.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        value.to_owned()
    };
    (!normalized
        .chars()
        .any(|character| character.is_control() && !(multiline && character == '\n')))
    .then_some(normalized)
}

fn valid_submitted_email(value: &str) -> bool {
    let mut parts = value.split('@');
    let Some(local) = parts.next() else {
        return false;
    };
    let Some(domain) = parts.next() else {
        return false;
    };
    if parts.next().is_some()
        || local.is_empty()
        || local.len() > 64
        || domain.is_empty()
        || value.len() > 320
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || value.chars().any(char::is_whitespace)
        || !local.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'.' | b'!'
                        | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'/'
                        | b'='
                        | b'?'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'{'
                        | b'|'
                        | b'}'
                        | b'~'
                )
        })
    {
        return false;
    }

    let labels = domain.split('.').collect::<Vec<_>>();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn valid_submitted_phone(value: &str) -> bool {
    let digits = value.bytes().filter(u8::is_ascii_digit).count();
    digits >= 3
        && value.chars().all(|character| {
            character.is_ascii_digit() || matches!(character, '+' | '-' | '(' | ')' | '.' | ' ')
        })
}

fn canonical_action_phone(value: &str) -> Result<String, DocumentError> {
    let value = value.trim();
    if value.len() > 64 {
        return Err(DocumentError::at(
            "validation/action_phone_invalid",
            "blocks.action.number",
        ));
    }

    let mut canonical = String::with_capacity(value.len());
    let mut digit_count = 0usize;
    let mut in_parentheses = false;
    let mut parenthetical_digit_count = 0usize;
    for (index, character) in value.chars().enumerate() {
        match character {
            '+' if index == 0 => canonical.push('+'),
            digit if digit.is_ascii_digit() => {
                digit_count += 1;
                if in_parentheses {
                    parenthetical_digit_count += 1;
                }
                canonical.push(digit);
            }
            '(' if !in_parentheses => {
                in_parentheses = true;
                parenthetical_digit_count = 0;
            }
            ')' if in_parentheses && parenthetical_digit_count > 0 => {
                in_parentheses = false;
            }
            ' ' | '-' | '.' => {}
            _ => {
                return Err(DocumentError::at(
                    "validation/action_phone_invalid",
                    "blocks.action.number",
                ));
            }
        }
    }
    if in_parentheses || !(3..=15).contains(&digit_count) {
        return Err(DocumentError::at(
            "validation/action_phone_invalid",
            "blocks.action.number",
        ));
    }
    Ok(canonical)
}

fn valid_field_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && value.len() <= 64
}

fn canonical_https_url(value: &str) -> Result<String, DocumentError> {
    canonical_https_url_at(value, "blocks.action.url")
}

pub fn canonical_information_url(value: &str) -> Result<String, DocumentError> {
    canonical_https_url_at(value, "forms.presentation.information_url")
}

fn canonical_https_url_at(value: &str, path: &str) -> Result<String, DocumentError> {
    if value.len() > 2048 {
        return Err(DocumentError::at("validation/url_invalid", path));
    }
    let bytes = value.as_bytes();
    if value.chars().any(|character| {
        character.is_control()
            || character.is_whitespace()
            || matches!(character, '"' | '\'' | '<' | '>' | '\\')
    }) {
        return Err(DocumentError::at("validation/url_invalid", path));
    }
    if !has_https_scheme_and_authority(value) || !valid_percent_escapes(bytes) {
        return Err(DocumentError::at("validation/url_invalid", path));
    }
    let parsed =
        Url::parse(value).map_err(|_| DocumentError::at("validation/url_invalid", path))?;
    let authority =
        raw_authority(value).ok_or_else(|| DocumentError::at("validation/url_invalid", path))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none_or(str::is_empty)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || authority.contains('@')
    {
        return Err(DocumentError::at("validation/url_invalid", path));
    }
    Ok(parsed.to_string())
}

fn has_https_scheme_and_authority(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };
    scheme.eq_ignore_ascii_case("https")
        && raw_authority_from_rest(rest).is_some_and(|authority| !authority.is_empty())
}

fn raw_authority(value: &str) -> Option<&str> {
    let (_, rest) = value.split_once("://")?;
    raw_authority_from_rest(rest)
}

fn raw_authority_from_rest(rest: &str) -> Option<&str> {
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(&rest[..end])
}

fn valid_percent_escapes(bytes: &[u8]) -> bool {
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        let Some(pair) = bytes.get(index + 1..index + 3) else {
            return false;
        };
        if !pair[0].is_ascii_hexdigit() || !pair[1].is_ascii_hexdigit() {
            return false;
        }
        let value = (hex_value(pair[0]) << 4) | hex_value(pair[1]);
        if value <= 0x20 || matches!(value, 0x7f | 0x22 | 0x27 | 0x3c | 0x3e | 0x5c) {
            return false;
        }
        index += 3;
    }
    true
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

trait FormFieldValidation {
    fn values(
        &self,
    ) -> (
        &str,
        &str,
        Option<&str>,
        Option<&super::ValidationRange>,
        Option<&[String]>,
    );
    fn max_value_length(&self) -> u32;
}

impl FormFieldValidation for FormField {
    fn values(
        &self,
    ) -> (
        &str,
        &str,
        Option<&str>,
        Option<&super::ValidationRange>,
        Option<&[String]>,
    ) {
        match self {
            FormField::Text {
                name,
                label,
                help,
                validation,
                ..
            }
            | FormField::Email {
                name,
                label,
                help,
                validation,
                ..
            }
            | FormField::Phone {
                name,
                label,
                help,
                validation,
                ..
            } => (name, label, help.as_deref(), validation.as_ref(), None),
            FormField::Checkbox {
                name, label, help, ..
            } => (name, label, help.as_deref(), None, None),
            FormField::Select {
                name,
                label,
                help,
                options,
                ..
            } => (name, label, help.as_deref(), None, Some(options)),
        }
    }

    fn max_value_length(&self) -> u32 {
        match self {
            FormField::Text { .. } => 8192,
            FormField::Email { .. } => 320,
            FormField::Phone { .. } => 64,
            FormField::Checkbox { .. } | FormField::Select { .. } => 0,
        }
    }
}

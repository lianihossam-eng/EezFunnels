use super::section_label;
#[cfg(feature = "browser")]
use super::{
    api_request, clear_idempotency_storage, error_message, prepare_idempotency_key,
    PreparedIdempotencyKey,
};
use dioxus::prelude::*;
use dioxus_dnd::prelude::{SortEvent, SortableList};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

mod inspector;
#[cfg(feature = "browser")]
use inspector::EditorPreviewResponse;
#[cfg(test)]
use inspector::{color_control_value, text_setting_value, EditorSettingField, EditorSettingOption};
use inspector::{
    setting_changes, EditorInspector, EditorSettingChange, EditorSettingValue, EditorSettingsSchema,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct EditorSection {
    id: String,
    #[serde(rename = "type")]
    section_type: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct EditorFunnel {
    id: String,
    name: String,
    slug: String,
    updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct EditorReadResponse {
    funnel: EditorFunnel,
    base_revision_id: String,
    published_revision_id: Option<String>,
    sections: Vec<EditorSection>,
    settings_schema: EditorSettingsSchema,
    settings_values: BTreeMap<String, EditorSettingValue>,
    preview_html: String,
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum PublicationAction {
    Publish {
        revision_id: String,
        expected_published_revision_id: Option<String>,
    },
    Unpublish {
        expected_published_revision_id: String,
    },
}

#[cfg(any(feature = "browser", test))]
impl PublicationAction {
    fn path(&self, funnel_id: &str) -> String {
        let action = match self {
            Self::Publish { .. } => "publish",
            Self::Unpublish { .. } => "unpublish",
        };
        format!("/api/funnels/{funnel_id}/{action}")
    }

    fn body(&self) -> String {
        let value = match self {
            Self::Publish {
                revision_id,
                expected_published_revision_id,
            } => serde_json::json!({
                "revision_id": revision_id,
                "expected_published_revision_id": expected_published_revision_id,
            }),
            Self::Unpublish {
                expected_published_revision_id,
            } => serde_json::json!({
                "expected_published_revision_id": expected_published_revision_id,
            }),
        };
        value.to_string()
    }

    fn resulting_revision_id(&self) -> Option<&str> {
        match self {
            Self::Publish { revision_id, .. } => Some(revision_id),
            Self::Unpublish { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PublicationState {
    known: bool,
    live_revision_id: Option<String>,
    pending: bool,
    retryable: bool,
    #[cfg(any(feature = "browser", test))]
    attempt: Option<PublicationAction>,
    refreshing: bool,
    conflict: bool,
    status: Option<String>,
    error: Option<String>,
}

impl PublicationState {
    fn is_publish_attempt(&self) -> bool {
        #[cfg(any(feature = "browser", test))]
        {
            matches!(self.attempt, Some(PublicationAction::Publish { .. }))
        }
        #[cfg(not(any(feature = "browser", test)))]
        {
            false
        }
    }

    fn is_unpublish_attempt(&self) -> bool {
        #[cfg(any(feature = "browser", test))]
        {
            matches!(self.attempt, Some(PublicationAction::Unpublish { .. }))
        }
        #[cfg(not(any(feature = "browser", test)))]
        {
            false
        }
    }
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicationCapture {
    route_epoch: u64,
    funnel_id: String,
    action: PublicationAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorDocument {
    funnel: EditorFunnel,
    base_revision_id: String,
    sections: Vec<EditorSection>,
    saved_order: Vec<String>,
    order: Vec<String>,
    settings_schema: EditorSettingsSchema,
    saved_settings_values: BTreeMap<String, EditorSettingValue>,
    settings_values: BTreeMap<String, EditorSettingValue>,
    settings_changes: BTreeMap<String, EditorSettingValue>,
    selected_section: Option<String>,
    preview_html: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorPreviewCapture {
    generation: u64,
    funnel_id: String,
    base_revision_id: String,
    section_ids: Vec<String>,
    settings_changes: Vec<EditorSettingChange>,
}

const EDITOR_HISTORY_LIMIT: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorDraftSnapshot {
    order: Vec<String>,
    settings_changes: BTreeMap<String, EditorSettingValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EditorJsonPath {
    Order,
    SettingChange(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorJsonChange {
    path: EditorJsonPath,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct EditorJsonDiff {
    moved_section_id: Option<String>,
    changes: Vec<EditorJsonChange>,
}

impl EditorJsonDiff {
    fn between(
        before: &EditorDraftSnapshot,
        after: &EditorDraftSnapshot,
        moved_section_id: Option<String>,
    ) -> Option<Self> {
        let mut changes = Vec::new();
        if before.order != after.order {
            changes.push(EditorJsonChange {
                path: EditorJsonPath::Order,
                before: Some(serde_json::to_value(&before.order).ok()?),
                after: Some(serde_json::to_value(&after.order).ok()?),
            });
        }

        let setting_ids = before
            .settings_changes
            .keys()
            .chain(after.settings_changes.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        for setting_id in setting_ids {
            let before_value = before
                .settings_changes
                .get(&setting_id)
                .map(serde_json::to_value)
                .transpose()
                .ok()?;
            let after_value = after
                .settings_changes
                .get(&setting_id)
                .map(serde_json::to_value)
                .transpose()
                .ok()?;
            if before_value != after_value {
                changes.push(EditorJsonChange {
                    path: EditorJsonPath::SettingChange(setting_id),
                    before: before_value,
                    after: after_value,
                });
            }
        }
        Some(Self {
            moved_section_id,
            changes,
        })
    }

    fn apply(&self, snapshot: &EditorDraftSnapshot, forward: bool) -> Option<EditorDraftSnapshot> {
        let mut next = snapshot.clone();
        for change in &self.changes {
            let (expected, replacement) = if forward {
                (&change.before, &change.after)
            } else {
                (&change.after, &change.before)
            };
            match &change.path {
                EditorJsonPath::Order => {
                    let current = Some(serde_json::to_value(&next.order).ok()?);
                    if &current != expected {
                        return None;
                    }
                    next.order = serde_json::from_value(replacement.clone()?).ok()?;
                }
                EditorJsonPath::SettingChange(setting_id) => {
                    let current = next
                        .settings_changes
                        .get(setting_id)
                        .map(serde_json::to_value)
                        .transpose()
                        .ok()?;
                    if &current != expected {
                        return None;
                    }
                    match replacement {
                        Some(value) => {
                            let setting = serde_json::from_value(value.clone()).ok()?;
                            next.settings_changes.insert(setting_id.clone(), setting);
                        }
                        None => {
                            next.settings_changes.remove(setting_id);
                        }
                    }
                }
            }
        }
        Some(next)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct EditorHistory {
    entries: Vec<EditorJsonDiff>,
    cursor: usize,
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorSaveCapture {
    route_epoch: u64,
    generation: u64,
    funnel_id: String,
    base_revision_id: String,
    section_ids: Vec<String>,
    settings_changes: Vec<EditorSettingChange>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct EditorState {
    funnel_id: String,
    #[cfg(any(feature = "browser", test))]
    route_epoch: u64,
    loading: bool,
    load_error: bool,
    document: Option<EditorDocument>,
    publication: PublicationState,
    generation: u64,
    preview_error: bool,
    keyboard_section: Option<String>,
    keyboard_original_order: Option<Vec<String>>,
    history: EditorHistory,
    announcement: String,
    announcement_revision: u64,
    saving: bool,
    retryable: bool,
    stale: bool,
    save_error: Option<String>,
}

impl EditorState {
    fn for_route(funnel_id: String) -> Self {
        Self {
            funnel_id,
            loading: true,
            #[cfg(any(feature = "browser", test))]
            route_epoch: 0,
            ..Self::default()
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_route(&mut self, funnel_id: String) -> u64 {
        let route_epoch = self.route_epoch.wrapping_add(1);
        *self = Self::for_route(funnel_id);
        self.route_epoch = route_epoch;
        route_epoch
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_load(
        &mut self,
        route_epoch: u64,
        funnel_id: &str,
        response: Result<EditorReadResponse, ()>,
    ) {
        if self.route_epoch != route_epoch || self.funnel_id != funnel_id {
            return;
        }
        self.loading = false;
        self.reset_history();
        match response {
            Ok(response) if response.funnel.id == funnel_id => {
                self.publication = PublicationState {
                    known: true,
                    live_revision_id: response.published_revision_id.clone(),
                    ..PublicationState::default()
                };
                let order = response
                    .sections
                    .iter()
                    .map(|section| section.id.clone())
                    .collect::<Vec<_>>();
                let selected_section = order.first().cloned();
                self.document = Some(EditorDocument {
                    funnel: response.funnel,
                    base_revision_id: response.base_revision_id,
                    sections: response.sections,
                    saved_order: order.clone(),
                    order,
                    settings_schema: response.settings_schema,
                    saved_settings_values: response.settings_values.clone(),
                    settings_values: response.settings_values,
                    settings_changes: BTreeMap::new(),
                    selected_section,
                    preview_html: response.preview_html,
                });
                self.load_error = false;
            }
            _ => self.load_error = true,
        }
    }

    fn has_unsaved_changes(&self) -> bool {
        self.document.as_ref().is_some_and(|document| {
            document.order != document.saved_order || !document.settings_changes.is_empty()
        })
    }

    fn can_publish(&self) -> bool {
        if self.publication.pending || self.publication.refreshing || self.saving {
            return false;
        }
        if self.publication.retryable {
            #[cfg(any(feature = "browser", test))]
            {
                return self.document.as_ref().is_some_and(|document| {
                    matches!(
                        self.publication.attempt,
                        Some(PublicationAction::Publish { ref revision_id, .. })
                            if revision_id == &document.base_revision_id
                    )
                });
            }
            #[cfg(not(any(feature = "browser", test)))]
            return false;
        }
        if !self.publication.known
            || self.retryable
            || self.stale
            || self.has_unsaved_changes()
            || self.keyboard_section.is_some()
            || self.keyboard_original_order.is_some()
        {
            return false;
        }
        let Some(document) = self.document.as_ref() else {
            return false;
        };
        self.publication.live_revision_id.as_deref() != Some(&document.base_revision_id)
    }

    fn can_unpublish(&self) -> bool {
        if !self.publication.known || self.publication.pending || self.publication.refreshing {
            return false;
        }
        if self.publication.retryable {
            #[cfg(any(feature = "browser", test))]
            return matches!(
                self.publication.attempt,
                Some(PublicationAction::Unpublish { .. })
            );
            #[cfg(not(any(feature = "browser", test)))]
            return false;
        }
        self.publication.live_revision_id.is_some()
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_publish(&mut self) -> Option<PublicationCapture> {
        if !self.can_publish() {
            return None;
        }
        let action = match self.publication.attempt.clone() {
            Some(action @ PublicationAction::Publish { .. }) if self.publication.retryable => {
                action
            }
            _ => {
                let document = self.document.as_ref()?;
                PublicationAction::Publish {
                    revision_id: document.base_revision_id.clone(),
                    expected_published_revision_id: self.publication.live_revision_id.clone(),
                }
            }
        };
        Some(self.begin_publication(action))
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_unpublish(&mut self) -> Option<PublicationCapture> {
        if !self.can_unpublish() {
            return None;
        }
        let action = match self.publication.attempt.clone() {
            Some(action @ PublicationAction::Unpublish { .. }) if self.publication.retryable => {
                action
            }
            _ => PublicationAction::Unpublish {
                expected_published_revision_id: self.publication.live_revision_id.clone()?,
            },
        };
        Some(self.begin_publication(action))
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_publication(&mut self, action: PublicationAction) -> PublicationCapture {
        self.publication.pending = true;
        self.publication.retryable = false;
        self.publication.attempt = Some(action.clone());
        self.publication.conflict = false;
        self.publication.error = None;
        self.publication.status = Some(match &action {
            PublicationAction::Publish { revision_id, .. } => {
                format!("Publishing revision {revision_id}…")
            }
            PublicationAction::Unpublish { .. } => "Unpublishing the live revision…".to_owned(),
        });
        PublicationCapture {
            route_epoch: self.route_epoch,
            funnel_id: self.funnel_id.clone(),
            action,
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn is_current_publication(&self, capture: &PublicationCapture) -> bool {
        self.route_epoch == capture.route_epoch
            && self.funnel_id == capture.funnel_id
            && self.publication.pending
            && self.publication.attempt.as_ref() == Some(&capture.action)
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_publication(
        &mut self,
        capture: &PublicationCapture,
        published_revision_id: Option<String>,
    ) -> Option<(u64, String)> {
        if !self.is_current_publication(capture)
            || published_revision_id.as_deref() != capture.action.resulting_revision_id()
        {
            return None;
        }
        self.publication.known = false;
        self.publication.pending = false;
        self.publication.retryable = false;
        self.publication.attempt = None;
        self.publication.refreshing = true;
        self.publication.conflict = false;
        self.publication.error = None;
        self.publication.status =
            Some("Publication request confirmed; checking the current live revision…".to_owned());
        Some((self.route_epoch, self.funnel_id.clone()))
    }

    #[cfg(any(feature = "browser", test))]
    fn fail_publication(
        &mut self,
        capture: &PublicationCapture,
        message: String,
        retryable: bool,
        conflict: bool,
    ) -> bool {
        if !self.is_current_publication(capture) {
            return false;
        }
        self.publication.pending = false;
        self.publication.retryable = retryable;
        if !retryable {
            self.publication.attempt = None;
        }
        self.publication.conflict = conflict;
        self.publication.error = Some(message);
        self.publication.status = None;
        true
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_publication_refresh(&mut self) -> Option<(u64, String, bool)> {
        if self.publication.refreshing || self.publication.pending {
            return None;
        }
        self.publication.refreshing = true;
        Some((
            self.route_epoch,
            self.funnel_id.clone(),
            self.publication.conflict,
        ))
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_publication_refresh(
        &mut self,
        route_epoch: u64,
        funnel_id: &str,
        response: Result<EditorReadResponse, ()>,
        conflict: bool,
    ) {
        if self.route_epoch != route_epoch || self.funnel_id != funnel_id {
            return;
        }
        self.publication.refreshing = false;
        self.publication.conflict = conflict;
        match response {
            Ok(response) if response.funnel.id == funnel_id => {
                self.publication.known = true;
                self.publication.live_revision_id = response.published_revision_id;
                self.publication.error = conflict.then(|| {
                    "Publication changed in another session. Review the refreshed live revision before publishing again."
                        .to_owned()
                });
                self.publication.status =
                    (!conflict).then(|| match self.publication.live_revision_id.as_deref() {
                        Some(revision_id) => format!("Published revision {revision_id}."),
                        None => {
                            "Unpublished. The public route now has no live revision.".to_owned()
                        }
                    });
            }
            _ => {
                self.publication.known = false;
                self.publication.error = Some(
                    "The live revision status could not be refreshed. Try again before publishing or unpublishing."
                        .to_owned(),
                );
                self.publication.status = None;
            }
        }
    }

    fn move_section(&mut self, from: usize, to: usize) -> Option<EditorPreviewCapture> {
        if self.saving || self.retryable || self.stale || self.publication.retryable {
            return None;
        }
        let before = if self.keyboard_section.is_some() || self.keyboard_original_order.is_some() {
            None
        } else {
            Some(self.history_snapshot()?)
        };
        let (moved_section_id, position, section_count) = {
            let document = self.document.as_mut()?;
            if from >= document.order.len() || to >= document.order.len() || from == to {
                return None;
            }
            let item = document.order.remove(from);
            let moved_section_id = item.clone();
            document.order.insert(to, item);
            (moved_section_id, to.saturating_add(1), document.order.len())
        };
        self.announce(format!(
            "Moved {} to position {} of {}.",
            self.section_name(&moved_section_id),
            position,
            section_count
        ));
        if let Some(before) = before {
            self.record_history_from(before, Some(moved_section_id));
        }
        Some(self.begin_preview())
    }

    fn move_section_by_id(
        &mut self,
        section_id: &str,
        delta: isize,
    ) -> Option<EditorPreviewCapture> {
        let document = self.document.as_ref()?;
        let from = document.order.iter().position(|id| id == section_id)?;
        let to = from.checked_add_signed(delta)?;
        self.move_section(from, to)
    }

    fn begin_preview(&mut self) -> EditorPreviewCapture {
        self.generation = self.generation.wrapping_add(1);
        self.preview_error = false;
        let document = self.document.as_ref();
        EditorPreviewCapture {
            generation: self.generation,
            funnel_id: self.funnel_id.clone(),
            base_revision_id: document
                .map(|document| document.base_revision_id.clone())
                .unwrap_or_default(),
            section_ids: document
                .map(|document| document.order.clone())
                .unwrap_or_default(),
            settings_changes: document
                .map(|document| setting_changes(&document.settings_changes))
                .unwrap_or_default(),
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_preview(&mut self, capture: &EditorPreviewCapture, response: Result<String, ()>) {
        self.finish_preview_with_settings(capture, response, None);
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_preview_with_settings(
        &mut self,
        capture: &EditorPreviewCapture,
        response: Result<String, ()>,
        settings_values: Option<BTreeMap<String, EditorSettingValue>>,
    ) {
        if self.funnel_id != capture.funnel_id || self.generation != capture.generation {
            return;
        }
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if document.base_revision_id != capture.base_revision_id
            || document.order != capture.section_ids
            || setting_changes(&document.settings_changes) != capture.settings_changes
        {
            return;
        }
        match response {
            Ok(html) => {
                document.preview_html = html;
                if let Some(preview_color) = settings_values
                    .as_ref()
                    .and_then(|values| values.get("theme.primary_color"))
                {
                    if let Some(color) = document.settings_values.get_mut("theme.primary_color") {
                        color.effective_value = preview_color.effective_value.clone();
                    }
                }
                self.preview_error = false;
            }
            Err(()) => self.preview_error = true,
        }
    }

    fn start_keyboard_drag(&mut self, section_id: &str) -> bool {
        let Some(document) = self.document.as_ref() else {
            return false;
        };
        if self.keyboard_section.is_some()
            || !document.order.iter().any(|id| id == section_id)
            || self.saving
            || self.retryable
            || self.publication.retryable
            || self.stale
        {
            return false;
        }
        self.keyboard_original_order = Some(document.order.clone());
        self.keyboard_section = Some(section_id.to_owned());
        self.announce(format!(
            "Picked up {} at position {} of {}. Use the arrow keys to move, Space to drop, or Escape to cancel.",
            self.section_name(section_id),
            document.order.iter().position(|id| id == section_id).unwrap_or(0) + 1,
            document.order.len()
        ));
        true
    }

    fn finish_keyboard_drag(&mut self) {
        let original_order = self.keyboard_original_order.take();
        let moved_section_id = self.keyboard_section.take();
        if let Some(section_id) = moved_section_id.as_deref() {
            let announcement = self.document.as_ref().map(|document| {
                let position = document
                    .order
                    .iter()
                    .position(|id| id == section_id)
                    .unwrap_or(0)
                    + 1;
                format!(
                    "Dropped {} at position {} of {}.",
                    self.section_name(section_id),
                    position,
                    document.order.len()
                )
            });
            if let Some(announcement) = announcement {
                self.announce(announcement);
            }
        }
        if let (Some(original_order), Some(mut before)) = (original_order, self.history_snapshot())
        {
            before.order = original_order;
            self.record_history_from(before, moved_section_id);
        }
    }

    fn cancel_keyboard_drag(&mut self) -> Option<EditorPreviewCapture> {
        if self.saving || self.retryable || self.stale || self.publication.retryable {
            return None;
        }
        let original = self.keyboard_original_order.take()?;
        self.keyboard_section = None;
        let changed = self
            .document
            .as_ref()
            .is_some_and(|document| document.order != original);
        if let Some(document) = self.document.as_mut() {
            document.order = original;
        }
        self.announce("Reorder canceled. The original order was restored.".to_owned());
        changed.then(|| self.begin_preview())
    }

    fn announce(&mut self, message: String) {
        self.announcement = message;
        self.announcement_revision = self.announcement_revision.wrapping_add(1);
    }

    fn can_save(&self) -> bool {
        !self.saving
            && !self.publication.pending
            && !self.publication.retryable
            && !self.stale
            && self.keyboard_section.is_none()
            && self.keyboard_original_order.is_none()
            && (self.retryable
                || self.document.as_ref().is_some_and(|document| {
                    document.order != document.saved_order || !document.settings_changes.is_empty()
                }))
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_save(&mut self) -> Option<EditorSaveCapture> {
        if !self.can_save() {
            return None;
        }
        let document = self.document.as_ref()?;
        let capture = EditorSaveCapture {
            route_epoch: self.route_epoch,
            generation: self.generation,
            funnel_id: self.funnel_id.clone(),
            base_revision_id: document.base_revision_id.clone(),
            section_ids: document.order.clone(),
            settings_changes: setting_changes(&document.settings_changes),
        };
        self.saving = true;
        self.save_error = None;
        Some(capture)
    }

    #[cfg(any(feature = "browser", test))]
    fn is_current_save(&self, capture: &EditorSaveCapture) -> bool {
        self.route_epoch == capture.route_epoch
            && self.generation == capture.generation
            && self.funnel_id == capture.funnel_id
            && self.saving
            && self.document.as_ref().is_some_and(|document| {
                document.base_revision_id == capture.base_revision_id
                    && document.order == capture.section_ids
                    && setting_changes(&document.settings_changes) == capture.settings_changes
            })
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_save(
        &mut self,
        capture: &EditorSaveCapture,
        revision_id: String,
        settings_values: BTreeMap<String, EditorSettingValue>,
    ) -> Option<EditorPreviewCapture> {
        if !self.is_current_save(capture) {
            return None;
        }
        let document = self.document.as_mut()?;
        document.base_revision_id = revision_id;
        document.saved_order = document.order.clone();
        document.saved_settings_values = settings_values.clone();
        document.settings_values = settings_values;
        document.settings_changes.clear();
        self.saving = false;
        self.retryable = false;
        self.stale = false;
        self.save_error = None;
        self.reset_history();
        Some(self.begin_preview())
    }

    #[cfg(any(feature = "browser", test))]
    fn fail_save(
        &mut self,
        capture: &EditorSaveCapture,
        stale: bool,
        retryable: bool,
        message: String,
    ) -> bool {
        if !self.is_current_save(capture) {
            return false;
        }
        self.saving = false;
        self.stale = stale;
        self.retryable = retryable && !stale;
        self.save_error = Some(message);
        true
    }

    fn section_name(&self, section_id: &str) -> String {
        self.document
            .as_ref()
            .and_then(|document| {
                document
                    .sections
                    .iter()
                    .find(|section| section.id == section_id)
            })
            .map(|section| section_label(&section.section_type))
            .unwrap_or_else(|| "Section".to_owned())
    }

    fn select_section(&mut self, section_id: &str) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if document
            .sections
            .iter()
            .any(|section| section.id == section_id)
        {
            document.selected_section = Some(section_id.to_owned());
        }
    }

    fn undo(&mut self) -> Option<EditorPreviewCapture> {
        if !self.can_undo() {
            return None;
        }
        let index = self.history.cursor.checked_sub(1)?;
        let diff = self.history.entries.get(index)?.clone();
        let snapshot = self.history_snapshot()?;
        let Some(restored) = diff.apply(&snapshot, false) else {
            self.reset_history();
            return None;
        };
        if !self.apply_history_snapshot(restored) {
            self.reset_history();
            return None;
        }
        self.history.cursor = index;
        self.announce_history_change(&diff, false);
        Some(self.begin_preview())
    }

    fn redo(&mut self) -> Option<EditorPreviewCapture> {
        if !self.can_redo() {
            return None;
        }
        let index = self.history.cursor;
        let diff = self.history.entries.get(index)?.clone();
        let snapshot = self.history_snapshot()?;
        let Some(restored) = diff.apply(&snapshot, true) else {
            self.reset_history();
            return None;
        };
        if !self.apply_history_snapshot(restored) {
            self.reset_history();
            return None;
        }
        self.history.cursor = index.saturating_add(1);
        self.announce_history_change(&diff, true);
        Some(self.begin_preview())
    }

    fn can_undo(&self) -> bool {
        !self.history_locked() && self.history.cursor > 0
    }

    fn can_redo(&self) -> bool {
        !self.history_locked() && self.history.cursor < self.history.entries.len()
    }

    fn history_locked(&self) -> bool {
        self.loading
            || self.saving
            || self.retryable
            || self.publication.retryable
            || self.stale
            || self.keyboard_section.is_some()
            || self.keyboard_original_order.is_some()
            || self.document.is_none()
    }

    fn history_snapshot(&self) -> Option<EditorDraftSnapshot> {
        let document = self.document.as_ref()?;
        Some(EditorDraftSnapshot {
            order: document.order.clone(),
            settings_changes: document.settings_changes.clone(),
        })
    }

    fn record_history_from(
        &mut self,
        before: EditorDraftSnapshot,
        moved_section_id: Option<String>,
    ) {
        let Some(after) = self.history_snapshot() else {
            return;
        };
        let Some(diff) = EditorJsonDiff::between(&before, &after, moved_section_id) else {
            return;
        };
        if diff.changes.is_empty() {
            return;
        }
        self.history.entries.truncate(self.history.cursor);
        self.history.entries.push(diff);
        if self.history.entries.len() > EDITOR_HISTORY_LIMIT {
            self.history.entries.remove(0);
        }
        self.history.cursor = self.history.entries.len();
    }

    fn announce_history_change(&mut self, diff: &EditorJsonDiff, forward: bool) {
        let action = if forward { "Redid" } else { "Undid" };
        let mut changes = Vec::new();
        for change in &diff.changes {
            match &change.path {
                EditorJsonPath::Order => {
                    let Some(section_id) = diff.moved_section_id.as_deref() else {
                        changes.push("section order".to_owned());
                        continue;
                    };
                    let position = self.document.as_ref().and_then(|document| {
                        document
                            .order
                            .iter()
                            .position(|id| id == section_id)
                            .map(|index| (index.saturating_add(1), document.order.len()))
                    });
                    let Some((position, count)) = position else {
                        changes.push("section order".to_owned());
                        continue;
                    };
                    changes.push(format!(
                        "moving {} to position {} of {}",
                        self.section_name(section_id),
                        position,
                        count
                    ));
                }
                EditorJsonPath::SettingChange(setting_id) => {
                    let label = self
                        .document
                        .as_ref()
                        .and_then(|document| {
                            document
                                .settings_schema
                                .fields
                                .iter()
                                .find(|field| field.id == *setting_id)
                        })
                        .map(|field| field.label.as_str())
                        .unwrap_or(setting_id);
                    changes.push(format!("change to {}", label));
                }
            }
        }
        self.announce(format!("{} {}.", action, changes.join(" and ")));
    }

    fn apply_history_snapshot(&mut self, snapshot: EditorDraftSnapshot) -> bool {
        let Some(document) = self.document.as_mut() else {
            return false;
        };
        let effective_color = document
            .settings_values
            .get("theme.primary_color")
            .and_then(|value| value.effective_value.clone());
        document.order = snapshot.order;
        document.settings_changes = snapshot.settings_changes;
        let mut settings_values = document.saved_settings_values.clone();
        for (setting_id, setting) in &document.settings_changes {
            settings_values.insert(setting_id.clone(), setting.clone());
        }
        if let Some(primary_color) = settings_values.get_mut("theme.primary_color") {
            primary_color.effective_value =
                effective_color.or_else(|| primary_color.effective_value.clone());
        }
        document.settings_values = settings_values;
        true
    }

    fn reset_history(&mut self) {
        self.history = EditorHistory::default();
    }
}

#[cfg(feature = "browser")]
#[derive(Deserialize)]
struct EditorSaveResponse {
    revision_id: String,
    settings_values: BTreeMap<String, EditorSettingValue>,
}

#[cfg(feature = "browser")]
#[derive(Deserialize)]
struct PublicationResponse {
    published_revision_id: Option<String>,
}

#[cfg(feature = "browser")]
#[derive(Deserialize)]
struct ApiErrorBody {
    code: String,
}

#[cfg(feature = "browser")]
async fn load_editor(funnel_id: &str) -> Result<EditorReadResponse, ()> {
    let path = format!("/api/funnels/{funnel_id}");
    let response = api_request("GET", &path, None, None).await?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<EditorReadResponse>(&response.body).map_err(|_| ())
}

#[cfg(feature = "browser")]
async fn refresh_publication_status(mut state: Signal<EditorState>) {
    let Some((route_epoch, funnel_id, conflict)) =
        state.with_mut(EditorState::begin_publication_refresh)
    else {
        return;
    };
    refresh_publication_status_from(state, route_epoch, funnel_id, conflict).await;
}

#[cfg(feature = "browser")]
async fn refresh_publication_status_from(
    mut state: Signal<EditorState>,
    route_epoch: u64,
    funnel_id: String,
    conflict: bool,
) {
    let response = load_editor(&funnel_id).await;
    if state.try_peek().is_ok() {
        state.with_mut(|state| {
            state.finish_publication_refresh(route_epoch, &funnel_id, response, conflict)
        });
    }
}

#[cfg(feature = "browser")]
fn clear_publication_attempt_if_current(
    state: Signal<EditorState>,
    publication_key: &mut Signal<Option<PreparedIdempotencyKey>>,
    publication_body: &mut Signal<Option<String>>,
    capture: &PublicationCapture,
    prepared_key: &PreparedIdempotencyKey,
) {
    let current = state.with(|state| state.is_current_publication(capture));
    let signal_matches = publication_key().as_ref() == Some(prepared_key);
    let _ = clear_idempotency_storage(prepared_key);
    if current && signal_matches {
        publication_key.set(None);
        publication_body.set(None);
    }
}

#[cfg(feature = "browser")]
async fn change_publication(
    mut state: Signal<EditorState>,
    mut publication_key: Signal<Option<PreparedIdempotencyKey>>,
    mut publication_body: Signal<Option<String>>,
    publish: bool,
) {
    let capture = state.with_mut(|state| {
        if publish {
            state.begin_publish()
        } else {
            state.begin_unpublish()
        }
    });
    let Some(capture) = capture else {
        return;
    };
    let body = capture.action.body();
    let path = capture.action.path(&capture.funnel_id);
    let body = if publication_body().as_deref() == Some(body.as_str()) {
        body
    } else {
        if let Some(key) = publication_key() {
            let _ = clear_idempotency_storage(&key);
        }
        publication_key.set(None);
        publication_body.set(None);
        body
    };
    let prepared_key = match prepare_idempotency_key("POST", &path, Some(&body), publication_key())
    {
        Ok(key) => key,
        Err(()) => {
            state.with_mut(|state| {
                state.fail_publication(
                    &capture,
                    "A retry-safe publication request could not be prepared.".to_owned(),
                    true,
                    false,
                )
            });
            return;
        }
    };
    publication_key.set(Some(prepared_key.clone()));
    publication_body.set(Some(body.clone()));
    let response = api_request("POST", &path, Some(body), Some(prepared_key.clone())).await;
    match response {
        Ok(response) if response.status == 200 => {
            let result = serde_json::from_str::<PublicationResponse>(&response.body);
            match result {
                Ok(result)
                    if result.published_revision_id.as_deref()
                        == capture.action.resulting_revision_id() =>
                {
                    clear_publication_attempt_if_current(
                        state,
                        &mut publication_key,
                        &mut publication_body,
                        &capture,
                        &prepared_key,
                    );
                    let refresh = state.with_mut(|state| {
                        state.finish_publication(&capture, result.published_revision_id)
                    });
                    if let Some((route_epoch, funnel_id)) = refresh {
                        refresh_publication_status_from(state, route_epoch, funnel_id, false).await;
                    }
                }
                _ => {
                    state.with_mut(|state| {
                        state.fail_publication(
                            &capture,
                            "The publication result is uncertain. Retry the same request to confirm it."
                                .to_owned(),
                            true,
                            false,
                        )
                    });
                }
            }
        }
        Ok(response) => {
            let error_code = serde_json::from_str::<ApiErrorBody>(&response.body)
                .map(|error| error.code)
                .unwrap_or_default();
            let conflict = error_code == "conflict/publish_stale_revision";
            let retryable = response.status == 0 || response.status >= 500;
            if !retryable {
                clear_publication_attempt_if_current(
                    state,
                    &mut publication_key,
                    &mut publication_body,
                    &capture,
                    &prepared_key,
                );
            }
            let message = if conflict {
                "Another session changed the live revision. Refreshing publication status; your draft is unchanged."
                    .to_owned()
            } else if retryable {
                "The publication result is uncertain. Retry the same request to confirm it."
                    .to_owned()
            } else {
                error_message(&response.body)
            };
            state.with_mut(|state| state.fail_publication(&capture, message, retryable, conflict));
            if conflict {
                refresh_publication_status(state).await;
            }
        }
        Err(()) => {
            state.with_mut(|state| {
                state.fail_publication(
                    &capture,
                    "The publication result is uncertain. Retry the same request to confirm it."
                        .to_owned(),
                    true,
                    false,
                )
            });
        }
    }
}

#[cfg(feature = "browser")]
async fn fetch_editor_preview(capture: &EditorPreviewCapture) -> Result<EditorPreviewResponse, ()> {
    let path = format!("/api/funnels/{}/preview", capture.funnel_id);
    let body = editor_request_body(
        &capture.base_revision_id,
        &capture.section_ids,
        &capture.settings_changes,
    );
    let response = api_request("POST", &path, Some(body), None).await?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<EditorPreviewResponse>(&response.body).map_err(|_| ())
}

#[cfg(any(feature = "browser", test))]
fn editor_request_body(
    base_revision_id: &str,
    section_ids: &[String],
    settings_changes: &[EditorSettingChange],
) -> String {
    let mut body = serde_json::json!({
        "base_revision_id": base_revision_id,
        "section_ids": section_ids,
    });
    if !settings_changes.is_empty() {
        body["settings"] = serde_json::json!({
            "schema_version": 1,
            "changes": settings_changes,
        });
    }
    body.to_string()
}

#[cfg(feature = "browser")]
async fn update_editor_preview(mut state: Signal<EditorState>, capture: EditorPreviewCapture) {
    let response = fetch_editor_preview(&capture).await;
    if state.try_peek().is_ok() {
        state.with_mut(|state| match response {
            Ok(response) => state.finish_preview_with_settings(
                &capture,
                Ok(response.preview_html),
                Some(response.settings_values),
            ),
            Err(()) => state.finish_preview(&capture, Err(())),
        });
    }
}

#[cfg(feature = "browser")]
fn schedule_editor_preview(state: Signal<EditorState>, capture: Option<EditorPreviewCapture>) {
    if let Some(capture) = capture {
        spawn(async move { update_editor_preview(state, capture).await });
    }
}

#[cfg(not(feature = "browser"))]
fn schedule_editor_preview(_state: Signal<EditorState>, _capture: Option<EditorPreviewCapture>) {}

#[cfg(feature = "browser")]
async fn focus_editor_section(section_id: String) {
    const FOCUS_EDITOR_SECTION: &str = r#"
const sectionId = await dioxus.recv();
const row = document.querySelector(`[data-editor-section="${CSS.escape(sectionId)}"]`);
row?.focus();
"#;
    let request = document::eval(FOCUS_EDITOR_SECTION);
    let _ = request.send(section_id);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewViewport {
    Mobile,
    Tablet,
    Desktop,
}

impl PreviewViewport {
    const ALL: [Self; 3] = [Self::Mobile, Self::Tablet, Self::Desktop];

    const fn width_px(self) -> u16 {
        match self {
            Self::Mobile => 375,
            Self::Tablet => 768,
            Self::Desktop => 1280,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Mobile => "Mobile",
            Self::Tablet => "Tablet",
            Self::Desktop => "Desktop",
        }
    }
}

#[component]
fn ResponsivePreview(title: String, srcdoc: String) -> Element {
    let mut viewport = use_signal(|| PreviewViewport::Desktop);
    let selected = viewport();

    rsx! {
        div { class: "editor-preview-panel",
            div { class: "section-heading",
                h2 { "Preview" }
                span { "Sandboxed" }
            }
            div {
                class: "editor-preview-switcher",
                role: "group",
                "aria-label": "Preview viewport",
                for option in PreviewViewport::ALL {
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        "aria-pressed": "{selected == option}",
                        onclick: move |_| viewport.set(option),
                        "{option.label()} ({option.width_px()} px)"
                    }
                }
            }
            div {
                class: "editor-preview-canvas",
                role: "region",
                tabindex: "0",
                "aria-label": "Preview canvas, {selected.width_px()} pixels wide",
                iframe {
                    class: "editor-preview-frame",
                    title: "{title}",
                    width: "{selected.width_px()}",
                    "sandbox": "",
                    srcdoc: "{srcdoc}",
                }
            }
        }
    }
}

#[component]
pub(super) fn FunnelDetail(id: String) -> Element {
    #[cfg(feature = "browser")]
    let mut state = use_signal(|| EditorState::for_route(id.clone()));
    #[cfg(not(feature = "browser"))]
    let mut state = use_signal(|| EditorState::for_route(id.clone()));
    #[cfg(feature = "browser")]
    let retry_route_id = id.clone();
    #[cfg(feature = "browser")]
    let reload_route_id = id.clone();
    #[cfg(feature = "browser")]
    let mut save_key = use_signal(|| None::<PreparedIdempotencyKey>);
    #[cfg(feature = "browser")]
    let mut save_body = use_signal(|| None::<String>);
    #[cfg(feature = "browser")]
    let mut publication_key = use_signal(|| None::<PreparedIdempotencyKey>);
    #[cfg(feature = "browser")]
    let mut publication_body = use_signal(|| None::<String>);
    let mut unpublish_confirmation = use_signal(|| false);

    #[cfg(feature = "browser")]
    use_effect(use_reactive!(|(id)| {
        let requested_id = id.clone();
        if let Some(key) = save_key.peek().as_ref() {
            let _ = clear_idempotency_storage(key);
        }
        if let Some(key) = publication_key.peek().as_ref() {
            let _ = clear_idempotency_storage(key);
        }
        save_key.set(None);
        save_body.set(None);
        publication_key.set(None);
        publication_body.set(None);
        unpublish_confirmation.set(false);
        let route_epoch = state.with_mut(|state| state.begin_route(requested_id.clone()));
        spawn(async move { load_editor_state(state, requested_id, route_epoch).await });
    }));

    let view = state();
    let title = view
        .document
        .as_ref()
        .map(|document| document.funnel.name.clone())
        .unwrap_or_else(|| "Funnel editor".to_owned());

    rsx! {
        div {
            class: "editor-page",
            onkeydown: move |event: KeyboardEvent| {
                let modifiers = event.modifiers();
                if !modifiers.ctrl() || modifiers.shift() || modifiers.alt() || modifiers.meta() {
                    return;
                }
                let capture = match event.key() {
                    Key::Character(value) if value.eq_ignore_ascii_case("z") => {
                        event.prevent_default();
                        state.with_mut(EditorState::undo)
                    }
                    Key::Character(value) if value.eq_ignore_ascii_case("y") => {
                        event.prevent_default();
                        state.with_mut(EditorState::redo)
                    }
                    _ => return,
                };
                schedule_editor_preview(state, capture);
            },
        section { class: "page-heading",
            p { class: "eyebrow", "Funnels / Editor" }
            h1 { "{title}" }
            p { class: "page-description", "Reorder sections and preview the funnel before saving a new revision." }
            code { class: "detail-id", "Funnel ID: {id}" }
        }
        if view.loading {
            section { class: "empty-state editor-loading", role: "status", "aria-label": "Funnel editor",
                "Loading the current funnel revision…"
            }
        }
        if view.load_error {
            section { class: "empty-state editor-load-error", role: "alert", "aria-label": "Funnel editor error",
                p { "The funnel editor could not be loaded." }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    onclick: move |_| {
                        #[cfg(feature = "browser")]
                        {
                            let route_epoch = state.with_mut(|state| state.begin_route(retry_route_id.clone()));
                            let requested_id = retry_route_id.clone();
                            spawn(async move { load_editor_state(state, requested_id, route_epoch).await });
                        }
                    },
                    "Try again"
                }
            }
        }
        if let Some(document) = view.document.clone() {
            section { class: "editor-toolbar", "aria-label": "Editor actions",
                div {
                    p { class: "eyebrow", "Draft revision" }
                    code { "{document.base_revision_id}" }
                    if document.order != document.saved_order || !document.settings_changes.is_empty() {
                        span { class: "editor-unsaved-badge", "Unsaved changes" }
                    } else {
                        span { class: "editor-saved-badge", "All changes saved" }
                    }
                }
                div { class: "editor-actions",
                    if view.stale {
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            onclick: move |_| {
                                #[cfg(feature = "browser")]
                                {
                                    if let Some(key) = save_key() { let _ = clear_idempotency_storage(&key); }
                                    save_key.set(None);
                                    save_body.set(None);
                                    let route_epoch = state.with_mut(|state| state.begin_route(reload_route_id.clone()));
                                    let requested_id = reload_route_id.clone();
                                    spawn(async move { load_editor_state(state, requested_id, route_epoch).await });
                                }
                            },
                            "Reload current revision"
                        }
                    }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        "aria-keyshortcuts": "Control+Z",
                        disabled: !view.can_undo(),
                        onclick: move |_| {
                            let capture = state.with_mut(EditorState::undo);
                            schedule_editor_preview(state, capture);
                        },
                        "Undo"
                    }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        "aria-keyshortcuts": "Control+Y",
                        disabled: !view.can_redo(),
                        onclick: move |_| {
                            let capture = state.with_mut(EditorState::redo);
                            schedule_editor_preview(state, capture);
                        },
                        "Redo"
                    }
                    button {
                        class: "primary-button",
                        r#type: "button",
                        disabled: !view.can_save(),
                        onclick: move |_| {
                            #[cfg(feature = "browser")]
                            spawn(async move {
                                save_editor_revision(state, save_key, save_body).await;
                            });
                        },
                        if view.retryable { "Retry save" } else if view.saving { "Saving…" } else { "Save revision" }
                    }
                }
            }
            if let Some(error) = view.save_error.clone() {
                p { class: "error-message editor-save-error", role: "alert", "{error}" }
            }
            if view.preview_error {
                p { class: "error-message editor-preview-error", role: "status",
                    "Preview could not be refreshed. Showing the last successful preview."
                }
            }
            section {
                class: "editor-publication-panel",
                role: "region",
                "aria-label": "Publication",
                div { class: "section-heading",
                    h2 { "Publication" }
                    span { "Draft and live status" }
                }
                div { class: "editor-publication-facts",
                    p {
                        strong { "Draft revision " }
                        code { "{document.base_revision_id}" }
                    }
                    p {
                        strong { "Live revision " }
                        if !view.publication.known {
                            span { "Status unavailable" }
                        } else if let Some(live_revision_id) = view.publication.live_revision_id.as_ref() {
                            code { "{live_revision_id}" }
                        } else {
                            span { "Not published" }
                        }
                    }
                    if view.publication.known && view.publication.live_revision_id.is_some() {
                        a {
                            class: "editor-live-link",
                            href: "/f/{document.funnel.slug}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "Open live funnel"
                        }
                    } else {
                        p { class: "editor-publication-url",
                            "Live URL: /f/{document.funnel.slug}"
                        }
                    }
                }
                if let Some(status) = view.publication.status.as_ref() {
                    p {
                        class: "editor-publication-status",
                        role: "status",
                        "aria-live": "polite",
                        "aria-atomic": "true",
                        "{status}"
                    }
                }
                if let Some(error) = view.publication.error.as_ref() {
                    p { class: "error-message editor-publication-error", role: "alert", "{error}" }
                }
                if view.has_unsaved_changes() {
                    p { class: "editor-publication-help", "Save draft changes before publishing." }
                } else if !view.publication.known {
                    p { class: "editor-publication-help", "Refresh live status before changing publication." }
                } else if view.publication.live_revision_id.as_deref() == Some(&document.base_revision_id) {
                    p { class: "editor-publication-help", "The saved draft is already live." }
                }
                div { class: "editor-publication-actions",
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        disabled: view.publication.pending || view.publication.refreshing,
                        onclick: move |_| {
                            #[cfg(feature = "browser")]
                            spawn(async move { refresh_publication_status(state).await; });
                        },
                        if view.publication.refreshing { "Refreshing…" } else { "Refresh live status" }
                    }
                    button {
                        class: "primary-button",
                        r#type: "button",
                        disabled: !view.can_publish(),
                        onclick: move |_| {
                            #[cfg(feature = "browser")]
                            spawn(async move {
                                change_publication(state, publication_key, publication_body, true).await;
                            });
                        },
                        if view.publication.pending && view.publication.is_publish_attempt() {
                            "Publishing…"
                        } else if view.publication.retryable && view.publication.is_publish_attempt() {
                            "Retry publish"
                        } else {
                            "Publish revision"
                        }
                    }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        disabled: !view.can_unpublish(),
                        onclick: move |_| {
                            if view.publication.retryable && view.publication.is_unpublish_attempt() {
                                #[cfg(feature = "browser")]
                                spawn(async move {
                                    change_publication(state, publication_key, publication_body, false).await;
                                });
                            } else {
                                unpublish_confirmation.set(true);
                                #[cfg(feature = "browser")]
                                spawn(async move {
                                    super::set_modal_dialog("unpublish-funnel-dialog", true).await;
                                });
                            }
                        },
                        if view.publication.pending && view.publication.is_unpublish_attempt() {
                            "Unpublishing…"
                        } else if view.publication.retryable && view.publication.is_unpublish_attempt() {
                            "Retry unpublish"
                        } else {
                            "Unpublish"
                        }
                    }
                }
            }
            if unpublish_confirmation() {
                dialog {
                    id: "unpublish-funnel-dialog",
                    class: "funnel-dialog confirmation-dialog",
                    "aria-modal": "true",
                    "aria-labelledby": "unpublish-funnel-title",
                    "aria-describedby": "unpublish-funnel-description",
                    oncancel: move |event| {
                        if view.publication.pending {
                            event.prevent_default();
                        } else {
                            unpublish_confirmation.set(false);
                        }
                    },
                    h2 { id: "unpublish-funnel-title", "Unpublish this funnel?" }
                    p { id: "unpublish-funnel-description",
                        "This removes the live revision pointer. Already viewed copies may remain available in browsers or intermediary caches."
                    }
                    if let Some(error) = view.publication.error.as_ref() {
                        p { class: "error-message", role: "alert", "{error}" }
                    }
                    div { class: "dialog-actions",
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: view.publication.pending,
                            autofocus: true,
                            onclick: move |_| {
                                unpublish_confirmation.set(false);
                                #[cfg(feature = "browser")]
                                spawn(async move {
                                    super::set_modal_dialog("unpublish-funnel-dialog", false).await;
                                });
                            },
                            "Cancel"
                        }
                        button {
                            class: "primary-button danger-button",
                            r#type: "button",
                            disabled: view.publication.pending,
                            onclick: move |_| {
                                unpublish_confirmation.set(false);
                                #[cfg(feature = "browser")]
                                spawn(async move {
                                    super::set_modal_dialog("unpublish-funnel-dialog", false).await;
                                    change_publication(state, publication_key, publication_body, false).await;
                                });
                            },
                            "Confirm unpublish"
                        }
                    }
                }
            }
            section { class: "editor-workspace", "aria-label": "Visual editor",
                div { class: "editor-outline-panel",
                    div { class: "section-heading",
                        h2 { "Sections" }
                        span { "{document.order.len()} sections" }
                    }
                    EditorSortableList { state }
                    p { class: "editor-keyboard-help",
                        "Focus a section and press Space. Use the arrow keys to reorder it, Space to drop, or Escape to cancel. Undo with Ctrl+Z. Redo with Ctrl+Y."
                    }
                    p { class: "sr-only", role: "status", "aria-live": "polite", "aria-atomic": "true",
                        span {
                            if view.announcement_revision % 2 == 0 {
                                "{view.announcement}"
                            }
                        }
                        span {
                            if view.announcement_revision % 2 != 0 {
                                "{view.announcement}"
                            }
                        }
                    }
                }
                EditorInspector { state }
                ResponsivePreview {
                    title: format!("Preview of {}", document.funnel.name),
                    srcdoc: document.preview_html,
                }
            }
        }
        }
    }
}

#[component]
fn EditorSortableList(state: Signal<EditorState>) -> Element {
    let view = state();
    let Some(document) = view.document else {
        return rsx! { div {} };
    };
    let rows_locked = view.saving || view.retryable || view.stale || view.publication.retryable;
    let sections = document.sections;
    let order = document.order;
    let mut editor_state = state;
    let item_order = order.clone();
    let render_order = order.clone();
    rsx! {
        if rows_locked {
            div { class: "editor-section-list", role: "list", "aria-label": "Funnel sections",
                for (index, section_id) in order.iter().enumerate() {
                    if let Some(section) = sections.iter().find(|section| section.id == *section_id) {
                        EditorSectionRow {
                            state: editor_state,
                            section: section.clone(),
                            index,
                            count: order.len(),
                            disabled: true,
                        }
                    }
                }
            }
        } else {
            SortableList {
                len: order.len(),
                class: "editor-section-list",
                role: "list",
                "aria-label": "Funnel sections",
                touch_handle: true,
                item_key: move |index| item_order.get(index).cloned().unwrap_or_default(),
                handle: move |_| rsx! { span { class: "editor-drag-grip", "⠿" } },
                render: move |index| {
                    let Some(section_id) = render_order.get(index) else { return rsx! { div {} }; };
                    let Some(section) = sections.iter().find(|section| section.id == *section_id) else {
                        return rsx! { div {} };
                    };
                    rsx! {
                        EditorSectionRow {
                            state: editor_state,
                            section: section.clone(),
                            index,
                            count: render_order.len(),
                            disabled: false,
                        }
                    }
                },
                on_sort: move |event: SortEvent| {
                    let section_id = editor_state.with(|state| {
                        state.document.as_ref().and_then(|document| document.order.get(event.from).cloned())
                    });
                    let capture = editor_state.with_mut(|state| state.move_section(event.from, event.to));
                    if let Some(section_id) = section_id {
                        #[cfg(feature = "browser")]
                        spawn(async move { focus_editor_section(section_id).await });
                        #[cfg(not(feature = "browser"))]
                        let _ = section_id;
                    }
                    #[cfg(feature = "browser")]
                    schedule_editor_preview(editor_state, capture);
                    #[cfg(not(feature = "browser"))]
                    let _ = capture;
                },
            }
        }
    }
}

#[component]
fn EditorSectionRow(
    state: Signal<EditorState>,
    section: EditorSection,
    index: usize,
    count: usize,
    disabled: bool,
) -> Element {
    let section_id = section.id.clone();
    let up_section_id = section_id.clone();
    let down_section_id = section_id.clone();
    let title = section_label(&section.section_type);
    let selected = state.with(|state| {
        state
            .document
            .as_ref()
            .and_then(|document| document.selected_section.as_deref())
            == Some(section_id.as_str())
    });
    let focus_section_id = section_id.clone();
    let click_section_id = section_id.clone();
    rsx! {
        div {
            class: "editor-section-row",
            role: "listitem",
            tabindex: "0",
            "data-editor-section": "{section_id}",
            "aria-current": if selected { "true" } else { "false" },
            "aria-label": "Section: {title}, position {index + 1} of {count}. Press Space to pick up and reorder.",
            onfocus: move |_| state.with_mut(|state| state.select_section(&focus_section_id)),
            onclick: move |_| state.with_mut(|state| state.select_section(&click_section_id)),
            onkeydown: move |event: KeyboardEvent| {
                if disabled { return; }
                let key = event.key();
                if matches!(&key, Key::Character(value) if value == " ") || key == Key::Enter {
                    event.prevent_default();
                    let active = state.with(|state| state.keyboard_section.clone());
                    if active.as_deref() == Some(section_id.as_str()) {
                        state.with_mut(EditorState::finish_keyboard_drag);
                    } else {
                        state.with_mut(|state| { state.start_keyboard_drag(&section_id); });
                    }
                } else if key == Key::Escape {
                    let capture = state.with_mut(EditorState::cancel_keyboard_drag);
                    if capture.is_some() { event.prevent_default(); }
                    #[cfg(feature = "browser")]
                    schedule_editor_preview(state, capture);
                } else if key == Key::ArrowUp || key == Key::ArrowDown {
                    let active = state.with(|state| state.keyboard_section.clone());
                    if active.as_deref() == Some(section_id.as_str()) {
                        event.prevent_default();
                        let delta = if key == Key::ArrowUp { -1 } else { 1 };
                        let capture = state.with_mut(|state| state.move_section_by_id(&section_id, delta));
                        #[cfg(feature = "browser")]
                        {
                            let focus_id = section_id.clone();
                            spawn(async move { focus_editor_section(focus_id).await });
                            schedule_editor_preview(state, capture);
                        }
                        #[cfg(not(feature = "browser"))]
                        let _ = capture;
                    }
                }
            },
            div { class: "editor-section-copy",
                span { class: "editor-section-type", "{title}" }
                code { "{section.id}" }
            }
            div { class: "editor-section-actions",
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    "aria-label": "Move up {title}",
                    onkeydown: move |event: KeyboardEvent| {
                        let key = event.key();
                        let modifiers = event.modifiers();
                        let history_shortcut = modifiers.ctrl()
                            && !modifiers.shift()
                            && !modifiers.alt()
                            && !modifiers.meta()
                            && matches!(&key, Key::Character(value) if value.eq_ignore_ascii_case("z") || value.eq_ignore_ascii_case("y"));
                        if !history_shortcut {
                            event.stop_propagation();
                        }
                    },
                    disabled: disabled || index == 0,
                    onclick: move |_| {
                        let capture = state.with_mut(|state| state.move_section_by_id(&up_section_id, -1));
                        #[cfg(feature = "browser")]
                        {
                            let focus_id = up_section_id.clone();
                            spawn(async move { focus_editor_section(focus_id).await });
                            schedule_editor_preview(state, capture);
                        }
                        #[cfg(not(feature = "browser"))]
                        let _ = capture;
                    },
                    "Move up"
                }
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    "aria-label": "Move down {title}",
                    onkeydown: move |event: KeyboardEvent| {
                        let key = event.key();
                        let modifiers = event.modifiers();
                        let history_shortcut = modifiers.ctrl()
                            && !modifiers.shift()
                            && !modifiers.alt()
                            && !modifiers.meta()
                            && matches!(&key, Key::Character(value) if value.eq_ignore_ascii_case("z") || value.eq_ignore_ascii_case("y"));
                        if !history_shortcut {
                            event.stop_propagation();
                        }
                    },
                    disabled: disabled || index + 1 >= count,
                    onclick: move |_| {
                        let capture = state.with_mut(|state| state.move_section_by_id(&down_section_id, 1));
                        #[cfg(feature = "browser")]
                        {
                            let focus_id = down_section_id.clone();
                            spawn(async move { focus_editor_section(focus_id).await });
                            schedule_editor_preview(state, capture);
                        }
                        #[cfg(not(feature = "browser"))]
                        let _ = capture;
                    },
                    "Move down"
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
async fn load_editor_state(mut state: Signal<EditorState>, funnel_id: String, route_epoch: u64) {
    let response = load_editor(&funnel_id).await;
    if state.try_peek().is_ok() {
        state.with_mut(|state| state.finish_load(route_epoch, &funnel_id, response));
    }
}

#[cfg(feature = "browser")]
fn clear_save_attempt_if_current(
    state: Signal<EditorState>,
    save_key: &mut Signal<Option<PreparedIdempotencyKey>>,
    save_body: &mut Signal<Option<String>>,
    capture: &EditorSaveCapture,
    prepared_key: &PreparedIdempotencyKey,
) {
    let current = state.with(|state| state.is_current_save(capture));
    let signal_matches = save_key().as_ref() == Some(prepared_key);
    let _ = clear_idempotency_storage(prepared_key);
    if current && signal_matches {
        save_key.set(None);
        save_body.set(None);
    }
}

#[cfg(feature = "browser")]
async fn save_editor_revision(
    mut state: Signal<EditorState>,
    mut save_key: Signal<Option<PreparedIdempotencyKey>>,
    mut save_body: Signal<Option<String>>,
) {
    let capture = state.with_mut(EditorState::begin_save);
    let Some(capture) = capture else {
        return;
    };
    let expected_body = editor_request_body(
        &capture.base_revision_id,
        &capture.section_ids,
        &capture.settings_changes,
    );
    let body = if save_body().as_deref() == Some(expected_body.as_str()) {
        expected_body
    } else {
        if let Some(key) = save_key() {
            let _ = clear_idempotency_storage(&key);
        }
        save_key.set(None);
        save_body.set(None);
        expected_body
    };
    let path = format!("/api/funnels/{}/revisions", capture.funnel_id);
    let prepared_key = match prepare_idempotency_key("PUT", &path, Some(&body), save_key()) {
        Ok(key) => key,
        Err(()) => {
            state.with_mut(|state| {
                state.fail_save(
                    &capture,
                    false,
                    true,
                    "A retry-safe save could not be prepared.".to_owned(),
                )
            });
            return;
        }
    };
    save_key.set(Some(prepared_key.clone()));
    save_body.set(Some(body.clone()));
    let response = api_request("PUT", &path, Some(body), Some(prepared_key.clone())).await;
    match response {
        Ok(response) if response.status == 200 => {
            let saved = serde_json::from_str::<EditorSaveResponse>(&response.body);
            if let Ok(saved) = saved {
                clear_save_attempt_if_current(
                    state,
                    &mut save_key,
                    &mut save_body,
                    &capture,
                    &prepared_key,
                );
                let preview = state.with_mut(|state| {
                    state.finish_save(&capture, saved.revision_id, saved.settings_values)
                });
                schedule_editor_preview(state, preview);
            } else {
                state.with_mut(|state| {
                    state.fail_save(
                        &capture,
                        false,
                        true,
                        "The save result is uncertain. Retry this save to confirm it.".to_owned(),
                    )
                });
            }
        }
        Ok(response) => {
            let error_code = serde_json::from_str::<ApiErrorBody>(&response.body)
                .map(|error| error.code)
                .unwrap_or_default();
            let stale = error_code == "conflict/edit_stale_revision";
            let definitive_client_error = (400..500).contains(&response.status);
            if definitive_client_error {
                clear_save_attempt_if_current(
                    state,
                    &mut save_key,
                    &mut save_body,
                    &capture,
                    &prepared_key,
                );
            }
            let message = if stale {
                "This funnel changed in another tab. Reload the current revision before saving."
                    .to_owned()
            } else if error_code == "conflict/revision_limit_reached" {
                "This funnel has reached its 100 revision limit.".to_owned()
            } else if response.status == 0 || response.status >= 500 {
                "The save result is uncertain. Retry the same save to confirm it.".to_owned()
            } else {
                error_message(&response.body)
            };
            let retryable = response.status == 0 || response.status >= 500;
            state.with_mut(|state| state.fail_save(&capture, stale, retryable, message));
        }
        Err(()) => {
            state.with_mut(|state| {
                state.fail_save(
                    &capture,
                    false,
                    true,
                    "The save result is uncertain. Retry the same save to confirm it.".to_owned(),
                )
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_settings() -> (EditorSettingsSchema, BTreeMap<String, EditorSettingValue>) {
        let schema = EditorSettingsSchema {
            version: 1,
            fields: vec![
                EditorSettingField {
                    id: "theme.preset".to_owned(),
                    label: "Theme preset".to_owned(),
                    control: "select".to_owned(),
                    section_id: None,
                    options: vec![EditorSettingOption {
                        value: "minimal".to_owned(),
                        label: "Minimal".to_owned(),
                    }],
                    multiline: false,
                    optional: false,
                    nullable: false,
                },
                EditorSettingField {
                    id: "theme.primary_color".to_owned(),
                    label: "Primary color".to_owned(),
                    control: "color".to_owned(),
                    section_id: None,
                    options: Vec::new(),
                    multiline: false,
                    optional: false,
                    nullable: true,
                },
                EditorSettingField {
                    id: "theme.square_buttons".to_owned(),
                    label: "Square button corners".to_owned(),
                    control: "toggle".to_owned(),
                    section_id: None,
                    options: Vec::new(),
                    multiline: false,
                    optional: false,
                    nullable: false,
                },
                EditorSettingField {
                    id: "section.hero.title".to_owned(),
                    label: "Headline".to_owned(),
                    control: "text".to_owned(),
                    section_id: Some("hero".to_owned()),
                    options: Vec::new(),
                    multiline: false,
                    optional: false,
                    nullable: false,
                },
                EditorSettingField {
                    id: "section.hero.image".to_owned(),
                    label: "Hero image".to_owned(),
                    control: "image".to_owned(),
                    section_id: Some("hero".to_owned()),
                    options: Vec::new(),
                    multiline: false,
                    optional: true,
                    nullable: true,
                },
                EditorSettingField {
                    id: "section.testimonials.items.0.attribution".to_owned(),
                    label: "Attribution".to_owned(),
                    control: "text".to_owned(),
                    section_id: Some("testimonials".to_owned()),
                    options: Vec::new(),
                    multiline: false,
                    optional: true,
                    nullable: true,
                },
            ],
        };
        let values = BTreeMap::from([
            (
                "theme.preset".to_owned(),
                EditorSettingValue {
                    kind: "select".to_owned(),
                    value: serde_json::json!("minimal"),
                    effective_value: None,
                },
            ),
            (
                "theme.primary_color".to_owned(),
                EditorSettingValue {
                    kind: "color".to_owned(),
                    value: serde_json::Value::Null,
                    effective_value: Some("#334155".to_owned()),
                },
            ),
            (
                "theme.square_buttons".to_owned(),
                EditorSettingValue {
                    kind: "toggle".to_owned(),
                    value: serde_json::json!(false),
                    effective_value: None,
                },
            ),
            (
                "section.hero.title".to_owned(),
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("Offer"),
                    effective_value: None,
                },
            ),
            (
                "section.hero.image".to_owned(),
                EditorSettingValue {
                    kind: "image".to_owned(),
                    value: serde_json::Value::Null,
                    effective_value: None,
                },
            ),
            (
                "section.testimonials.items.0.attribution".to_owned(),
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("Founder"),
                    effective_value: None,
                },
            ),
        ]);
        (schema, values)
    }

    fn loaded_editor_state() -> EditorState {
        let mut state = EditorState::for_route("funnel-1".to_owned());
        let (settings_schema, settings_values) = fixture_settings();
        state.finish_load(
            state.route_epoch,
            "funnel-1",
            Ok(EditorReadResponse {
                funnel: EditorFunnel {
                    id: "funnel-1".to_owned(),
                    name: "Offer".to_owned(),
                    slug: "offer".to_owned(),
                    updated_at: "now".to_owned(),
                },
                base_revision_id: "revision-1".to_owned(),
                published_revision_id: None,
                sections: vec![
                    EditorSection {
                        id: "hero".to_owned(),
                        section_type: "hero".to_owned(),
                    },
                    EditorSection {
                        id: "features".to_owned(),
                        section_type: "features".to_owned(),
                    },
                    EditorSection {
                        id: "cta".to_owned(),
                        section_type: "cta".to_owned(),
                    },
                ],
                settings_schema,
                settings_values,
                preview_html: "preview-1".to_owned(),
            }),
        );
        state
    }

    fn editor_response(
        funnel_id: &str,
        base_revision_id: &str,
        published_revision_id: Option<&str>,
    ) -> EditorReadResponse {
        let (settings_schema, settings_values) = fixture_settings();
        EditorReadResponse {
            funnel: EditorFunnel {
                id: funnel_id.to_owned(),
                name: "Offer".to_owned(),
                slug: "offer".to_owned(),
                updated_at: "now".to_owned(),
            },
            base_revision_id: base_revision_id.to_owned(),
            published_revision_id: published_revision_id.map(str::to_owned),
            sections: vec![
                EditorSection {
                    id: "hero".to_owned(),
                    section_type: "hero".to_owned(),
                },
                EditorSection {
                    id: "features".to_owned(),
                    section_type: "features".to_owned(),
                },
                EditorSection {
                    id: "cta".to_owned(),
                    section_type: "cta".to_owned(),
                },
            ],
            settings_schema,
            settings_values,
            preview_html: "preview".to_owned(),
        }
    }

    #[test]
    fn publication_requires_a_saved_draft_and_uses_the_live_pointer_as_its_cas_base() {
        let mut state = loaded_editor_state();
        assert!(state.can_publish());
        assert!(!state.can_unpublish());

        state
            .set_setting(
                "section.hero.title",
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("Unsaved heading"),
                    effective_value: None,
                },
            )
            .expect("the editor accepts a local change");
        assert!(!state.can_publish());

        state = loaded_editor_state();
        state.publication.live_revision_id = Some("revision-live".to_owned());
        state.publication.known = true;
        assert!(state.can_publish());
        assert!(state.can_unpublish());
        let capture = state.begin_publish().expect("saved draft can be published");
        assert_eq!(
            capture.action.path("funnel-1"),
            "/api/funnels/funnel-1/publish"
        );
        assert_eq!(
            capture.action.body(),
            r#"{"expected_published_revision_id":"revision-live","revision_id":"revision-1"}"#
        );
    }

    #[test]
    fn publication_conflict_refresh_preserves_unsaved_editor_changes() {
        let mut state = loaded_editor_state();
        state
            .move_section(0, 1)
            .expect("the draft can be reordered");
        let save = state.begin_save().expect("the changed draft can be saved");
        let (_, settings_values) = fixture_settings();
        state
            .finish_save(&save, "revision-2".to_owned(), settings_values)
            .expect("the editor records the saved revision");

        let publish = state
            .begin_publish()
            .expect("saved revision can be published");
        assert_eq!(
            publish.action,
            PublicationAction::Publish {
                revision_id: "revision-2".to_owned(),
                expected_published_revision_id: None,
            }
        );
        state
            .set_setting(
                "section.hero.title",
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("Unsaved during publication"),
                    effective_value: None,
                },
            )
            .expect("the local editor remains available during publication");
        let local_draft = state.document.clone().expect("loaded editor document");
        assert!(state.fail_publication(&publish, "stale live pointer".to_owned(), false, true,));

        let (route_epoch, funnel_id, conflict) = state
            .begin_publication_refresh()
            .expect("publication status can be refreshed");
        let response = editor_response(&funnel_id, "revision-external", Some("revision-live"));
        state.finish_publication_refresh(route_epoch, &funnel_id, Ok(response), conflict);

        assert_eq!(
            state.publication.live_revision_id.as_deref(),
            Some("revision-live")
        );
        assert!(state.publication.conflict);
        assert_eq!(state.document, Some(local_draft));
        assert!(
            !state.can_publish(),
            "the unsaved local edit must remain unsaved"
        );
    }

    #[test]
    fn ambiguous_publication_retry_reuses_the_exact_action_and_unpublish_cas_target() {
        let mut state = loaded_editor_state();
        let first = state.begin_publish().expect("saved draft can be published");
        let first_body = first.action.body();
        assert!(state.fail_publication(&first, "request outcome unknown".to_owned(), true, false,));
        let retry = state
            .begin_publish()
            .expect("ambiguous publish can be retried");
        assert_eq!(retry.action, first.action);
        assert_eq!(retry.action.body(), first_body);
        assert_eq!(retry.action.resulting_revision_id(), Some("revision-1"));
        let (route_epoch, funnel_id) = state
            .finish_publication(&retry, Some("revision-1".to_owned()))
            .expect("the retry response starts a current-pointer refresh");
        state.finish_publication_refresh(
            route_epoch,
            &funnel_id,
            Ok(editor_response(
                &funnel_id,
                "revision-1",
                Some("revision-1"),
            )),
            false,
        );
        assert!(!state.can_publish(), "the saved revision is now live");

        let unpublish = state
            .begin_unpublish()
            .expect("a published funnel can be unpublished");
        assert_eq!(
            unpublish.action.path("funnel-1"),
            "/api/funnels/funnel-1/unpublish"
        );
        assert_eq!(
            unpublish.action.body(),
            r#"{"expected_published_revision_id":"revision-1"}"#
        );
        let (route_epoch, funnel_id) = state
            .finish_publication(&unpublish, None)
            .expect("unpublish response starts a current-pointer refresh");
        state.finish_publication_refresh(
            route_epoch,
            &funnel_id,
            Ok(editor_response(&funnel_id, "revision-1", None)),
            false,
        );
        assert!(state.publication.known);
        assert_eq!(state.publication.live_revision_id, None);
        assert!(!state.can_unpublish());
    }

    #[test]
    fn replayed_publication_receipt_refreshes_the_current_live_pointer() {
        let mut state = loaded_editor_state();
        let first = state.begin_publish().expect("saved draft can be published");
        assert!(state.fail_publication(&first, "request outcome unknown".to_owned(), true, false));
        let retry = state
            .begin_publish()
            .expect("ambiguous publish can be retried");

        let (route_epoch, funnel_id) = state
            .finish_publication(&retry, Some("revision-1".to_owned()))
            .expect("the successful replay starts a current-pointer refresh");
        assert!(state.publication.refreshing);
        assert!(
            !state.publication.known,
            "the stored replay response is not treated as the current live pointer"
        );
        state.finish_publication_refresh(
            route_epoch,
            &funnel_id,
            Ok(editor_response(
                &funnel_id,
                "revision-1",
                Some("revision-external"),
            )),
            false,
        );

        assert_eq!(
            state.publication.live_revision_id.as_deref(),
            Some("revision-external")
        );
        assert!(state.can_publish());
        let next = state
            .begin_publish()
            .expect("the draft can publish against the refreshed current pointer");
        assert_eq!(
            next.action,
            PublicationAction::Publish {
                revision_id: "revision-1".to_owned(),
                expected_published_revision_id: Some("revision-external".to_owned()),
            }
        );
    }

    #[test]
    fn ambiguous_publish_retry_stays_available_after_an_edit_during_the_request() {
        let mut state = loaded_editor_state();
        let first = state.begin_publish().expect("saved draft can be published");
        state
            .set_setting(
                "section.hero.title",
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("Unsaved during publication"),
                    effective_value: None,
                },
            )
            .expect("local editing remains available while publication is pending");
        assert!(state.start_keyboard_drag("hero"));
        let local_draft = state.document.clone();

        assert!(state.fail_publication(&first, "request outcome unknown".to_owned(), true, false));
        assert!(
            state.can_publish(),
            "an ambiguous publish must remain retryable even with an unsaved local edit"
        );
        let retry = state
            .begin_publish()
            .expect("the exact publish action remains retryable");
        assert_eq!(retry.action, first.action);
        assert_eq!(retry.action.body(), first.action.body());
        assert_eq!(state.document, local_draft);
        let (route_epoch, funnel_id) = state
            .finish_publication(&retry, Some("revision-1".to_owned()))
            .expect("the retry response starts a current-pointer refresh");
        state.finish_publication_refresh(
            route_epoch,
            &funnel_id,
            Ok(editor_response(
                &funnel_id,
                "revision-1",
                Some("revision-1"),
            )),
            false,
        );
        assert_eq!(state.keyboard_section.as_deref(), Some("hero"));
        assert!(state.cancel_keyboard_drag().is_none());
        assert!(
            state.can_save(),
            "the local draft is saveable after retry resolves"
        );
    }

    #[test]
    fn editor_keeps_one_hundred_setting_edits_undoable_and_redoable() {
        let mut state = loaded_editor_state();
        for index in 0..100 {
            state
                .set_setting(
                    "section.hero.title",
                    EditorSettingValue {
                        kind: "text".to_owned(),
                        value: serde_json::json!(format!("history-edit-{index}")),
                        effective_value: None,
                    },
                )
                .expect("each accepted title edit schedules a preview");
        }

        for _ in 0..50 {
            state.undo().expect("each prior edit can be undone");
        }
        assert_eq!(
            state
                .document
                .as_ref()
                .and_then(|document| document.settings_values.get("section.hero.title"))
                .map(|value| &value.value),
            Some(&serde_json::json!("history-edit-49"))
        );

        for _ in 0..50 {
            state.redo().expect("each undone edit can be redone");
        }
        assert_eq!(
            state
                .document
                .as_ref()
                .and_then(|document| document.settings_values.get("section.hero.title"))
                .map(|value| &value.value),
            Some(&serde_json::json!("history-edit-99"))
        );
    }

    #[test]
    fn editor_undo_distinguishes_missing_changes_from_nullable_values() {
        let mut state = loaded_editor_state();
        let attribution_id = "section.testimonials.items.0.attribution";
        state
            .set_setting(
                attribution_id,
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::Value::Null,
                    effective_value: None,
                },
            )
            .expect("nullable attribution edit schedules a preview");

        assert!(state
            .document
            .as_ref()
            .is_some_and(|document| document.settings_changes.contains_key(attribution_id)));
        state.undo().expect("nullable change can be undone");
        assert!(!state
            .document
            .as_ref()
            .is_some_and(|document| document.settings_changes.contains_key(attribution_id)));
        assert_eq!(
            state
                .document
                .as_ref()
                .and_then(|document| document.settings_values.get(attribution_id))
                .map(|value| &value.value),
            Some(&serde_json::json!("Founder"))
        );

        state.redo().expect("nullable change can be redone");
        assert_eq!(
            state
                .document
                .as_ref()
                .and_then(|document| document.settings_changes.get(attribution_id))
                .map(|value| &value.value),
            Some(&serde_json::Value::Null)
        );
        assert_eq!(
            state
                .document
                .as_ref()
                .and_then(|document| document.settings_values.get(attribution_id))
                .map(|value| &value.value),
            Some(&serde_json::Value::Null)
        );
    }

    #[test]
    fn editor_new_edit_discards_redo_history() {
        let mut state = loaded_editor_state();
        for title in ["First edit", "Second edit"] {
            state
                .set_setting(
                    "section.hero.title",
                    EditorSettingValue {
                        kind: "text".to_owned(),
                        value: serde_json::json!(title),
                        effective_value: None,
                    },
                )
                .expect("title edit schedules a preview");
        }
        state.undo().expect("second edit can be undone");
        state
            .set_setting(
                "section.hero.title",
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("Branched edit"),
                    effective_value: None,
                },
            )
            .expect("new branch edit schedules a preview");

        assert!(!state.can_redo());
        state.undo().expect("branched edit can be undone");
        assert_eq!(
            state
                .document
                .as_ref()
                .and_then(|document| document.settings_values.get("section.hero.title"))
                .map(|value| &value.value),
            Some(&serde_json::json!("First edit"))
        );
        state.redo().expect("branched edit can be redone");
        assert_eq!(
            state
                .document
                .as_ref()
                .and_then(|document| document.settings_values.get("section.hero.title"))
                .map(|value| &value.value),
            Some(&serde_json::json!("Branched edit"))
        );
    }

    #[test]
    fn keyboard_reorder_is_one_history_edit_and_escape_records_none() {
        let mut state = loaded_editor_state();
        assert!(state.start_keyboard_drag("hero"));
        state.move_section_by_id("hero", 1);
        state.move_section_by_id("hero", 1);
        assert!(!state.can_undo());
        state.finish_keyboard_drag();

        assert!(state.can_undo());
        state.undo().expect("dropped reorder is one undo step");
        assert_eq!(state.announcement, "Undid moving Hero to position 1 of 3.");
        assert_eq!(
            state.document.as_ref().map(|document| {
                document
                    .order
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            }),
            Some(vec!["hero", "features", "cta"])
        );
        state.redo().expect("dropped reorder is one redo step");
        assert_eq!(state.announcement, "Redid moving Hero to position 3 of 3.");
        assert_eq!(
            state.document.as_ref().map(|document| {
                document
                    .order
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            }),
            Some(vec!["features", "cta", "hero"])
        );

        state.start_keyboard_drag("hero");
        state.move_section_by_id("hero", -1);
        state
            .cancel_keyboard_drag()
            .expect("Escape restores the order changed during the drag");
        assert!(state.can_undo());
        state
            .undo()
            .expect("only the dropped reorder is in history");
        assert_eq!(
            state.document.as_ref().map(|document| {
                document
                    .order
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            }),
            Some(vec!["hero", "features", "cta"])
        );
        assert!(!state.can_undo());
    }

    #[test]
    fn setting_history_announcement_replaces_stale_reorder_text() {
        let mut state = loaded_editor_state();
        state
            .move_section(0, 2)
            .expect("move hero to the last position");
        assert!(state.announcement.starts_with("Moved Hero to position 3"));
        state
            .set_setting(
                "section.hero.title",
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("Updated headline"),
                    effective_value: None,
                },
            )
            .expect("headline edit schedules a preview");

        state.undo().expect("headline edit can be undone");
        assert_eq!(state.announcement, "Undid change to Headline.");
        state.redo().expect("headline edit can be redone");
        assert_eq!(state.announcement, "Redid change to Headline.");
    }

    #[test]
    fn successful_save_clears_history_and_locks_undo_while_in_flight() {
        let mut state = loaded_editor_state();
        state
            .set_setting(
                "section.hero.title",
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("Saved headline"),
                    effective_value: None,
                },
            )
            .expect("edit schedules preview");
        let capture = state.begin_save().expect("dirty editor can save");

        assert!(state.undo().is_none());
        assert!(!state.can_undo());

        let saved_values = state
            .document
            .as_ref()
            .expect("loaded editor")
            .settings_values
            .clone();
        state
            .finish_save(&capture, "revision-2".to_owned(), saved_values)
            .expect("saved revision refreshes preview");
        assert!(!state.can_undo());
        assert!(!state.can_save());
    }

    #[test]
    fn editor_history_stays_locked_for_retryable_and_stale_saves() {
        for (stale, retryable) in [(false, true), (true, false)] {
            let mut state = loaded_editor_state();
            state
                .set_setting(
                    "section.hero.title",
                    EditorSettingValue {
                        kind: "text".to_owned(),
                        value: serde_json::json!("Pending headline"),
                        effective_value: None,
                    },
                )
                .expect("edit schedules preview");
            let capture = state.begin_save().expect("dirty editor can save");
            assert!(state.fail_save(
                &capture,
                stale,
                retryable,
                "save result unavailable".to_owned()
            ));

            assert!(state.undo().is_none());
            assert!(state.redo().is_none());
            assert!(!state.can_undo());
            assert!(!state.can_redo());
        }
    }

    #[test]
    fn editor_pointer_and_keyboard_reordering_share_the_exact_order_transition() {
        let mut state = loaded_editor_state();
        let first_capture = state.move_section(0, 2).expect("pointer reorder");
        assert_eq!(first_capture.section_ids, ["features", "cta", "hero"]);
        assert!(state.start_keyboard_drag("features"));
        let keyboard_capture = state
            .move_section_by_id("features", 1)
            .expect("keyboard reorder");
        assert_eq!(keyboard_capture.section_ids, ["cta", "features", "hero"]);
        assert_eq!(state.announcement, "Moved Features to position 2 of 3.");
        let restore = state
            .cancel_keyboard_drag()
            .expect("Escape restores the pre-drag order");
        assert_eq!(restore.section_ids, ["features", "cta", "hero"]);
        assert_eq!(
            state.document.as_ref().map(|document| {
                document
                    .order
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            }),
            Some(vec!["features", "cta", "hero"])
        );
        assert!(state.announcement.contains("original order was restored"));
    }

    #[test]
    fn editor_save_waits_until_keyboard_reorder_is_dropped_or_canceled() {
        let mut state = loaded_editor_state();
        assert!(state.start_keyboard_drag("hero"));
        state.move_section_by_id("hero", 1);

        assert!(state.begin_save().is_none());
        assert!(!state.saving);

        state.finish_keyboard_drag();
        let capture = state.begin_save().expect("dropped order can be saved");
        assert!(state.saving);
        assert_eq!(capture.section_ids, ["features", "hero", "cta"]);
    }

    #[test]
    fn keyboard_cancel_cannot_invalidate_an_in_flight_save() {
        let mut state = loaded_editor_state();
        state.start_keyboard_drag("hero");
        state.move_section_by_id("hero", 1);
        let order_during_drag = state
            .document
            .as_ref()
            .expect("loaded editor")
            .order
            .clone();
        let generation_during_drag = state.generation;
        state.saving = true;

        assert!(state.cancel_keyboard_drag().is_none());
        assert_eq!(state.generation, generation_during_drag);
        assert_eq!(
            state
                .document
                .as_ref()
                .map(|document| document.order.clone()),
            Some(order_during_drag)
        );
    }

    #[test]
    fn editor_preview_discards_old_generations_and_preserves_last_good_frame_on_error() {
        let mut state = loaded_editor_state();
        let old = state.move_section(0, 1).expect("first reorder");
        let current = state.move_section(1, 2).expect("second reorder");
        state.finish_preview(&old, Ok("stale-preview".to_owned()));
        assert_eq!(
            state
                .document
                .as_ref()
                .map(|document| document.preview_html.as_str()),
            Some("preview-1")
        );
        state.finish_preview(&current, Ok("current-preview".to_owned()));
        state.finish_preview(&current, Err(()));
        assert!(state.preview_error);
        assert_eq!(
            state
                .document
                .as_ref()
                .map(|document| document.preview_html.as_str()),
            Some("current-preview")
        );
    }

    #[test]
    fn successful_editor_save_adopts_its_revision_as_a_clean_base() {
        let mut state = loaded_editor_state();
        state.move_section(0, 2);
        let capture = state.begin_save().expect("dirty editor can save");
        assert!(state.saving);
        let settings_values = state
            .document
            .as_ref()
            .expect("loaded editor")
            .settings_values
            .clone();
        let preview = state
            .finish_save(&capture, "revision-2".to_owned(), settings_values)
            .expect("preview refresh");
        assert_eq!(preview.base_revision_id, "revision-2");
        assert_eq!(preview.section_ids, ["features", "cta", "hero"]);
        assert!(!state.can_save());
    }

    #[test]
    fn schema_setting_edits_preview_and_save_without_changing_order_only_requests() {
        let mut state = loaded_editor_state();
        assert!(!state.can_save());
        state.select_section("features");
        assert_eq!(
            state
                .document
                .as_ref()
                .and_then(|document| document.selected_section.as_deref()),
            Some("features")
        );

        let first = state
            .set_setting(
                "section.hero.title",
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("New headline"),
                    effective_value: None,
                },
            )
            .expect("text edit schedules preview");
        assert!(state.can_save());
        assert_eq!(first.settings_changes.len(), 1);
        let changed_body = editor_request_body(
            &first.base_revision_id,
            &first.section_ids,
            &first.settings_changes,
        );
        let changed_body: serde_json::Value =
            serde_json::from_str(&changed_body).expect("settings request");
        assert_eq!(changed_body["base_revision_id"], first.base_revision_id);
        assert!(changed_body.get("expected_revision_id").is_none());
        assert_eq!(changed_body["settings"]["schema_version"], 1);
        assert_eq!(
            changed_body["settings"]["changes"][0]["id"],
            "section.hero.title"
        );
        assert_eq!(
            changed_body["settings"]["changes"][0]["value"]["kind"],
            "text"
        );

        let second = state
            .set_setting(
                "theme.square_buttons",
                EditorSettingValue {
                    kind: "toggle".to_owned(),
                    value: serde_json::json!(true),
                    effective_value: None,
                },
            )
            .expect("toggle edit schedules preview");
        state.finish_preview(&first, Ok("stale draft preview".to_owned()));
        assert_eq!(
            state
                .document
                .as_ref()
                .map(|document| document.preview_html.as_str()),
            Some("preview-1")
        );
        state.finish_preview(&second, Ok("current draft preview".to_owned()));
        assert_eq!(
            state
                .document
                .as_ref()
                .map(|document| document.preview_html.as_str()),
            Some("current draft preview")
        );

        let capture = state.begin_save().expect("settings-only draft can save");
        assert_eq!(capture.settings_changes.len(), 2);
        assert!(state.saving);
        let saved_values = state
            .document
            .as_ref()
            .expect("loaded editor")
            .settings_values
            .clone();
        state
            .finish_save(&capture, "revision-2".to_owned(), saved_values)
            .expect("successful settings save");
        assert!(!state.can_save());
        assert!(state
            .document
            .as_ref()
            .expect("saved editor")
            .settings_changes
            .is_empty());

        let legacy_body = editor_request_body("revision-1", &["hero".to_owned()], &[]);
        let legacy_body: serde_json::Value =
            serde_json::from_str(&legacy_body).expect("legacy request");
        assert!(legacy_body.get("settings").is_none());
    }

    #[test]
    fn preset_preview_updates_inherited_color_and_explicit_old_color_stays_dirty() {
        let mut state = loaded_editor_state();
        let preset = state
            .set_setting(
                "theme.preset",
                EditorSettingValue {
                    kind: "select".to_owned(),
                    value: serde_json::json!("bold"),
                    effective_value: None,
                },
            )
            .expect("preset edit");
        let mut preview_values = state
            .document
            .as_ref()
            .expect("loaded editor")
            .settings_values
            .clone();
        preview_values
            .get_mut("theme.primary_color")
            .expect("color field")
            .effective_value = Some("#f97316".to_owned());
        state.finish_preview_with_settings(
            &preset,
            Ok("bold preview".to_owned()),
            Some(preview_values),
        );
        let document = state.document.as_ref().expect("loaded editor");
        assert_eq!(
            document.settings_values["theme.primary_color"]
                .effective_value
                .as_deref(),
            Some("#f97316")
        );
        assert_eq!(
            document
                .settings_changes
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["theme.preset"]
        );

        let explicit_old_preset_color = state
            .set_setting(
                "theme.primary_color",
                EditorSettingValue {
                    kind: "color".to_owned(),
                    value: serde_json::json!("#334155"),
                    effective_value: None,
                },
            )
            .expect("explicit color equal to the old preset");
        assert_eq!(
            color_control_value(
                &state
                    .document
                    .as_ref()
                    .expect("loaded editor")
                    .settings_values["theme.primary_color"]
            ),
            "#334155"
        );
        assert!(explicit_old_preset_color
            .settings_changes
            .iter()
            .any(|change| {
                change.id == "theme.primary_color" && change.value.value == "#334155"
            }));
        let request = editor_request_body(
            &explicit_old_preset_color.base_revision_id,
            &explicit_old_preset_color.section_ids,
            &explicit_old_preset_color.settings_changes,
        );
        assert!(!request.contains("effective_value"));

        let reset = state
            .set_setting(
                "theme.primary_color",
                EditorSettingValue {
                    kind: "color".to_owned(),
                    value: serde_json::Value::Null,
                    effective_value: None,
                },
            )
            .expect("reset color to selected preset");
        assert_eq!(
            color_control_value(
                &state
                    .document
                    .as_ref()
                    .expect("loaded editor")
                    .settings_values["theme.primary_color"]
            ),
            "#f97316"
        );
        assert!(!reset
            .settings_changes
            .iter()
            .any(|change| change.id == "theme.primary_color"));
        let mut reset_values = state
            .document
            .as_ref()
            .expect("loaded editor")
            .settings_values
            .clone();
        reset_values
            .get_mut("theme.primary_color")
            .expect("color field")
            .effective_value = Some("#f97316".to_owned());
        state.finish_preview_with_settings(
            &reset,
            Ok("reset preview".to_owned()),
            Some(reset_values),
        );
        assert_eq!(
            state
                .document
                .as_ref()
                .expect("loaded editor")
                .settings_values["theme.primary_color"]
                .effective_value
                .as_deref(),
            Some("#f97316")
        );
    }

    #[test]
    fn definitive_client_save_error_unlocks_controls_without_retrying_same_request() {
        let mut state = loaded_editor_state();
        state
            .set_setting(
                "section.hero.title",
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("Unsaved title"),
                    effective_value: None,
                },
            )
            .expect("draft title");
        let capture = state.begin_save().expect("dirty draft can save");
        assert!(state.fail_save(&capture, false, false, "Invalid destination".to_owned()));
        assert!(!state.saving);
        assert!(!state.retryable);
        assert!(!state.stale);
        assert!(state.can_save());
        assert!(state
            .set_setting(
                "section.hero.title",
                EditorSettingValue {
                    kind: "text".to_owned(),
                    value: serde_json::json!("Corrected title"),
                    effective_value: None,
                },
            )
            .is_some());
    }

    #[test]
    fn clearing_nullable_attribution_submits_null_but_required_text_stays_empty_string() {
        let cleared = text_setting_value(String::new(), true);
        assert!(cleared.value.is_null());
        assert_eq!(text_setting_value(String::new(), false).value, "");

        let mut state = loaded_editor_state();
        let capture = state
            .set_setting("section.testimonials.items.0.attribution", cleared)
            .expect("clear nullable attribution");
        assert_eq!(capture.settings_changes.len(), 1);
        assert!(capture.settings_changes[0].value.value.is_null());
        let request = editor_request_body(
            &capture.base_revision_id,
            &capture.section_ids,
            &capture.settings_changes,
        );
        assert!(request.contains("\"value\":null"));
    }

    #[test]
    fn image_control_clears_or_updates_one_typed_value_and_preserves_other_properties() {
        let mut state = loaded_editor_state();
        let url_edit = state
            .set_image_property(
                "section.hero.image",
                "url",
                "https://example.test/image.png".to_owned(),
            )
            .expect("image URL edit");
        assert_eq!(
            url_edit.settings_changes[0].value.value["url"],
            "https://example.test/image.png"
        );
        let alt_edit = state
            .set_image_property("section.hero.image", "alt", "An offer image".to_owned())
            .expect("image alt edit");
        assert_eq!(
            alt_edit.settings_changes[0].value.value["alt"],
            "An offer image"
        );
        let cleared = state
            .set_setting(
                "section.hero.image",
                EditorSettingValue {
                    kind: "image".to_owned(),
                    value: serde_json::Value::Null,
                    effective_value: None,
                },
            )
            .expect("remove image");
        assert!(cleared.settings_changes.is_empty());
        assert!(state
            .document
            .as_ref()
            .expect("loaded editor")
            .settings_values["section.hero.image"]
            .value
            .is_null());
        assert!(!state.can_save());
    }

    #[test]
    fn old_save_responses_cannot_mutate_a_different_route_or_revision() {
        let mut state = loaded_editor_state();
        state.move_section(0, 2);
        let capture = state.begin_save().expect("dirty editor can save");
        state.begin_route("funnel-2".to_owned());
        let (settings_schema, settings_values) = fixture_settings();
        state.finish_load(
            state.route_epoch,
            "funnel-2",
            Ok(EditorReadResponse {
                funnel: EditorFunnel {
                    id: "funnel-2".to_owned(),
                    name: "Second funnel".to_owned(),
                    slug: "second".to_owned(),
                    updated_at: "now".to_owned(),
                },
                base_revision_id: "revision-20".to_owned(),
                published_revision_id: Some("revision-19".to_owned()),
                sections: vec![EditorSection {
                    id: "hero-2".to_owned(),
                    section_type: "hero".to_owned(),
                }],
                settings_schema,
                settings_values,
                preview_html: "second preview".to_owned(),
            }),
        );
        assert!(state
            .finish_save(&capture, "revision-2".to_owned(), BTreeMap::new())
            .is_none());
        assert!(!state.fail_save(&capture, false, false, "old response".to_owned()));
        assert_eq!(state.funnel_id, "funnel-2");
        assert_eq!(
            state
                .document
                .as_ref()
                .map(|document| document.base_revision_id.as_str()),
            Some("revision-20")
        );
        assert_eq!(state.save_error, None);
    }
}

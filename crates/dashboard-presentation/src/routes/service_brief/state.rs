use crate::routes::api::{ServiceBriefApiResponse, ServiceBriefSaveFailure};
use eez_domain::service_brief::{
    BriefVersion, ServiceBrief, ServiceBriefDraft, ServiceBriefValidationError,
    SERVICE_BRIEF_FORMAT_VERSION,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ServiceBriefPageState {
    funnel_id: String,
    route_epoch: u64,
    screen: ServiceBriefScreen,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ServiceBriefScreen {
    Loading,
    LoadError,
    Ready(Box<ServiceBriefEditor>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ServiceBriefEditor {
    version: BriefVersion,
    format_version: u8,
    draft: ServiceBriefDraft,
    saved_draft: ServiceBriefDraft,
    phase: SavePhase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SavePhase {
    Editing,
    Invalid,
    Saving(Box<SaveCapture>),
    Uncertain(Box<SaveCapture>),
    RefreshingConflict(Box<SaveCapture>),
    Conflict {
        attempt: Box<SaveCapture>,
        latest: Box<ServiceBriefApiResponse>,
    },
    ConflictLoadFailed(Box<SaveCapture>),
    Saved,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SaveCapture {
    pub(super) funnel_id: String,
    pub(super) route_epoch: u64,
    pub(super) expected_version: BriefVersion,
    pub(super) brief: ServiceBrief,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ServiceBriefField {
    Audience,
    Need,
    Offer,
    Scope,
    Terms,
    Action,
    PromisedNextStep,
    AvailableEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ServiceBriefPageView {
    Loading,
    LoadError,
    Ready(Box<ServiceBriefEditorView>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ServiceBriefEditorView {
    pub(super) version: BriefVersion,
    pub(super) format_version: u8,
    pub(super) draft: ServiceBriefDraft,
    pub(super) dirty: bool,
    pub(super) can_edit: bool,
    pub(super) phase: ServiceBriefSavePhaseView,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ServiceBriefSavePhaseView {
    Editing,
    Invalid,
    Saving,
    Uncertain,
    RefreshingConflict,
    Conflict {
        latest: Box<ServiceBriefApiResponse>,
    },
    ConflictLoadFailed,
    Saved,
    Rejected,
}

impl ServiceBriefPageState {
    pub(super) fn new(funnel_id: String) -> Self {
        Self {
            funnel_id,
            route_epoch: 0,
            screen: ServiceBriefScreen::Loading,
        }
    }

    pub(super) fn begin_route(&mut self, funnel_id: String) -> u64 {
        self.funnel_id = funnel_id;
        self.route_epoch = self.route_epoch.wrapping_add(1);
        self.screen = ServiceBriefScreen::Loading;
        self.route_epoch
    }

    pub(super) fn complete_load(
        &mut self,
        funnel_id: &str,
        route_epoch: u64,
        result: Result<ServiceBriefApiResponse, ()>,
    ) {
        if self.funnel_id != funnel_id || self.route_epoch != route_epoch {
            return;
        }
        self.screen = match result {
            Ok(snapshot)
                if snapshot.funnel_id == funnel_id
                    && snapshot.format_version == SERVICE_BRIEF_FORMAT_VERSION
                    && snapshot.version.value() >= 0 =>
            {
                let draft = snapshot.brief.draft();
                ServiceBriefScreen::Ready(Box::new(ServiceBriefEditor {
                    version: snapshot.version,
                    format_version: snapshot.format_version,
                    draft: draft.clone(),
                    saved_draft: draft,
                    phase: SavePhase::Editing,
                }))
            }
            _ => ServiceBriefScreen::LoadError,
        };
    }

    pub(super) fn view(&self) -> ServiceBriefPageView {
        match &self.screen {
            ServiceBriefScreen::Loading => ServiceBriefPageView::Loading,
            ServiceBriefScreen::LoadError => ServiceBriefPageView::LoadError,
            ServiceBriefScreen::Ready(editor) => {
                let phase = match &editor.phase {
                    SavePhase::Editing => ServiceBriefSavePhaseView::Editing,
                    SavePhase::Invalid => ServiceBriefSavePhaseView::Invalid,
                    SavePhase::Saving(_) => ServiceBriefSavePhaseView::Saving,
                    SavePhase::Uncertain(_) => ServiceBriefSavePhaseView::Uncertain,
                    SavePhase::RefreshingConflict(_) => {
                        ServiceBriefSavePhaseView::RefreshingConflict
                    }
                    SavePhase::Conflict { latest, .. } => ServiceBriefSavePhaseView::Conflict {
                        latest: Box::new((**latest).clone()),
                    },
                    SavePhase::ConflictLoadFailed(_) => {
                        ServiceBriefSavePhaseView::ConflictLoadFailed
                    }
                    SavePhase::Saved => ServiceBriefSavePhaseView::Saved,
                    SavePhase::Rejected => ServiceBriefSavePhaseView::Rejected,
                };
                ServiceBriefPageView::Ready(Box::new(ServiceBriefEditorView {
                    version: editor.version,
                    format_version: editor.format_version,
                    draft: editor.draft.clone(),
                    dirty: editor.draft != editor.saved_draft,
                    can_edit: !matches!(
                        &editor.phase,
                        SavePhase::Saving(_)
                            | SavePhase::Uncertain(_)
                            | SavePhase::RefreshingConflict(_)
                    ),
                    phase,
                }))
            }
        }
    }

    pub(super) fn retry_load(&mut self) -> (String, u64) {
        let funnel_id = self.funnel_id.clone();
        let route_epoch = self.begin_route(funnel_id.clone());
        (funnel_id, route_epoch)
    }

    pub(super) fn edit_text(&mut self, field: ServiceBriefField, value: String) {
        let ServiceBriefScreen::Ready(editor) = &mut self.screen else {
            return;
        };
        if matches!(
            &editor.phase,
            SavePhase::Saving(_) | SavePhase::Uncertain(_) | SavePhase::RefreshingConflict(_)
        ) {
            return;
        }
        match field {
            ServiceBriefField::Audience => editor.draft.audience = Some(value),
            ServiceBriefField::Need => editor.draft.need = Some(value),
            ServiceBriefField::Offer => editor.draft.offer = Some(value),
            ServiceBriefField::Scope => editor.draft.scope = Some(value),
            ServiceBriefField::Terms => editor.draft.terms = Some(value),
            ServiceBriefField::Action => editor.draft.action = Some(value),
            ServiceBriefField::PromisedNextStep => editor.draft.promised_next_step = Some(value),
            ServiceBriefField::AvailableEvidence => editor.draft.available_evidence = Some(value),
        }
        if !matches!(
            &editor.phase,
            SavePhase::Conflict { .. } | SavePhase::ConflictLoadFailed(_)
        ) {
            editor.phase = SavePhase::Editing;
        }
    }

    pub(super) fn set_intention(
        &mut self,
        intention: Option<eez_domain::service_brief::ServiceIntention>,
    ) {
        let ServiceBriefScreen::Ready(editor) = &mut self.screen else {
            return;
        };
        if matches!(
            &editor.phase,
            SavePhase::Saving(_) | SavePhase::Uncertain(_) | SavePhase::RefreshingConflict(_)
        ) {
            return;
        }
        editor.draft.intention = intention;
        if !matches!(
            &editor.phase,
            SavePhase::Conflict { .. } | SavePhase::ConflictLoadFailed(_)
        ) {
            editor.phase = SavePhase::Editing;
        }
    }

    pub(super) fn begin_save(
        &mut self,
    ) -> Result<Option<SaveCapture>, ServiceBriefValidationError> {
        let ServiceBriefScreen::Ready(editor) = &mut self.screen else {
            return Ok(None);
        };
        if !matches!(
            &editor.phase,
            SavePhase::Editing | SavePhase::Invalid | SavePhase::Saved | SavePhase::Rejected
        ) || editor.draft == editor.saved_draft
        {
            return Ok(None);
        }
        let brief = match ServiceBrief::from_draft(editor.draft.clone()) {
            Ok(brief) => brief,
            Err(error) => {
                editor.phase = SavePhase::Invalid;
                return Err(error);
            }
        };
        let attempt = SaveCapture {
            funnel_id: self.funnel_id.clone(),
            route_epoch: self.route_epoch,
            expected_version: editor.version,
            brief,
        };
        editor.phase = SavePhase::Saving(Box::new(attempt.clone()));
        Ok(Some(attempt))
    }

    pub(super) fn retry_uncertain_save(&mut self) -> Option<SaveCapture> {
        let ServiceBriefScreen::Ready(editor) = &mut self.screen else {
            return None;
        };
        let SavePhase::Uncertain(attempt) = &editor.phase else {
            return None;
        };
        let attempt = (**attempt).clone();
        editor.phase = SavePhase::Saving(Box::new(attempt.clone()));
        Some(attempt)
    }

    pub(super) fn complete_save(
        &mut self,
        attempt: &SaveCapture,
        result: Result<ServiceBriefApiResponse, ServiceBriefSaveFailure>,
    ) -> bool {
        if !self.current_attempt(attempt, |phase| matches!(phase, SavePhase::Saving(_))) {
            return false;
        }
        let ServiceBriefScreen::Ready(editor) = &mut self.screen else {
            return false;
        };
        match result {
            Ok(snapshot)
                if snapshot.funnel_id == attempt.funnel_id
                    && snapshot.format_version == SERVICE_BRIEF_FORMAT_VERSION
                    && snapshot.version.value() >= attempt.expected_version.value()
                    && snapshot.brief == attempt.brief =>
            {
                editor.version = snapshot.version;
                editor.format_version = snapshot.format_version;
                editor.draft = snapshot.brief.draft();
                editor.saved_draft = editor.draft.clone();
                editor.phase = SavePhase::Saved;
                false
            }
            Ok(_) | Err(ServiceBriefSaveFailure::Rejected) => {
                editor.phase = SavePhase::Rejected;
                false
            }
            Err(ServiceBriefSaveFailure::Uncertain) => {
                editor.phase = SavePhase::Uncertain(Box::new(attempt.clone()));
                false
            }
            Err(ServiceBriefSaveFailure::Conflict) => {
                editor.phase = SavePhase::RefreshingConflict(Box::new(attempt.clone()));
                true
            }
        }
    }

    pub(super) fn retry_conflict_load(&mut self) -> Option<SaveCapture> {
        let ServiceBriefScreen::Ready(editor) = &mut self.screen else {
            return None;
        };
        let SavePhase::ConflictLoadFailed(attempt) = &editor.phase else {
            return None;
        };
        let attempt = (**attempt).clone();
        editor.phase = SavePhase::RefreshingConflict(Box::new(attempt.clone()));
        Some(attempt)
    }

    pub(super) fn complete_conflict_load(
        &mut self,
        attempt: &SaveCapture,
        result: Result<ServiceBriefApiResponse, ()>,
    ) {
        if !self.current_attempt(attempt, |phase| {
            matches!(phase, SavePhase::RefreshingConflict(_))
        }) {
            return;
        }
        let ServiceBriefScreen::Ready(editor) = &mut self.screen else {
            return;
        };
        editor.phase = match result {
            Ok(latest)
                if latest.funnel_id == attempt.funnel_id
                    && latest.format_version == SERVICE_BRIEF_FORMAT_VERSION =>
            {
                SavePhase::Conflict {
                    attempt: Box::new(attempt.clone()),
                    latest: Box::new(latest),
                }
            }
            _ => SavePhase::ConflictLoadFailed(Box::new(attempt.clone())),
        };
    }

    pub(super) fn load_latest_after_conflict(&mut self) {
        let ServiceBriefScreen::Ready(editor) = &mut self.screen else {
            return;
        };
        let SavePhase::Conflict { latest, .. } = &editor.phase else {
            return;
        };
        editor.version = latest.version;
        editor.format_version = latest.format_version;
        editor.draft = latest.brief.draft();
        editor.saved_draft = editor.draft.clone();
        editor.phase = SavePhase::Editing;
    }

    pub(super) fn save_my_draft_over_latest(
        &mut self,
    ) -> Result<Option<SaveCapture>, ServiceBriefValidationError> {
        let ServiceBriefScreen::Ready(editor) = &mut self.screen else {
            return Ok(None);
        };
        let SavePhase::Conflict { latest, .. } = &editor.phase else {
            return Ok(None);
        };
        let latest = latest.clone();
        editor.version = latest.version;
        editor.format_version = latest.format_version;
        editor.saved_draft = latest.brief.draft();
        editor.phase = SavePhase::Editing;
        self.begin_save()
    }

    fn current_attempt(
        &self,
        attempt: &SaveCapture,
        matches_phase: impl FnOnce(&SavePhase) -> bool,
    ) -> bool {
        self.funnel_id == attempt.funnel_id
            && self.route_epoch == attempt.route_epoch
            && matches!(&self.screen, ServiceBriefScreen::Ready(editor) if matches_phase(&editor.phase))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ServiceBriefField, ServiceBriefPageState, ServiceBriefPageView, ServiceBriefSavePhaseView,
    };
    use crate::routes::api::{ServiceBriefApiResponse, ServiceBriefSaveFailure};
    use eez_domain::service_brief::{
        BriefVersion, ServiceBrief, ServiceBriefDraft, ServiceIntention,
        SERVICE_BRIEF_FORMAT_VERSION,
    };

    fn snapshot(funnel_id: &str, version: i32, brief: ServiceBrief) -> ServiceBriefApiResponse {
        ServiceBriefApiResponse {
            funnel_id: funnel_id.to_owned(),
            version: BriefVersion::try_from(version).expect("test version"),
            format_version: SERVICE_BRIEF_FORMAT_VERSION,
            brief,
            updated_at: None,
        }
    }

    #[test]
    fn late_brief_loads_cannot_replace_the_current_route() {
        let mut state = ServiceBriefPageState::new("funnel-a".to_owned());
        let old_epoch = state.begin_route("funnel-a".to_owned());
        let current_epoch = state.begin_route("funnel-b".to_owned());

        state.complete_load(
            "funnel-a",
            old_epoch,
            Ok(snapshot("funnel-a", 1, ServiceBrief::default())),
        );
        assert_eq!(state.view(), ServiceBriefPageView::Loading);

        state.complete_load(
            "funnel-b",
            current_epoch,
            Ok(snapshot("funnel-b", 0, ServiceBrief::default())),
        );
        assert!(matches!(state.view(), ServiceBriefPageView::Ready(_)));
    }

    #[test]
    fn uncertain_retry_reuses_the_same_version_and_saved_target() {
        let mut state = ready_state("funnel-a");
        state.set_intention(Some(ServiceIntention::Contact));
        state.edit_text(ServiceBriefField::Audience, "Local studios".to_owned());
        let attempt = state
            .begin_save()
            .expect("valid draft")
            .expect("dirty draft");

        state.complete_save(&attempt, Err(ServiceBriefSaveFailure::Uncertain));
        assert!(matches!(
            state.view(),
            ServiceBriefPageView::Ready(view)
                if view.phase == ServiceBriefSavePhaseView::Uncertain
        ));
        let retry = state.retry_uncertain_save().expect("retryable save");
        assert_eq!(retry, attempt);

        let accepted = snapshot("funnel-a", 1, attempt.brief.clone());
        state.complete_save(&retry, Ok(accepted));
        assert!(matches!(
            state.view(),
            ServiceBriefPageView::Ready(view)
                if view.version.value() == 1 && !view.dirty && view.phase == ServiceBriefSavePhaseView::Saved
        ));
    }

    #[test]
    fn a_stale_save_keeps_local_notes_until_the_owner_loads_or_replaces_them() {
        let mut state = ready_state("funnel-a");
        state.set_intention(Some(ServiceIntention::Contact));
        state.edit_text(ServiceBriefField::Audience, "My local draft".to_owned());
        let attempt = state
            .begin_save()
            .expect("valid draft")
            .expect("dirty draft");
        state.complete_save(&attempt, Err(ServiceBriefSaveFailure::Conflict));

        let latest = ServiceBrief::from_draft(ServiceBriefDraft {
            intention: Some(ServiceIntention::QuoteRequest),
            offer: Some("The newer saved offer".to_owned()),
            ..ServiceBriefDraft::default()
        })
        .expect("valid latest brief");
        state.complete_conflict_load(&attempt, Ok(snapshot("funnel-a", 2, latest.clone())));

        assert!(matches!(
            state.view(),
            ServiceBriefPageView::Ready(view)
                if view.dirty
                    && view.draft.audience.as_deref() == Some("My local draft")
                    && matches!(view.phase, ServiceBriefSavePhaseView::Conflict { ref latest } if latest.version.value() == 2)
        ));

        let replacement = state
            .save_my_draft_over_latest()
            .expect("valid local brief")
            .expect("explicit overwrite request");
        assert_eq!(replacement.expected_version.value(), 2);
        assert_eq!(replacement.brief.audience(), Some("My local draft"));
        assert_eq!(replacement.brief.offer(), None);
    }

    #[test]
    fn loading_the_latest_conflict_value_replaces_the_local_draft_explicitly() {
        let mut state = ready_state("funnel-a");
        state.edit_text(ServiceBriefField::Need, "Local note".to_owned());
        let attempt = state
            .begin_save()
            .expect("valid draft")
            .expect("dirty draft");
        state.complete_save(&attempt, Err(ServiceBriefSaveFailure::Conflict));
        let latest = ServiceBrief::from_draft(ServiceBriefDraft {
            need: Some("Latest server note".to_owned()),
            ..ServiceBriefDraft::default()
        })
        .expect("valid latest brief");
        state.complete_conflict_load(&attempt, Ok(snapshot("funnel-a", 1, latest)));

        state.load_latest_after_conflict();
        assert!(matches!(
            state.view(),
            ServiceBriefPageView::Ready(view)
                if !view.dirty && view.draft.need.as_deref() == Some("Latest server note")
        ));
    }

    #[test]
    fn failed_load_can_be_retried_against_the_same_funnel() {
        let mut state = ServiceBriefPageState::new("funnel-a".to_owned());
        let first_epoch = state.begin_route("funnel-a".to_owned());
        state.complete_load("funnel-a", first_epoch, Err(()));

        let (funnel_id, retry_epoch) = state.retry_load();
        assert_eq!(funnel_id, "funnel-a");
        assert_ne!(retry_epoch, first_epoch);
        state.complete_load(
            "funnel-a",
            retry_epoch,
            Ok(snapshot("funnel-a", 0, ServiceBrief::default())),
        );
        assert!(matches!(state.view(), ServiceBriefPageView::Ready(_)));
    }

    #[test]
    fn failed_conflict_refresh_can_be_retried_without_discarding_the_draft() {
        let mut state = ready_state("funnel-a");
        state.edit_text(ServiceBriefField::Need, "Keep this local note".to_owned());
        let attempt = state
            .begin_save()
            .expect("valid draft")
            .expect("dirty draft");
        assert!(state.complete_save(&attempt, Err(ServiceBriefSaveFailure::Conflict)));
        state.complete_conflict_load(&attempt, Err(()));

        let retry = state.retry_conflict_load().expect("refresh is retryable");
        assert_eq!(retry, attempt);
        assert!(matches!(
            state.view(),
            ServiceBriefPageView::Ready(view)
                if view.dirty && view.draft.need.as_deref() == Some("Keep this local note")
                    && view.phase == ServiceBriefSavePhaseView::RefreshingConflict
        ));
    }

    #[test]
    fn every_private_context_field_is_saved_in_the_versioned_brief() {
        let mut state = ready_state("funnel-a");
        for field in [
            ServiceBriefField::Audience,
            ServiceBriefField::Need,
            ServiceBriefField::Offer,
            ServiceBriefField::Scope,
            ServiceBriefField::Terms,
            ServiceBriefField::Action,
            ServiceBriefField::PromisedNextStep,
            ServiceBriefField::AvailableEvidence,
        ] {
            state.edit_text(field, format!("note-{field:?}"));
        }
        let attempt = state
            .begin_save()
            .expect("valid draft")
            .expect("dirty draft");

        assert_eq!(attempt.brief.audience(), Some("note-Audience"));
        assert_eq!(attempt.brief.need(), Some("note-Need"));
        assert_eq!(attempt.brief.offer(), Some("note-Offer"));
        assert_eq!(attempt.brief.scope(), Some("note-Scope"));
        assert_eq!(attempt.brief.terms(), Some("note-Terms"));
        assert_eq!(attempt.brief.action(), Some("note-Action"));
        assert_eq!(
            attempt.brief.promised_next_step(),
            Some("note-PromisedNextStep")
        );
        assert_eq!(
            attempt.brief.available_evidence(),
            Some("note-AvailableEvidence")
        );
    }

    #[test]
    fn rejected_save_stays_visible_and_does_not_mark_the_brief_saved() {
        let mut state = ready_state("funnel-a");
        state.edit_text(ServiceBriefField::Offer, "Consulting".to_owned());
        let attempt = state
            .begin_save()
            .expect("valid draft")
            .expect("dirty draft");
        state.complete_save(&attempt, Err(ServiceBriefSaveFailure::Rejected));

        assert!(matches!(
            state.view(),
            ServiceBriefPageView::Ready(view)
                if view.dirty && view.phase == ServiceBriefSavePhaseView::Rejected
        ));
    }

    fn ready_state(funnel_id: &str) -> ServiceBriefPageState {
        let mut state = ServiceBriefPageState::new(funnel_id.to_owned());
        let epoch = state.begin_route(funnel_id.to_owned());
        state.complete_load(
            funnel_id,
            epoch,
            Ok(snapshot(funnel_id, 0, ServiceBrief::default())),
        );
        state
    }
}

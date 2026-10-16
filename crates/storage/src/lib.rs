#![forbid(unsafe_code)]

use eez_domain::ThemePreset;
use repository::{
    MutationResult, NewFunnel, NewRevision, NewTheme, PublicationMutation, RepositoryError,
};
use worker::d1::D1Type;
use worker::{Env, Error, Result};

pub mod repository;
pub mod stage;

pub const MAX_PUBLIC_FORM_SUBMISSION_BYTES: usize = 512 * 1024;

pub struct PublicationRegressionObservations {
    pub cross_funnel_published_revision_insert_accepted: bool,
    pub null_published_revision_insert_accepted: bool,
    pub unsupported_schema_publish_accepted: bool,
    pub unsupported_schema_published_revision_id: Option<String>,
    pub current_revision_noop_accepted: bool,
    pub publication_baseline_cas_successes: usize,
    pub distinct_candidate_cas_successes: usize,
    pub publication_replay_interleavings_safe: bool,
}

pub struct Bindings {
    database: worker::D1Database,
    bucket: worker::Bucket,
    cache: worker::kv::KvStore,
    stage_capacity: Option<stage::StageCapacity>,
}

impl Bindings {
    pub fn from_env(env: &Env) -> Result<Self> {
        Ok(Self {
            database: env.d1("DB")?,
            bucket: env.bucket("R2")?,
            cache: env.kv("KV")?,
            stage_capacity: stage::StageCapacity::from_env(env),
        })
    }

    pub async fn verify_local_round_trip(&self, key: &str, value: &str) -> Result<()> {
        let operation = self.verify_values(key, value).await;
        let cleanup = self.clear_probe(key).await;

        match operation {
            Err(error) => Err(error),
            Ok(()) => cleanup,
        }
    }

    pub fn repository(&self) -> repository::Repository<'_> {
        repository::Repository::new(&self.database)
    }

    pub fn document_stage_repository(&self) -> stage::DocumentStageRepository<'_> {
        stage::DocumentStageRepository::new(&self.database, &self.bucket, self.stage_capacity)
    }

    pub async fn verify_product_repository_round_trip(
        &self,
    ) -> Result<PublicationRegressionObservations> {
        let result = self.run_product_repository_checks().await;
        let repository = self.repository();
        let funnel_cleanup = repository.delete_funnel("pr003-smoke-funnel").await;
        let theme_cleanup = repository.delete_theme("pr003-smoke-theme").await;
        let receipt_cleanup = self
            .database
            .prepare(
                "DELETE FROM command_receipts WHERE idempotency_key IN ('pr003-replay-publish','pr003-replay-unpublish','pr003-replay-publish-later')",
            )
            .run()
            .await;

        match result {
            Err(error) => Err(error),
            Ok(observations) => {
                funnel_cleanup?;
                theme_cleanup?;
                receipt_cleanup?;
                Ok(observations)
            }
        }
    }

    pub async fn prepare_publication_race(&self) -> Result<Vec<String>> {
        let repository = self.repository();
        repository
            .create_funnel(&NewFunnel {
                id: "pr003-race-funnel".to_owned(),
                name: "PR-003 Publication Race".to_owned(),
                slug: "pr003-publication-race".to_owned(),
            })
            .await?;

        let mut revision_ids = Vec::with_capacity(100);
        for index in 0..100 {
            let revision_id = format!("pr003-race-revision-{index:03}");
            repository
                .create_revision(&NewRevision {
                    id: revision_id.clone(),
                    funnel_id: "pr003-race-funnel".to_owned(),
                    schema_version: 1,
                    data_json: "{\"schema_version\":1,\"sections\":[]}".to_owned(),
                    render_manifest_json: "{}".to_owned(),
                })
                .await?;
            revision_ids.push(revision_id);
        }
        let over_limit = repository
            .create_revision(&NewRevision {
                id: "pr003-race-revision-100".to_owned(),
                funnel_id: "pr003-race-funnel".to_owned(),
                schema_version: 1,
                data_json: "{\"schema_version\":1,\"sections\":[]}".to_owned(),
                render_manifest_json: "{}".to_owned(),
            })
            .await;
        if !matches!(over_limit, Err(RepositoryError::RevisionLimitReached)) {
            return Err(Error::RustError(
                "revision retention limit was not enforced".to_owned(),
            ));
        }
        Ok(revision_ids)
    }

    pub async fn publish_race_candidate(
        &self,
        revision_id: &str,
        expected_published_revision_id: Option<&str>,
    ) -> Result<bool> {
        self.repository()
            .publish_revision(
                "pr003-race-funnel",
                revision_id,
                expected_published_revision_id,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn publication_race_winner(&self) -> Result<Option<String>> {
        let funnel = self
            .repository()
            .get_funnel("pr003-race-funnel")
            .await?
            .ok_or_else(|| Error::RustError("publication race funnel missing".to_owned()))?;
        Ok(funnel.published_revision_id)
    }

    pub async fn cleanup_publication_race(&self) -> Result<()> {
        self.repository().delete_funnel("pr003-race-funnel").await?;
        Ok(())
    }

    async fn run_product_repository_checks(&self) -> Result<PublicationRegressionObservations> {
        let repository = self.repository();
        let seeded_themes = repository.list_themes().await?;
        let expected_theme_ids = ["bold", "corporate", "elegant", "minimal", "playful"];
        let actual_theme_ids = seeded_themes
            .iter()
            .map(|theme| theme.id.as_str())
            .collect::<Vec<_>>();
        if seeded_themes.len() != expected_theme_ids.len()
            || expected_theme_ids
                .iter()
                .any(|expected| !actual_theme_ids.contains(expected))
            || seeded_themes
                .iter()
                .filter(|theme| theme.is_default == 1)
                .count()
                != 1
        {
            return Err(Error::RustError("product theme seed mismatch".to_owned()));
        }

        for preset in ThemePreset::all() {
            let tokens_json = preset
                .tokens_json()
                .map_err(|_| Error::RustError("preset theme serialization failed".to_owned()))?;
            let Some(theme) = seeded_themes.iter().find(|theme| theme.id == preset.id()) else {
                return Err(Error::RustError("product theme seed mismatch".to_owned()));
            };
            if theme.name != preset.name() || theme.tokens_json != tokens_json {
                return Err(Error::RustError("product theme preset mismatch".to_owned()));
            }
        }

        let created_theme = repository
            .create_theme(&NewTheme {
                id: "pr003-smoke-theme".to_owned(),
                name: "PR-003 Smoke Theme".to_owned(),
                tokens_json: "{\"palette\":\"test\"}".to_owned(),
                is_default: false,
            })
            .await?;
        if created_theme.id != "pr003-smoke-theme" {
            return Err(Error::RustError("theme create/read mismatch".to_owned()));
        }
        let updated_theme = repository
            .update_theme(
                &created_theme.id,
                "PR-003 Updated Theme",
                "{\"palette\":\"updated\"}",
                false,
            )
            .await?;
        if updated_theme.as_ref().map(|theme| theme.name.as_str()) != Some("PR-003 Updated Theme") {
            return Err(Error::RustError("theme update mismatch".to_owned()));
        }
        if !repository.delete_theme(&created_theme.id).await? {
            return Err(Error::RustError("theme delete mismatch".to_owned()));
        }

        let created_funnel = repository
            .create_funnel(&NewFunnel {
                id: "pr003-smoke-funnel".to_owned(),
                name: "PR-003 Smoke Funnel".to_owned(),
                slug: "pr003-smoke-funnel".to_owned(),
            })
            .await?;
        if repository.get_funnel(&created_funnel.id).await?.is_none()
            || !repository
                .list_funnels()
                .await?
                .iter()
                .any(|funnel| funnel.id == created_funnel.id)
        {
            return Err(Error::RustError(
                "funnel create/read/list mismatch".to_owned(),
            ));
        }

        let unsupported = repository
            .create_revision(&NewRevision {
                id: "pr003-unsupported-revision".to_owned(),
                funnel_id: created_funnel.id.clone(),
                schema_version: 3,
                data_json: "{}".to_owned(),
                render_manifest_json: "{}".to_owned(),
            })
            .await;
        if !matches!(
            unsupported,
            Err(RepositoryError::UnsupportedSchemaVersion(3))
        ) {
            return Err(Error::RustError(
                "unsupported schema version was not refused".to_owned(),
            ));
        }

        let missing_funnel = repository
            .create_revision(&NewRevision {
                id: "pr003-missing-funnel-revision".to_owned(),
                funnel_id: "pr003-missing-funnel".to_owned(),
                schema_version: 1,
                data_json: "{}".to_owned(),
                render_manifest_json: "{}".to_owned(),
            })
            .await;
        if !matches!(missing_funnel, Err(RepositoryError::FunnelNotFound)) {
            return Err(Error::RustError(
                "revision for missing funnel was not refused".to_owned(),
            ));
        }

        let persisted_unsupported_id = D1Type::Text("pr003-persisted-unsupported-revision");
        let persisted_unsupported_funnel_id = D1Type::Text(&created_funnel.id);
        let unsupported_data = D1Type::Text("{}");
        self.database
            .prepare("INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) VALUES (?, ?, 3, ?, ?)")
            .bind_refs([
                &persisted_unsupported_id,
                &persisted_unsupported_funnel_id,
                &unsupported_data,
                &unsupported_data,
            ])?
            .run()
            .await?;
        if !matches!(
            repository
                .get_revision("pr003-persisted-unsupported-revision")
                .await,
            Err(RepositoryError::UnsupportedSchemaVersion(3))
        ) {
            return Err(Error::RustError(
                "persisted unsupported schema version was not refused".to_owned(),
            ));
        }
        let unsupported_schema_publish_accepted = repository
            .publish_revision(
                &created_funnel.id,
                "pr003-persisted-unsupported-revision",
                None,
            )
            .await?;
        let unsupported_schema_published_revision_id = repository
            .get_funnel(&created_funnel.id)
            .await?
            .and_then(|funnel| funnel.published_revision_id);
        let funnel_id = D1Type::Text(&created_funnel.id);
        self.database
            .prepare("UPDATE funnels SET published_revision_id = NULL WHERE id = ?")
            .bind_refs([&funnel_id])?
            .run()
            .await?;
        self.database
            .prepare("DELETE FROM revisions WHERE id = ?")
            .bind_refs([&persisted_unsupported_id])?
            .run()
            .await?;

        let invalid_json = repository
            .create_revision(&NewRevision {
                id: "pr003-invalid-json-revision".to_owned(),
                funnel_id: created_funnel.id.clone(),
                schema_version: 1,
                data_json: "{".to_owned(),
                render_manifest_json: "{}".to_owned(),
            })
            .await;
        if !matches!(invalid_json, Err(RepositoryError::InvalidJson)) {
            return Err(Error::RustError(
                "invalid revision JSON was accepted".to_owned(),
            ));
        }

        let first_revision = repository
            .create_revision(&NewRevision {
                id: "pr003-smoke-revision-1".to_owned(),
                funnel_id: created_funnel.id.clone(),
                schema_version: 1,
                data_json: "{\"schema_version\":1,\"sections\":[]}".to_owned(),
                render_manifest_json: "{}".to_owned(),
            })
            .await?;
        if repository.get_revision(&first_revision.id).await?.as_ref() != Some(&first_revision)
            || repository.list_revisions(&created_funnel.id).await?.len() != 1
        {
            return Err(Error::RustError(
                "revision create/read/list mismatch".to_owned(),
            ));
        }
        let revision_id = D1Type::Text(&first_revision.id);
        let replacement_data =
            D1Type::Text("{\"schema_version\":1,\"sections\":[{\"id\":\"mutated\"}]}");
        let attempted_mutation = self
            .database
            .prepare("UPDATE revisions SET data_json = ? WHERE id = ?")
            .bind_refs([&replacement_data, &revision_id])?
            .run()
            .await;
        if attempted_mutation.is_ok() {
            return Err(Error::RustError(
                "revision snapshot was mutable after creation".to_owned(),
            ));
        }

        if !repository
            .publish_revision(&created_funnel.id, &first_revision.id, None)
            .await?
            || repository
                .publish_revision(&created_funnel.id, &first_revision.id, None)
                .await?
        {
            return Err(Error::RustError(
                "conditional publication mismatch".to_owned(),
            ));
        }
        let published_funnel = repository
            .get_funnel(&created_funnel.id)
            .await?
            .ok_or_else(|| Error::RustError("published funnel missing".to_owned()))?;
        if published_funnel.published_revision_id.as_deref() != Some(&first_revision.id) {
            return Err(Error::RustError(
                "published revision pointer mismatch".to_owned(),
            ));
        }
        if repository.delete_revision(&first_revision.id).await? {
            return Err(Error::RustError(
                "published revision was deleted".to_owned(),
            ));
        }

        let current_revision_noop_accepted = repository
            .publish_revision(
                &created_funnel.id,
                &first_revision.id,
                Some(&first_revision.id),
            )
            .await?;
        let pointer_after_noop = repository
            .get_funnel(&created_funnel.id)
            .await?
            .and_then(|funnel| funnel.published_revision_id);
        if pointer_after_noop.as_deref() != Some(&first_revision.id) {
            return Err(Error::RustError(
                "no-op publication changed the published revision".to_owned(),
            ));
        }

        let second_revision = repository
            .create_revision(&NewRevision {
                id: "pr003-smoke-revision-2".to_owned(),
                funnel_id: created_funnel.id.clone(),
                schema_version: 1,
                data_json: "{\"schema_version\":1,\"sections\":[{\"id\":\"new\"}]}".to_owned(),
                render_manifest_json: "{}".to_owned(),
            })
            .await?;
        let third_revision = repository
            .create_revision(&NewRevision {
                id: "pr003-smoke-revision-3".to_owned(),
                funnel_id: created_funnel.id.clone(),
                schema_version: 1,
                data_json: "{\"schema_version\":1,\"sections\":[{\"id\":\"other\"}]}".to_owned(),
                render_manifest_json: "{}".to_owned(),
            })
            .await?;
        let first_candidate_won = repository
            .publish_revision(
                &created_funnel.id,
                &second_revision.id,
                Some(&first_revision.id),
            )
            .await?;
        let second_candidate_won = repository
            .publish_revision(
                &created_funnel.id,
                &third_revision.id,
                Some(&first_revision.id),
            )
            .await?;
        let distinct_candidate_cas_successes =
            usize::from(first_candidate_won) + usize::from(second_candidate_won);
        let publication_baseline_cas_successes =
            usize::from(current_revision_noop_accepted) + distinct_candidate_cas_successes;

        let funnel_id = D1Type::Text(&created_funnel.id);
        self.database
            .prepare("UPDATE funnels SET published_revision_id = NULL WHERE id = ?")
            .bind_refs([&funnel_id])?
            .run()
            .await?;
        let publish_payload = serde_json::json!({
            "funnel_id": created_funnel.id,
            "revision_id": second_revision.id,
            "expected_published_revision_id": null,
        })
        .to_string();
        let publish_result = serde_json::json!({
            "command_id": "funnel.publish",
            "funnel_id": created_funnel.id,
            "published_revision_id": second_revision.id,
        })
        .to_string();
        let unpublish_payload = serde_json::json!({
            "funnel_id": created_funnel.id,
            "expected_published_revision_id": second_revision.id,
        })
        .to_string();
        let unpublish_result = serde_json::json!({
            "command_id": "funnel.unpublish",
            "funnel_id": created_funnel.id,
            "published_revision_id": null,
            "unpublished": true,
        })
        .to_string();
        let publish = PublicationMutation {
            funnel_id: &created_funnel.id,
            published_revision_id: Some(&second_revision.id),
            expected_published_revision_id: None,
            command_id: "funnel.publish",
            idempotency_key: "pr003-replay-publish",
            payload_json: &publish_payload,
            result_json: &publish_result,
        };
        if !matches!(
            repository.set_published_revision_idempotently(&publish).await?,
            MutationResult::Stored(result) if result == publish_result
        ) {
            return Err(Error::RustError(
                "initial idempotent publication did not store its receipt".to_owned(),
            ));
        }
        let unpublish = PublicationMutation {
            funnel_id: &created_funnel.id,
            published_revision_id: None,
            expected_published_revision_id: Some(&second_revision.id),
            command_id: "funnel.unpublish",
            idempotency_key: "pr003-replay-unpublish",
            payload_json: &unpublish_payload,
            result_json: &unpublish_result,
        };
        if !matches!(
            repository
                .set_published_revision_idempotently(&unpublish)
                .await?,
            MutationResult::Stored(result) if result == unpublish_result
        ) {
            return Err(Error::RustError(
                "initial idempotent unpublication did not store its receipt".to_owned(),
            ));
        }
        let late_publish_replay = repository
            .set_published_revision_idempotently(&publish)
            .await?;
        let pointer_after_late_publish = repository
            .get_funnel(&created_funnel.id)
            .await?
            .and_then(|funnel| funnel.published_revision_id);
        if !matches!(late_publish_replay, MutationResult::Stored(result) if result == publish_result)
            || pointer_after_late_publish.is_some()
        {
            return Err(Error::RustError(
                "late publication replay reapplied an old pointer mutation".to_owned(),
            ));
        }
        let later_publish_payload = serde_json::json!({
            "funnel_id": created_funnel.id,
            "revision_id": second_revision.id,
            "expected_published_revision_id": null,
        })
        .to_string();
        let later_publish = PublicationMutation {
            funnel_id: &created_funnel.id,
            published_revision_id: Some(&second_revision.id),
            expected_published_revision_id: None,
            command_id: "funnel.publish",
            idempotency_key: "pr003-replay-publish-later",
            payload_json: &later_publish_payload,
            result_json: &publish_result,
        };
        if !matches!(
            repository
                .set_published_revision_idempotently(&later_publish)
                .await?,
            MutationResult::Stored(result) if result == publish_result
        ) {
            return Err(Error::RustError(
                "later publication after replay test did not succeed".to_owned(),
            ));
        }
        let late_unpublish_replay = repository
            .set_published_revision_idempotently(&unpublish)
            .await?;
        let pointer_after_late_unpublish = repository
            .get_funnel(&created_funnel.id)
            .await?
            .and_then(|funnel| funnel.published_revision_id);
        if !matches!(late_unpublish_replay, MutationResult::Stored(result) if result == unpublish_result)
            || pointer_after_late_unpublish.as_deref() != Some(&second_revision.id)
        {
            return Err(Error::RustError(
                "late unpublication replay reapplied an old pointer mutation".to_owned(),
            ));
        }
        let publication_replay_interleavings_safe = true;

        if repository.get_revision(&first_revision.id).await?.is_none()
            || !repository.delete_revision(&first_revision.id).await?
        {
            return Err(Error::RustError(
                "revision replacement or retention mismatch".to_owned(),
            ));
        }

        let other_funnel = repository
            .create_funnel(&NewFunnel {
                id: "pr003-other-funnel".to_owned(),
                name: "PR-003 Other Funnel".to_owned(),
                slug: "pr003-other-funnel".to_owned(),
            })
            .await?;
        let null_published_revision_insert_accepted = repository
            .get_funnel(&other_funnel.id)
            .await?
            .is_some_and(|funnel| funnel.published_revision_id.is_none());
        let cross_funnel_id = D1Type::Text("pr003-cross-funnel-insert");
        let cross_funnel_name = D1Type::Text("PR-003 Cross Funnel Insert");
        let cross_funnel_slug = D1Type::Text("pr003-cross-funnel-insert");
        let published_revision_id = D1Type::Text(&second_revision.id);
        let cross_funnel_published_revision_insert_accepted = self
            .database
            .prepare(
                "INSERT INTO funnels (id, name, slug, published_revision_id) VALUES (?, ?, ?, ?)",
            )
            .bind_refs([
                &cross_funnel_id,
                &cross_funnel_name,
                &cross_funnel_slug,
                &published_revision_id,
            ])?
            .run()
            .await
            .is_ok();
        self.database
            .prepare("DELETE FROM funnels WHERE id = ?")
            .bind_refs([&cross_funnel_id])?
            .run()
            .await?;
        let foreign_revision = repository
            .create_revision(&NewRevision {
                id: "pr003-foreign-revision".to_owned(),
                funnel_id: other_funnel.id.clone(),
                schema_version: 1,
                data_json: "{\"schema_version\":1,\"sections\":[]}".to_owned(),
                render_manifest_json: "{}".to_owned(),
            })
            .await?;
        if repository
            .publish_revision(
                &created_funnel.id,
                &foreign_revision.id,
                Some(&second_revision.id),
            )
            .await?
            || repository
                .get_funnel(&created_funnel.id)
                .await?
                .and_then(|funnel| funnel.published_revision_id)
                .as_deref()
                != Some(&second_revision.id)
            || !repository.delete_funnel(&other_funnel.id).await?
        {
            return Err(Error::RustError(
                "cross-funnel revision publication was not refused".to_owned(),
            ));
        }

        let updated_funnel = repository
            .update_funnel(&created_funnel.id, "PR-003 Updated Funnel", "pr003-updated")
            .await?;
        if updated_funnel.as_ref().map(|funnel| funnel.slug.as_str()) != Some("pr003-updated") {
            return Err(Error::RustError("funnel update mismatch".to_owned()));
        }

        Ok(PublicationRegressionObservations {
            cross_funnel_published_revision_insert_accepted,
            null_published_revision_insert_accepted,
            unsupported_schema_publish_accepted,
            unsupported_schema_published_revision_id,
            current_revision_noop_accepted,
            publication_baseline_cas_successes,
            distinct_candidate_cas_successes,
            publication_replay_interleavings_safe,
        })
    }

    async fn verify_values(&self, key: &str, value: &str) -> Result<()> {
        let key_argument = D1Type::Text(key);
        let value_argument = D1Type::Text(value);
        self.database
            .prepare(
                "INSERT OR REPLACE INTO pr002_runtime_probe (probe_key, probe_value) VALUES (?, ?)",
            )
            .bind_refs([&key_argument, &value_argument])?
            .run()
            .await?;

        let stored_value = self
            .database
            .prepare("SELECT probe_value FROM pr002_runtime_probe WHERE probe_key = ?")
            .bind_refs([&key_argument])?
            .first::<String>(Some("probe_value"))
            .await?;
        if stored_value.as_deref() != Some(value) {
            return Err(Error::RustError("D1 round-trip mismatch".to_owned()));
        }

        self.bucket.put(key, value.to_owned()).execute().await?;
        let object = self
            .bucket
            .get(key)
            .execute()
            .await?
            .ok_or_else(|| Error::RustError("R2 probe value missing".to_owned()))?;
        let object_body = object
            .body()
            .ok_or_else(|| Error::RustError("R2 probe body missing".to_owned()))?;
        if object_body.text().await? != value {
            return Err(Error::RustError("R2 round-trip mismatch".to_owned()));
        }

        self.cache.put(key, value)?.execute().await?;
        let cached_value = self
            .cache
            .get(key)
            .text()
            .await?
            .ok_or_else(|| Error::RustError("KV probe value missing".to_owned()))?;
        if cached_value != value {
            return Err(Error::RustError("KV round-trip mismatch".to_owned()));
        }

        Ok(())
    }

    async fn clear_probe(&self, key: &str) -> Result<()> {
        let key_argument = D1Type::Text(key);
        self.database
            .prepare("DELETE FROM pr002_runtime_probe WHERE probe_key = ?")
            .bind_refs([&key_argument])?
            .run()
            .await?;
        self.bucket.delete(key).await?;
        self.cache.delete(key).await?;
        Ok(())
    }
}

#[cfg(test)]
mod theme_seed_tests;

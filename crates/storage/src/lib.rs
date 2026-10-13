#![forbid(unsafe_code)]

use worker::d1::D1Type;
use worker::{Env, Error, Result};

pub struct Bindings {
    database: worker::D1Database,
    bucket: worker::Bucket,
    cache: worker::kv::KvStore,
}

impl Bindings {
    pub fn from_env(env: &Env) -> Result<Self> {
        Ok(Self {
            database: env.d1("DB")?,
            bucket: env.bucket("R2")?,
            cache: env.kv("KV")?,
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

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Storage configuration for the application.
///
/// Mirrors [`crate::config::CacheConfig`]: a `kind`-tagged enum selects the
/// driver, and the operator picks it from YAML instead of wiring a
/// [`crate::storage::drivers::StoreDriver`] by hand in
/// [`crate::app::Hooks::after_context`]. That override path still works —
/// `after_context` runs after this config produces the default store, and
/// can freely replace it.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "kind")]
#[non_exhaustive]
pub enum StorageConfig {
    #[cfg(feature = "storage_aws_s3")]
    /// AWS S3 storage
    Aws(AwsStorageConfig),
    #[cfg(feature = "storage_gcp")]
    /// Google Cloud Storage
    Gcp(GcpStorageConfig),
    #[cfg(feature = "storage_azure")]
    /// Azure Blob storage
    Azure(AzureStorageConfig),
    /// Local filesystem storage
    Local(LocalStorageConfig),
    /// Null storage: every operation errors. The default when no `storage:`
    /// block is present, matching today's hardcoded behavior.
    #[default]
    Null,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalStorageConfig {
    /// Root directory for stored files. Defaults to the process working
    /// directory when omitted.
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AwsStorageConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GcpStorageConfig {
    pub bucket: String,
    pub credential_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AzureStorageConfig {
    pub container: String,
    pub account_name: String,
    pub access_key: String,
    pub endpoint: String,
}

/// Config for a secondary [`crate::storage::Storage`] with several named
/// stores, read from `initializers.storage` and wired in automatically by
/// [`crate::initializers::storage::MultiStorageInitializer`] — no
/// `after_context` code required. See [`crate::storage::create_multi_storage_provider`].
///
/// ```yaml
/// initializers:
///   storage:
///     stores:
///       primary: { kind: Aws, bucket: my-app-uploads, region: us-east-1 }
///       exports: { kind: Local, path: storage/exports }
///     strategy:
///       kind: Single
///       default: primary
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MultiStorageConfig {
    /// Every named store this `Storage` can address, keyed by the name
    /// passed to `ctx.storage.as_store(name)`. A name doesn't have to be
    /// referenced by `strategy` at all — an entry with no role in the
    /// strategy is still reachable by name, just never touched by an
    /// unqualified `storage.upload()`/`.download()` call or by mirror/backup
    /// fan-out. That's how a plain "extra bucket to pull from" is modeled:
    /// give it a name here and nothing else.
    pub stores: HashMap<String, StorageConfig>,
    pub strategy: StorageStrategyConfig,
}

/// Config for a [`crate::storage::strategies::StorageStrategy`].
///
/// A `Storage` has exactly one `strategy` field — so this selects, at most,
/// one primary/secondaries fan-out group per `MultiStorageConfig`. Stores
/// not named by `default`/`primary`/`secondaries` are still in the `stores`
/// map and still reachable via `as_store`, just outside the strategy.
///
/// Two independent mirror/backup groups in a single app need two separate
/// `Storage` instances (two `MultiStorageConfig` entries under different
/// initializer keys) — there's no single-`Storage` config shape for that,
/// because there's no single-`Storage` *code* shape for that either.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind")]
pub enum StorageStrategyConfig {
    /// No fan-out: every unqualified `storage.upload()`/`.download()` call
    /// targets `default`. Every other entry in `stores` is still directly
    /// reachable via `as_store(name)`.
    Single { default: String },
    /// [`crate::storage::strategies::replicated::ReplicatedStrategy::mirror`]:
    /// writes fan out to every secondary; reads fall back to a secondary
    /// when the primary fails or (for `exists`/`list`) reports a miss.
    Mirror {
        primary: String,
        #[serde(default)]
        secondaries: Vec<String>,
        #[serde(default)]
        failure_policy: crate::storage::strategies::replicated::FailurePolicy,
    },
    /// [`crate::storage::strategies::replicated::ReplicatedStrategy::backup`]:
    /// writes fan out the same as `Mirror`, but reads are always served from
    /// `primary` only.
    Backup {
        primary: String,
        #[serde(default)]
        secondaries: Vec<String>,
        #[serde(default)]
        failure_policy: crate::storage::strategies::replicated::FailurePolicy,
    },
}

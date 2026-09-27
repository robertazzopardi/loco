use std::sync::Arc;

use async_trait::async_trait;
use axum::{Extension, Router as AxumRouter};

use crate::{
    app::{AppContext, Initializer},
    storage, Error, Result,
};

/// Wires a second, named-store [`storage::Storage`] from `initializers.storage`
/// onto the router as `Extension<Arc<storage::Storage>>` — the storage-side
/// equivalent of [`crate::initializers::multi_db::MultiDbInitializer`].
///
/// Unlike `after_context`-based storage setup, this never touches
/// `ctx.storage`: the app's primary store (from the top-level `storage:` key,
/// or an `after_context` override) is untouched, and this initializer adds a
/// second, independent, multi-store `Storage` alongside it, extracted in a
/// handler with `Extension(storage): Extension<Arc<storage::Storage>>`.
#[allow(clippy::module_name_repetitions)]
pub struct MultiStorageInitializer;

#[async_trait]
impl Initializer for MultiStorageInitializer {
    fn name(&self) -> String {
        "storage".to_string()
    }

    async fn after_routes(&self, router: AxumRouter, ctx: &AppContext) -> Result<AxumRouter> {
        let cfg = ctx
            .config
            .initializers
            .as_ref()
            .and_then(|settings| settings.get("storage"))
            .ok_or_else(|| Error::Message("storage initializer not configured".to_string()))?;

        let cfg: crate::config::MultiStorageConfig =
            serde::Deserialize::deserialize(cfg).map_err(|err: serde_json::Error| {
                Error::Message(format!("invalid storage initializer config: {err}"))
            })?;
        let multi_storage = storage::create_multi_storage_provider(&cfg)?;

        Ok(router.layer(Extension(Arc::new(multi_storage))))
    }
}

#[cfg(test)]
mod tests {
    use axum::{extract::Extension, routing::get};
    use tower::ServiceExt;

    use super::*;
    use crate::{config::Initializers, storage::Storage, tests_cfg};

    fn storage_initializer_settings() -> serde_json::Value {
        serde_json::json!({
            "storage": {
                "stores": {
                    "avatars": { "kind": "Local", "path": null },
                    "exports": { "kind": "Local", "path": null },
                },
                "strategy": { "kind": "Single", "default": "avatars" },
            }
        })
    }

    async fn probe(Extension(storage): Extension<Arc<Storage>>) -> String {
        let mut names: Vec<_> = storage.stores.keys().cloned().collect();
        names.sort();
        names.join(",")
    }

    /// `after_routes` reads `initializers.storage`, builds the multi-store
    /// `Storage`, and layers it as `Extension<Arc<Storage>>` — extractable in
    /// a handler without any `after_context` code in the app.
    #[tokio::test]
    async fn wires_extension_with_every_named_store() {
        let mut ctx = tests_cfg::app::get_app_context().await;
        let raw: Initializers = serde_json::from_value(storage_initializer_settings())
            .expect("valid initializer settings");
        ctx.config.initializers = Some(raw);

        let router = axum::Router::new().route("/probe", get(probe));
        let router = MultiStorageInitializer
            .after_routes(router, &ctx)
            .await
            .expect("initializer builds the extension");

        let req = axum::http::Request::builder()
            .uri("/probe")
            .method("GET")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = router.oneshot(req).await.unwrap();
        assert_eq!(response.status(), 200);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "avatars,exports");
    }

    /// Absent `initializers.storage` is a clear config error, not a silent
    /// empty `Storage` — there's nothing sensible to default to here, unlike
    /// the top-level `storage:` key's `Null` default.
    #[tokio::test]
    async fn missing_config_is_an_error() {
        let ctx = tests_cfg::app::get_app_context().await;
        let router = axum::Router::new();

        let err = MultiStorageInitializer
            .after_routes(router, &ctx)
            .await
            .expect_err("no initializers.storage configured");
        assert!(err.to_string().contains("not configured"));
    }
}

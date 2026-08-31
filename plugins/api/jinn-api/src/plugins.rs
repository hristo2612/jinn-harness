//! The plugins seam ON the operator surface: the routes, the catalog
//! list's schema, and the pure mapping of the plugins seam's typed error
//! onto this seam's. The catalog CONTRACT itself is not restated here —
//! its one home is `plugins/plugins/jinn-plugins/README.md`; this module
//! only says how an operator reaches it over a transport.
//!
//! Two parameters, as the todos and workflows surfaces have: a CATALOG
//! and, within it, one plugin. The catalog is in the path because a
//! composition holds several at once (one contract name per catalog id),
//! so an operator addresses the catalog they mean rather than the API
//! guessing a default.
//!
//! # `history` is a RESERVED path segment
//!
//! A plugin is addressed at `/v1/plugins/{catalog}/{id}` and its ledger
//! lines at `/v1/plugins/{catalog}/{id}/history`. The rule is stated once,
//! here: a THIRD segment of exactly `history` is always the history, and
//! a plugin id may not contain a `/`, so the two shapes cannot collide.

use jinn_plugins::{
    catalog_contract, ErrorCode as CatalogErrorCode, PluginsError, OP_DESCRIBE,
    OP_DESCRIBE_CATALOG, OP_HISTORY, OP_LIST,
};
use serde::{Deserialize, Serialize};

use crate::{ApiError, ErrorCode, Extensions, API_VERSION};

/// The plugins surface's path prefix.
pub const PLUGINS_PATH: &str = "/v1/plugins";

/// The path segment reserved for a plugin's ledger lines.
pub const HISTORY_SEGMENT: &str = "history";

/// The methods the plugins surface answers. A path this table shapes
/// under another method is a method refusal, not a route miss.
pub const PLUGIN_METHODS: [&str; 1] = ["GET"];

/// One plugins route: which operation the path names, in which catalog.
/// `Catalogs` is the only one that is not a call on a provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginRoute {
    /// `GET /v1/plugins` — every catalog this API may route to.
    Catalogs,
    /// `GET /v1/plugins/{catalog}`
    List { catalog: String },
    /// `GET /v1/plugins/{catalog}/{id}`
    Describe { catalog: String, plugin: String },
    /// `GET /v1/plugins/{catalog}/{id}/history`
    History { catalog: String, plugin: String },
}

impl PluginRoute {
    /// The catalog a call route addresses.
    #[must_use]
    pub fn catalog(&self) -> Option<&str> {
        match self {
            Self::Catalogs => None,
            Self::List { catalog }
            | Self::Describe { catalog, .. }
            | Self::History { catalog, .. } => Some(catalog),
        }
    }

    /// The contract operation a call route names.
    #[must_use]
    pub fn operation(&self) -> Option<&'static str> {
        match self {
            Self::Catalogs => None,
            Self::List { .. } => Some(OP_LIST),
            Self::Describe { .. } => Some(OP_DESCRIBE),
            Self::History { .. } => Some(OP_HISTORY),
        }
    }

    /// The plugin id a route names, where it names one.
    #[must_use]
    pub fn plugin(&self) -> Option<&str> {
        match self {
            Self::Describe { plugin, .. } | Self::History { plugin, .. } => Some(plugin),
            _ => None,
        }
    }
}

/// Whether a path belongs to this surface at all.
#[must_use]
pub fn is_plugins_path(path: &str) -> bool {
    path == PLUGINS_PATH || path.starts_with(&format!("{PLUGINS_PATH}/"))
}

/// Matches a method + path against this surface.
#[must_use]
pub fn plugin_route(method: &str, path: &str) -> Option<PluginRoute> {
    if method != "GET" || !is_plugins_path(path) {
        return None;
    }
    if path == PLUGINS_PATH {
        return Some(PluginRoute::Catalogs);
    }
    let rest = path.strip_prefix(&format!("{PLUGINS_PATH}/"))?;
    let segments: Vec<&str> = rest.split('/').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return None;
    }
    match segments.as_slice() {
        [catalog] => Some(PluginRoute::List {
            catalog: (*catalog).to_owned(),
        }),
        [catalog, plugin] => Some(PluginRoute::Describe {
            catalog: (*catalog).to_owned(),
            plugin: (*plugin).to_owned(),
        }),
        [catalog, plugin, HISTORY_SEGMENT] => Some(PluginRoute::History {
            catalog: (*catalog).to_owned(),
            plugin: (*plugin).to_owned(),
        }),
        _ => None,
    }
}

/// The request payload one route carries.
#[must_use]
pub fn plugin_payload(route: &PluginRoute, mut document: serde_json::Value) -> serde_json::Value {
    if let Some(plugin) = route.plugin() {
        document["plugin-id"] = serde_json::Value::String(plugin.to_owned());
    }
    document
}

/// The catalog contract name, when this API is granted it. A catalog the
/// profile did not grant is a 404 WITHOUT a kernel call — the API never
/// probes for authority it does not hold.
///
/// # Errors
///
/// [`ApiError`] naming the catalog that is not routable.
pub fn plugin_catalog_routable(catalogs: &[String], catalog: &str) -> Result<String, ApiError> {
    if catalogs.iter().any(|granted| granted == catalog) {
        Ok(catalog_contract(catalog))
    } else {
        Err(ApiError::new(
            ErrorCode::NotFound,
            format!("no catalog {catalog:?} on this API"),
        ))
    }
}

/// One catalog on the operator surface: what its provider says about
/// itself, or the typed reason it could not say.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginCatalogEntry {
    pub catalog: String,
    /// The contract name it is served under — what a profile edit swaps.
    pub contract: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub describe: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `GET /v1/plugins` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginCatalogList {
    pub api_version: String,
    pub catalogs: Vec<PluginCatalogEntry>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The catalog list, from each catalog's own `describe-catalog`.
pub fn plugin_catalog_list<I>(described: I) -> PluginCatalogList
where
    I: IntoIterator<Item = (String, Result<serde_json::Value, ApiError>)>,
{
    PluginCatalogList {
        api_version: API_VERSION.to_owned(),
        catalogs: described
            .into_iter()
            .map(|(catalog, answer)| {
                let contract = catalog_contract(&catalog);
                let (describe, error) = match answer {
                    Ok(value) => (Some(value), None),
                    Err(error) => (None, Some(error)),
                };
                PluginCatalogEntry {
                    catalog,
                    contract,
                    describe,
                    error,
                    extra: Extensions::new(),
                }
            })
            .collect(),
        extra: Extensions::new(),
    }
}

/// The operation a catalog's own `describe` is asked for.
pub const OP_CATALOG_DESCRIBE: &str = OP_DESCRIBE_CATALOG;

/// The plugins seam's typed error as this seam's. The catalog's own code
/// rides along verbatim as `catalog-code` (additive) so `failed` is never
/// mistaken for `refused` by an operator reading the answer, and the
/// CONTRACT a read refused on rides along too — the difference between
/// "there is nothing" and "I could not look" is this seam's whole point,
/// and it must survive the mapping.
#[must_use]
pub fn plugin_api_error(error: &PluginsError) -> ApiError {
    let code = match error.code {
        CatalogErrorCode::Invalid => ErrorCode::Invalid,
        CatalogErrorCode::NotFound => ErrorCode::NotFound,
        CatalogErrorCode::Refused | CatalogErrorCode::Failed => ErrorCode::Refused,
        CatalogErrorCode::Unavailable => ErrorCode::Unavailable,
    };
    let mut mapped = ApiError::new(code, error.message.clone());
    if let Ok(catalog_code) = serde_json::to_value(error.code) {
        mapped.extra.insert("catalog-code".into(), catalog_code);
    }
    if let Some(contract) = error.extra.get("contract") {
        mapped.extra.insert("contract".into(), contract.clone());
    }
    mapped
}

/// One catalog answer decoded into this seam's outcome.
///
/// # Errors
///
/// [`ApiError`] for a malformed answer or the catalog's typed refusal.
pub fn decode_plugin_answer(bytes: &[u8]) -> Result<serde_json::Value, ApiError> {
    let answer = jinn_plugins::Answer::decode(bytes);
    match answer.outcome {
        jinn_plugins::Outcome::Ok(value) => Ok(value),
        jinn_plugins::Outcome::Error(error) => Err(plugin_api_error(&error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_shapes_parse_and_history_is_reserved() {
        assert_eq!(
            plugin_route("GET", "/v1/plugins"),
            Some(PluginRoute::Catalogs)
        );
        assert_eq!(
            plugin_route("GET", "/v1/plugins/main"),
            Some(PluginRoute::List {
                catalog: "main".to_owned()
            })
        );
        assert_eq!(
            plugin_route("GET", "/v1/plugins/main/jinn-status"),
            Some(PluginRoute::Describe {
                catalog: "main".to_owned(),
                plugin: "jinn-status".to_owned()
            })
        );
        assert_eq!(
            plugin_route("GET", "/v1/plugins/main/jinn-status/history"),
            Some(PluginRoute::History {
                catalog: "main".to_owned(),
                plugin: "jinn-status".to_owned()
            })
        );
    }

    #[test]
    fn a_path_this_surface_does_not_shape_is_not_a_route() {
        for path in [
            "/v1/plugins/",
            "/v1/plugins//x",
            "/v1/plugins/main/x/y",
            "/v1/plugins/main/x/history/z",
            "/v1/pluginsX",
            "/v1/status",
        ] {
            assert_eq!(plugin_route("GET", path), None, "{path}");
        }
        // A write method is a METHOD refusal on a shaped path, never a
        // route: this surface is read-only, and says so by shape.
        assert!(is_plugins_path("/v1/plugins/main"));
        assert_eq!(plugin_route("POST", "/v1/plugins/main"), None);
    }

    #[test]
    fn a_catalog_this_api_was_not_granted_is_a_404_without_a_kernel_call() {
        let granted = vec!["main".to_owned()];
        assert_eq!(
            plugin_catalog_routable(&granted, "main").expect("granted"),
            "jinn:plugins.main"
        );
        assert_eq!(
            plugin_catalog_routable(&granted, "appliance")
                .expect_err("not granted")
                .code,
            ErrorCode::NotFound
        );
    }

    #[test]
    fn an_unreadable_read_keeps_its_contract_all_the_way_out() {
        let mapped = plugin_api_error(&PluginsError::unreadable("jinn:profile", "no grant"));
        assert_eq!(mapped.code, ErrorCode::Unavailable);
        assert_eq!(mapped.extra["contract"], "jinn:profile");
        assert_eq!(mapped.extra["catalog-code"], "unavailable");
    }
}

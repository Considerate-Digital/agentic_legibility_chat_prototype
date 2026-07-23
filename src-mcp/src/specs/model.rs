//! Data types that represent a parsed markdown spec.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Categorisation of a markdown spec file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    Endpoint,
    Service,
    Plan,
}

impl DocKind {
    /// Sub-directory under `LIVE_RESOURCES_DIR` where docs of this kind live.
    pub fn subdir(self) -> &'static str {
        match self {
            DocKind::Endpoint => "endpoints",
            DocKind::Service => "services",
            DocKind::Plan => "plans",
        }
    }
}

/// A reference to an endpoint parsed from a service or plan body.
///
/// Service bodies (per `specs/service.md`) use the form
/// `1. <id>, <name>, <department>`. Plan bodies (per
/// `specs/service_plan.md`) add a trailing `required` or `optional` flag.
/// The parser is intentionally lenient: any line that doesn't match the
/// expected shape is skipped with a warning rather than aborting the scan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointRef {
    /// Endpoint file stem (e.g. `getDrivingLicence`) — used as the
    /// primary key to resolve this reference against the endpoint index.
    pub name: String,
    /// Endpoint frontmatter `id` (UUID), if the line included one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Department, if the line included one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    /// `Some(true)` for `required`, `Some(false)` for `optional`, `None`
    /// for services (where the field is absent) or unparseable plans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// YAML front-matter block at the top of a spec markdown file.
///
/// Unknown keys are preserved via `#[serde(default)]` semantics on the
/// `extra` map below. We use a flat `Frontmatter` struct because the
/// `endpoint`, `service`, and `plan` documents in this repo share most keys.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Frontmatter {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// For endpoints: the department that owns the API. Singular.
    #[serde(default)]
    pub department: Option<String>,
    /// For services and plans: the owning departments. Plural.
    #[serde(default)]
    pub departments: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    /// `draft | published | deprecated` for services/plans.
    #[serde(default)]
    pub status: Option<String>,
    /// HTTP method (`GET | POST | PUT | PATCH | DELETE`) for endpoints.
    #[serde(default)]
    pub method: Option<String>,
    /// Endpoint URL (distinct from `path` for the eventual rest of the build).
    #[serde(default)]
    pub endpoint: Option<String>,
    /// For plans: `route | plan`.
    #[serde(default)]
    pub r#type: Option<String>,
    /// For plans: a policy/legislation reference.
    #[serde(default)]
    pub policy_ref: Option<String>,
    /// For plans: comma-separated IDs of source routes.
    #[serde(default)]
    pub source_routes: Option<String>,
    /// Free-form tags (the publishing tools may add these).
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A single loaded spec document.
#[derive(Debug, Clone)]
pub struct Doc {
    /// Kind of document, inferred from the directory it was loaded from.
    pub kind: DocKind,
    /// Legacy field retained for compatibility with callers that still
    /// pass a service slug to look up an endpoint. With the flat-endpoint
    /// layout, this is **always `None` for endpoints** — ownership is
    /// derived from the service/plan body references, not the directory
    /// tree. Kept on the struct so the existing call sites continue to
    /// compile.
    pub service: Option<String>,
    /// File stem, e.g. `hello` for `hello.md`. Used as a primary identifier.
    pub name: String,
    /// Path relative to `LIVE_RESOURCES_DIR`, using forward slashes.
    pub rel_path: PathBuf,
    /// Parsed front-matter.
    pub frontmatter: Frontmatter,
    /// Markdown body, with the front-matter block stripped.
    pub body: String,
    /// Full raw document text with body and frontmatter combined.
    pub raw: String,
    /// Endpoint references parsed out of the body. Only populated for
    /// `Service` and `Plan` docs; empty for `Endpoint` docs.
    pub endpoint_refs: Vec<EndpointRef>,
}

impl Doc {
    /// Returns a display name for this document, preferring the `name` from
    /// front-matter and falling back to the file stem.
    pub fn display_name(&self) -> &str {
        self.frontmatter
            .name
            .as_deref()
            .unwrap_or(self.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_parses_endpoint_keys() {
        let yaml = r#"
id: abc-123
name: Driving licence check
endpoint: https://api.example.com/check
description: Verify a UK driving licence
department: DVLA
owner: alice
method: GET
tags:
  - driving
  - licence
"#;
        let fm: Frontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(fm.id.as_deref(), Some("abc-123"));
        assert_eq!(fm.method.as_deref(), Some("GET"));
        assert_eq!(fm.tags, vec!["driving", "licence"]);
    }

    #[test]
    fn frontmatter_parses_plan_keys() {
        let yaml = r#"
id: plan-1
name: Apply for a passport
type: plan
status: draft
departments: HMPO, GDS
policy_ref: "https://example.com/policy"
source_routes: route-1, route-2
"#;
        let fm: Frontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(fm.r#type.as_deref(), Some("plan"));
        assert_eq!(fm.status.as_deref(), Some("draft"));
        assert_eq!(fm.policy_ref.as_deref(), Some("https://example.com/policy"));
    }

    #[test]
    fn frontmatter_handles_missing_fields() {
        let fm: Frontmatter = serde_yaml::from_str("").unwrap();
        assert!(fm.id.is_none());
        assert!(fm.tags.is_empty());
    }

    #[test]
    fn endpoint_ref_serializes_with_optional_fields_omitted() {
        let r = EndpointRef {
            name: "getFoo".to_string(),
            id: None,
            department: None,
            required: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#"{"name":"getFoo"}"#);
    }
}

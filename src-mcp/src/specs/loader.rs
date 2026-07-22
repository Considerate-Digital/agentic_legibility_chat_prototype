//! Loader that scans `LIVE_RESOURCES_DIR` and produces an in-memory `SpecIndex`.
//!
//! Layout (relative to `LIVE_RESOURCES_DIR`):
//!
//! - `endpoints/<endpoint>.md` — endpoint docs are **flat** (no service
//!   subdirectory). Ownership is recovered from the service and plan
//!   bodies at scan time.
//! - `services/<service>.md` — service docs live at the top of the
//!   `services/` subdir. Their body holds a numbered list of endpoint
//!   references in the form `<id>, <name>, <department>`.
//! - `plans/<plan>.md` — plan docs live at the top of the `plans/`
//!   subdir. Their body holds the same shape with an optional trailing
//!   `required` or `optional` flag.
//!
//! Missing `services/`, `endpoints/`, or `plans/` subdirs are not fatal:
//! the loader just yields an empty list for that kind and logs a warning.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;
use tokio::sync::RwLock;
use walkdir::WalkDir;

use super::model::{Doc, DocKind, EndpointRef, Frontmatter};

/// In-memory index of all spec documents plus a back-map of
/// endpoint-name → the services and plans that reference it.
#[derive(Debug, Default, Clone)]
pub struct SpecIndex {
    docs: Vec<Doc>,
    /// `endpoint_name → set of service names that reference it`.
    service_to_endpoints: BTreeMap<String, BTreeSet<String>>,
    /// `endpoint_name → set of plan names that reference it`.
    plan_to_endpoints: BTreeMap<String, BTreeSet<String>>,
}

impl SpecIndex {
    /// Build an empty index (used as a placeholder before the first scan).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Synchronously scan `live_resources_dir` and return a fresh index.
    ///
    /// This is the workhorse used by both the startup path and the
    /// background rescan task. It is intentionally synchronous and pure so
    /// the integration tests can call it on a `tempfile::TempDir`.
    pub fn scan(live_resources_dir: &Path) -> Result<Self> {
        let mut docs = Vec::new();

        for kind in [DocKind::Endpoint, DocKind::Service, DocKind::Plan] {
            let sub = live_resources_dir.join(kind.subdir());
            if !sub.exists() {
                tracing::warn!(
                    "specs subdir '{}' does not exist; loading zero docs of kind {:?}",
                    sub.display(),
                    kind
                );
                continue;
            }
            scan_kind(&sub, kind, &mut docs)
                .with_context(|| format!("failed to scan {}", sub.display()))?;
        }

        Ok(Self::from_docs(docs))
    }

    /// Build the cross-reference back-maps from a pre-loaded `Vec<Doc>`.
    /// Exposed so the unit tests can construct an index without going
    /// through the filesystem.
    pub fn from_docs(docs: Vec<Doc>) -> Self {
        let mut service_to_endpoints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut plan_to_endpoints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for doc in &docs {
            let target = match doc.kind {
                DocKind::Service => Some(&mut service_to_endpoints),
                DocKind::Plan => Some(&mut plan_to_endpoints),
                DocKind::Endpoint => None,
            };
            let Some(map) = target else { continue };

            for r in &doc.endpoint_refs {
                map.entry(r.name.clone())
                    .or_default()
                    .insert(doc.name.clone());
            }
        }

        Self {
            docs,
            service_to_endpoints,
            plan_to_endpoints,
        }
    }

    /// Return all documents of a given kind.
    pub fn list(&self, kind: DocKind) -> Vec<&Doc> {
        self.docs.iter().filter(|d| d.kind == kind).collect()
    }

    /// Look up a document by kind and file-stem name. The `service`
    /// argument is retained for backward compatibility but **ignored for
    /// endpoints** under the flat layout: endpoints are looked up by
    /// `name` alone.
    pub fn get(&self, kind: DocKind, _service: Option<&str>, name: &str) -> Option<&Doc> {
        self.docs.iter().find(|d| d.kind == kind && d.name == name)
    }

    /// Return the service document for `name` plus all the endpoints it
    /// references (resolved through the cross-reference index).
    pub fn for_service(&self, name: &str) -> Vec<&Doc> {
        let mut out: Vec<&Doc> = Vec::new();
        if let Some(svc) = self.docs.iter().find(|d| d.kind == DocKind::Service && d.name == name) {
            out.push(svc);
        }
        out.extend(self.endpoints_for_service(name).into_iter());
        out
    }

    /// All services, useful for resource listing.
    pub fn services(&self) -> Vec<&Doc> {
        self.list(DocKind::Service)
    }

    /// All plans, useful for resource listing.
    pub fn plans(&self) -> Vec<&Doc> {
        self.list(DocKind::Plan)
    }

    /// All endpoints, optionally filtered by a case-insensitive substring
    /// across the endpoint's service slug, file stem, HTTP method, path
    /// (`frontmatter.endpoint`), and front-matter `name`.
    pub fn endpoints(&self, filter: Option<&str>) -> Vec<&Doc> {
        let needle = filter.map(|s| s.to_lowercase());
        self.docs
            .iter()
            .filter(|d| d.kind == DocKind::Endpoint)
            .filter(|d| match needle.as_deref() {
                None => true,
                Some(n) => {
                    let mut hay = String::new();
                    hay.push_str(&d.name.to_lowercase());
                    hay.push('\n');
                    hay.push_str(&d.display_name().to_lowercase());
                    hay.push('\n');
                    if let Some(m) = &d.frontmatter.method {
                        hay.push_str(&m.to_lowercase());
                        hay.push('\n');
                    }
                    if let Some(p) = &d.frontmatter.endpoint {
                        hay.push_str(&p.to_lowercase());
                        hay.push('\n');
                    }
                    // Legacy "service slug" was the parent dir name; with
                    // the flat layout it is always `None`, but we still
                    // expose it so filter-by-service-legacy keeps working
                    // when callers pass a service slug that used to map
                    // 1:1 to endpoints.
                    if let Some(s) = &d.service {
                        hay.push_str(&s.to_lowercase());
                        hay.push('\n');
                    }
                    hay.contains(n)
                }
            })
            .collect()
    }

    /// Endpoints that the service `name` references, resolved through the
    /// cross-reference index. Order matches the order in which the
    /// references appear in the service body, with duplicates removed.
    pub fn endpoints_for_service(&self, name: &str) -> Vec<&Doc> {
        let Some(svc) = self
            .docs
            .iter()
            .find(|d| d.kind == DocKind::Service && d.name == name)
        else {
            return Vec::new();
        };
        resolve_refs(&self.docs, &svc.endpoint_refs)
    }

    /// Endpoints that the plan `name` references, resolved through the
    /// cross-reference index. Order matches the order in which the
    /// references appear in the plan body, with duplicates removed.
    pub fn endpoints_for_plan(&self, name: &str) -> Vec<&Doc> {
        let Some(plan) = self
            .docs
            .iter()
            .find(|d| d.kind == DocKind::Plan && d.name == name)
        else {
            return Vec::new();
        };
        resolve_refs(&self.docs, &plan.endpoint_refs)
    }

    /// Services that reference the endpoint `name` (i.e. the endpoint is
    /// listed in the service body).
    pub fn services_for_endpoint(&self, name: &str) -> Vec<&Doc> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        self.service_to_endpoints
            .get(name)
            .map(|set| set.iter().map(String::as_str).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|svc_name| {
                if !seen.insert(svc_name) {
                    return None;
                }
                self.docs
                    .iter()
                    .find(|d| d.kind == DocKind::Service && d.name == svc_name)
            })
            .collect()
    }

    /// Plans that reference the endpoint `name` (i.e. the endpoint is
    /// listed in the plan body).
    pub fn plans_for_endpoint(&self, name: &str) -> Vec<&Doc> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        self.plan_to_endpoints
            .get(name)
            .map(|set| set.iter().map(String::as_str).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|plan_name| {
                if !seen.insert(plan_name) {
                    return None;
                }
                self.docs
                    .iter()
                    .find(|d| d.kind == DocKind::Plan && d.name == plan_name)
            })
            .collect()
    }

    /// Number of endpoint references in `name`'s body that resolve to an
    /// actual endpoint in the index. Used by the service/plan summaries.
    pub fn resolved_endpoint_count(&self, name: &str) -> usize {
        let Some(doc) = self
            .docs
            .iter()
            .find(|d| (d.kind == DocKind::Service || d.kind == DocKind::Plan) && d.name == name)
        else {
            return 0;
        };
        doc.endpoint_refs
            .iter()
            .filter(|r| {
                self.docs
                    .iter()
                    .any(|d| d.kind == DocKind::Endpoint && d.name == r.name)
            })
            .count()
    }

    /// Total number of documents currently indexed.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// All documents. Used by the search fallback.
    pub fn iter(&self) -> impl Iterator<Item = &Doc> {
        self.docs.iter()
    }
}

/// Resolve a list of `EndpointRef`s against the endpoint index, in
/// declaration order, deduplicating and warning on unresolved names.
fn resolve_refs<'a>(docs: &'a [Doc], refs: &'a [EndpointRef]) -> Vec<&'a Doc> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for r in refs {
        if !seen.insert(r.name.as_str()) {
            continue;
        }
        match docs
            .iter()
            .find(|d| d.kind == DocKind::Endpoint && d.name == r.name)
        {
            Some(d) => out.push(d),
            None => {
                tracing::warn!(
                    endpoint = %r.name,
                    id = r.id.as_deref().unwrap_or(""),
                    "endpoint reference does not resolve to a known endpoint"
                );
            }
        }
    }
    out
}

fn scan_kind(root: &Path, kind: DocKind, out: &mut Vec<Doc>) -> Result<()> {
    match kind {
        DocKind::Endpoint => {
            // Flat layout: endpoints live as <name>.md directly under
            // `endpoints/`. We still allow deeper nesting in case callers
            // mix layouts, but a `service` is only set when a single-level
            // subdir is present so the legacy `get(Endpoint, Some(s), n)`
            // shape keeps working. Pure top-level files have `service=None`
            // and the body-parser is the only thing that links them to a
            // service or plan.
            for entry in WalkDir::new(root)
                .min_depth(1)
                .max_depth(3)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if !entry.file_type().is_file() {
                    continue;
                }
                if entry.path().extension().and_then(|s| s.to_str()) != Some("md") {
                    continue;
                }
                if entry
                    .path()
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with('.'))
                    .unwrap_or(false)
                {
                    continue;
                }
                let rel = entry
                    .path()
                    .strip_prefix(root.parent().unwrap_or(root))
                    .unwrap_or(entry.path())
                    .to_path_buf();
                // Depth relative to `endpoints/`: 0 = the file itself,
                // 1 = one subdir deep, etc.
                let depth = entry
                    .path()
                    .strip_prefix(root)
                    .map(|p| p.components().count().saturating_sub(1))
                    .unwrap_or(0);
                let service = if depth == 1 {
                    // Legacy nested layout: <root>/<service>/<endpoint>.md
                    entry
                        .path()
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                };
                let name = entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unnamed")
                    .to_string();
                let doc = load_doc(entry.path(), kind, service, name, rel)?;
                out.push(doc);
            }
        }
        DocKind::Service | DocKind::Plan => {
            for entry in WalkDir::new(root)
                .min_depth(1)
                .max_depth(2)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if !entry.file_type().is_file() {
                    continue;
                }
                if entry.path().extension().and_then(|s| s.to_str()) != Some("md") {
                    continue;
                }
                if entry
                    .path()
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with('.'))
                    .unwrap_or(false)
                {
                    continue;
                }
                let rel = entry
                    .path()
                    .strip_prefix(root.parent().unwrap_or(root))
                    .unwrap_or(entry.path())
                    .to_path_buf();
                let name = entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unnamed")
                    .to_string();
                let mut doc = load_doc(entry.path(), kind, None, name, rel)?;
                if kind == DocKind::Service || kind == DocKind::Plan {
                    doc.endpoint_refs = parse_endpoint_refs(&doc.body);
                }
                out.push(doc);
            }
        }
    }
    Ok(())
}

fn load_doc(
    path: &Path,
    kind: DocKind,
    service: Option<String>,
    name: String,
    rel_path: PathBuf,
) -> Result<Doc> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read failed for {}", path.display()))?;
    let (frontmatter, body) = split_frontmatter(&raw);
    Ok(Doc {
        kind,
        service,
        name,
        rel_path,
        frontmatter,
        body,
        endpoint_refs: Vec::new(),
    })
}

/// Parse the numbered endpoint list out of a service or plan body.
///
/// Lines are expected to look like one of:
///
/// - `<n>. <id>, <name>, <department>`
/// - `<n>. <id>, <name>, <department>, required`
/// - `<n>. <id>, <name>, <department>, optional`
///
/// where `<n>` is a positive integer followed by `.` or `)`. Lines that
/// don't match are silently skipped (the body is plain markdown and may
/// contain unrelated prose or headings). The parser is intentionally
/// lenient — see the unit tests in this module for the accepted shapes.
pub(crate) fn parse_endpoint_refs(body: &str) -> Vec<EndpointRef> {
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        // Match `<n>. ` or `<n>) ` at the start.
        let Some(rest) = strip_number_prefix(trimmed) else {
            continue;
        };
        let parts: Vec<&str> = rest.split(',').map(|p| p.trim()).collect();
        // We need at least id and name.
        if parts.len() < 2 {
            continue;
        }
        let id = parts[0];
        let name = parts[1];
        if id.is_empty() || name.is_empty() {
            continue;
        }
        let department = parts
            .get(2)
            .filter(|d| !d.is_empty())
            .map(|d| d.to_string());
        let required = parts.get(3).and_then(|flag| match flag.to_lowercase().as_str() {
            "required" => Some(true),
            "optional" => Some(false),
            _ => None,
        });
        out.push(EndpointRef {
            name: name.to_string(),
            id: Some(id.to_string()),
            department,
            required,
        });
    }
    out
}

fn strip_number_prefix(line: &str) -> Option<&str> {
    // Find first non-digit character.
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let after_digits = &line[i..];
    let after_sep = after_digits
        .strip_prefix('.')
        .or_else(|| after_digits.strip_prefix(')'))?;
    let after_ws = after_sep.trim_start();
    Some(after_ws)
}

/// Splits a markdown file into `(frontmatter, body)`. If no frontmatter is
/// present, returns `(Frontmatter::default(), raw)`. A malformed YAML
/// frontmatter is logged and treated as empty so the loader never crashes
/// on a single bad file.
pub(crate) fn split_frontmatter(raw: &str) -> (Frontmatter, String) {
    let trimmed = raw.trim_start_matches('\u{feff}');
    let rest = trimmed.strip_prefix("---\n").or_else(|| {
        if trimmed.starts_with("---\r\n") {
            Some(&trimmed[5..])
        } else {
            None
        }
    });
    let Some(after_open) = rest else {
        return (Frontmatter::default(), raw.to_string());
    };
    // Find the closing `---` on its own line.
    let mut lines = after_open.split_inclusive('\n');
    let mut yaml = String::new();
    let mut found_close = false;
    let mut body_start = 0usize;
    let mut cursor: usize = trimmed.len() - after_open.len();
    while let Some(line) = lines.next() {
        let is_close = line.trim() == "---" || line.trim() == "---";
        if is_close {
            found_close = true;
            // Skip the newline after `---`.
            body_start = cursor + line.len();
            break;
        } else {
            yaml.push_str(line);
            cursor += line.len();
        }
    }
    if !found_close {
        // Unterminated frontmatter block — treat whole file as body.
        return (Frontmatter::default(), raw.to_string());
    }
    let body = trimmed[body_start..].to_string();
    let fm: Frontmatter = match serde_yaml::from_str(&yaml) {
        Ok(fm) => fm,
        Err(err) => {
            tracing::warn!("malformed YAML frontmatter: {err}; treating as empty");
            Frontmatter::default()
        }
    };
    (fm, body)
}

/// Handle to a running rescan task. Drop it to detach (the task continues
/// until process exit, since it lives in the runtime).
pub struct LoaderHandle {
    _rescan: tokio::task::JoinHandle<()>,
}

/// Spawn a background task that rescans `live_resources_dir` every `interval` and
/// swaps the new index into the shared `Arc<RwLock<SpecIndex>>`.
pub fn spawn_rescan_loop(
    state: Arc<RwLock<SpecIndex>>,
    live_resources_dir: PathBuf,
    interval: Duration,
) -> LoaderHandle {
    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // First tick fires immediately; skip it to avoid double work on startup.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match SpecIndex::scan(&live_resources_dir) {
                Ok(new_index) => {
                    let count = new_index.len();
                    let mut guard = state.write().await;
                    *guard = new_index;
                    tracing::info!("rescanned specs: {count} docs indexed");
                }
                Err(err) => {
                    tracing::error!("specs rescan failed: {err:#}");
                }
            }
        }
    });
    LoaderHandle { _rescan: handle }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    #[test]
    fn scans_flat_service_dir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("services"),
            "foo.md",
            "---\nname: Foo service\nstatus: published\n---\nbody text",
        );
        let idx = SpecIndex::scan(root).unwrap();
        assert_eq!(idx.len(), 1);
        let s = &idx.list(DocKind::Service)[0];
        assert_eq!(s.name, "foo");
        assert_eq!(s.frontmatter.name.as_deref(), Some("Foo service"));
        assert_eq!(s.body, "body text");
    }

    #[test]
    fn scans_flat_endpoint_dir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("endpoints"),
            "getFoo.md",
            "---\nname: Get Foo\nmethod: GET\n---\napi body",
        );
        let idx = SpecIndex::scan(root).unwrap();
        assert_eq!(idx.len(), 1);
        let e = &idx.list(DocKind::Endpoint)[0];
        assert_eq!(e.name, "getFoo");
        assert!(e.service.is_none(), "flat endpoints have no service slug");
        assert_eq!(e.frontmatter.method.as_deref(), Some("GET"));
    }

    #[test]
    fn scans_legacy_nested_endpoint_dir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("endpoints").join("driver"),
            "licence.md",
            "---\nname: Driving licence check\nmethod: GET\n---\napi body",
        );
        let idx = SpecIndex::scan(root).unwrap();
        assert_eq!(idx.len(), 1);
        let e = &idx.list(DocKind::Endpoint)[0];
        assert_eq!(e.name, "licence");
        assert_eq!(e.service.as_deref(), Some("driver"));
    }

    #[test]
    fn missing_subdirs_are_tolerated() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Only create services/ — endpoints/ and plans/ are absent.
        write(
            &root.join("services"),
            "x.md",
            "---\nname: X\n---\nbody",
        );
        let idx = SpecIndex::scan(root).unwrap();
        assert_eq!(idx.list(DocKind::Endpoint).len(), 0);
        assert_eq!(idx.list(DocKind::Service).len(), 1);
        assert_eq!(idx.list(DocKind::Plan).len(), 0);
    }

    #[test]
    fn files_without_frontmatter_load_with_defaults() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("services"),
            "nofront.md",
            "# just markdown\n\nno frontmatter at all",
        );
        let idx = SpecIndex::scan(root).unwrap();
        let s = &idx.list(DocKind::Service)[0];
        assert!(s.frontmatter.name.is_none());
        assert_eq!(s.body, "# just markdown\n\nno frontmatter at all");
    }

    #[test]
    fn malformed_frontmatter_is_skipped_not_fatal() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("services"),
            "bad.md",
            "---\nname: : : invalid yaml: [unterminated\n---\nbody",
        );
        let idx = SpecIndex::scan(root).unwrap();
        let s = &idx.list(DocKind::Service)[0];
        assert!(s.frontmatter.name.is_none());
        // The whole document (including the failed YAML block) ends up as body.
        assert!(s.body.contains("body"));
    }

    #[test]
    fn for_service_returns_service_doc_plus_resolved_endpoints() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("services"),
            "driver.md",
            "---\nname: Driver\n---\n\
             1. ep-1, getFoo, DVLA\n\
             2. ep-2, getBar, DVLA\n",
        );
        write(
            &root.join("endpoints"),
            "getFoo.md",
            "---\nid: ep-1\nname: Get Foo\nmethod: GET\n---\n",
        );
        write(
            &root.join("endpoints"),
            "getBar.md",
            "---\nid: ep-2\nname: Get Bar\nmethod: GET\n---\n",
        );
        let idx = SpecIndex::scan(root).unwrap();
        let docs = idx.for_service("driver");
        assert_eq!(docs.len(), 3);
        assert_eq!(docs[0].kind, DocKind::Service);
        assert_eq!(docs[1].name, "getFoo");
        assert_eq!(docs[2].name, "getBar");
    }

    #[test]
    fn unresolved_refs_are_skipped_with_warning() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("services"),
            "driver.md",
            "---\nname: Driver\n---\n\
             1. ep-1, getFoo, DVLA\n\
             2. ep-2, doesNotExist, DVLA\n",
        );
        write(
            &root.join("endpoints"),
            "getFoo.md",
            "---\nid: ep-1\nname: Get Foo\nmethod: GET\n---\n",
        );
        let idx = SpecIndex::scan(root).unwrap();
        let docs = idx.for_service("driver");
        assert_eq!(docs.len(), 2, "unresolved ref must be dropped");
        assert_eq!(docs[1].name, "getFoo");
    }

    #[test]
    fn endpoints_filter_substring_matches_name_and_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("endpoints"),
            "getFoo.md",
            "---\nname: Get Foo\nmethod: GET\nendpoint: https://api.example.com/foo\n---\n",
        );
        write(
            &root.join("endpoints"),
            "postBar.md",
            "---\nname: Post Bar\nmethod: POST\nendpoint: https://api.example.com/bar\n---\n",
        );
        let idx = SpecIndex::scan(root).unwrap();

        // Empty filter returns everything.
        assert_eq!(idx.endpoints(None).len(), 2);
        // Substring on file stem.
        let hits = idx.endpoints(Some("foo"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "getFoo");
        // Substring on method.
        let hits = idx.endpoints(Some("POST"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "postBar");
        // Substring on path.
        let hits = idx.endpoints(Some("api.example.com"));
        assert_eq!(hits.len(), 2);
        // Substring on frontmatter name.
        let hits = idx.endpoints(Some("Get Foo"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "getFoo");
        // Case-insensitive.
        let hits = idx.endpoints(Some("GETFOO"));
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn services_and_plans_for_endpoint_resolve_back() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("services"),
            "driver.md",
            "---\nname: Driver\n---\n1. ep-1, getFoo, DVLA\n",
        );
        write(
            &root.join("services"),
            "fleet.md",
            "---\nname: Fleet\n---\n1. ep-1, getFoo, DVLA\n2. ep-2, getBar, DVLA\n",
        );
        write(
            &root.join("plans"),
            "annual.md",
            "---\nname: Annual\ntype: plan\n---\n1. ep-1, getFoo, DVLA, required\n",
        );
        write(
            &root.join("endpoints"),
            "getFoo.md",
            "---\nid: ep-1\nname: Get Foo\nmethod: GET\n---\n",
        );
        write(
            &root.join("endpoints"),
            "getBar.md",
            "---\nid: ep-2\nname: Get Bar\nmethod: GET\n---\n",
        );
        let idx = SpecIndex::scan(root).unwrap();

        let services = idx.services_for_endpoint("getFoo");
        let names: Vec<_> = services.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["driver", "fleet"]);

        let plans = idx.plans_for_endpoint("getFoo");
        let names: Vec<_> = plans.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["annual"]);

        let services = idx.services_for_endpoint("getBar");
        let names: Vec<_> = services.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["fleet"]);

        // An endpoint referenced by nobody.
        let lone = idx.services_for_endpoint("doesNotExist");
        assert!(lone.is_empty());
    }

    #[test]
    fn endpoints_for_plan_preserves_required_flag() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("plans"),
            "annual.md",
            "---\nname: Annual\ntype: plan\n---\n\
             1. ep-1, getFoo, DVLA, required\n\
             2. ep-2, getBar, DVLA, optional\n",
        );
        write(
            &root.join("endpoints"),
            "getFoo.md",
            "---\nid: ep-1\nname: Get Foo\n---\n",
        );
        write(
            &root.join("endpoints"),
            "getBar.md",
            "---\nid: ep-2\nname: Get Bar\n---\n",
        );
        let idx = SpecIndex::scan(root).unwrap();
        let plan = &idx.plans()[0];
        assert_eq!(plan.endpoint_refs.len(), 2);
        assert_eq!(plan.endpoint_refs[0].name, "getFoo");
        assert_eq!(plan.endpoint_refs[0].required, Some(true));
        assert_eq!(plan.endpoint_refs[1].name, "getBar");
        assert_eq!(plan.endpoint_refs[1].required, Some(false));

        let endpoints = idx.endpoints_for_plan("annual");
        assert_eq!(endpoints.len(), 2);
    }

    #[test]
    fn split_frontmatter_handles_crlf() {
        let raw = "---\r\nname: x\r\n---\r\nbody";
        let (fm, body) = split_frontmatter(raw);
        assert_eq!(fm.name.as_deref(), Some("x"));
        assert!(body.starts_with("body"));
    }

    #[test]
    fn parse_endpoint_refs_handles_required_and_optional() {
        let body = "\
1. ep-1, getFoo, DVLA
2. ep-2, getBar, DVLA, required
3. ep-3, getBaz, DVLA, optional
4. ep-4, getQux, DWP, REQUIRED
";
        let refs = parse_endpoint_refs(body);
        assert_eq!(refs.len(), 4);
        assert_eq!(refs[0].name, "getFoo");
        assert_eq!(refs[0].required, None);
        assert_eq!(refs[1].name, "getBar");
        assert_eq!(refs[1].required, Some(true));
        assert_eq!(refs[2].name, "getBaz");
        assert_eq!(refs[2].required, Some(false));
        assert_eq!(refs[3].name, "getQux");
        assert_eq!(refs[3].required, Some(true));
    }

    #[test]
    fn parse_endpoint_refs_ignores_prose_and_headings() {
        let body = "\
# Notes

This service owns the following endpoints.

1. ep-1, getFoo, DVLA
2. ep-2, getBar, DVLA, required

## See also

- some prose line
";
        let refs = parse_endpoint_refs(body);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].name, "getFoo");
        assert_eq!(refs[1].name, "getBar");
    }

    #[test]
    fn parse_endpoint_refs_handles_paren_separator() {
        let body = "1) ep-1, getFoo, DVLA\n2) ep-2, getBar, DVLA\n";
        let refs = parse_endpoint_refs(body);
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn parse_endpoint_refs_skips_malformed_lines() {
        let body = "\
1. ep-1, getFoo, DVLA
not a numbered line
2. only-name, no-id
3. ep-3, getBaz, DWP, malformed-flag
4. ep-4, getQux, DWP, required
";
        let refs = parse_endpoint_refs(body);
        // Line 1 OK, line 2 OK (id+name present, department skipped), line 3 OK,
        // line 4 OK. The "malformed-flag" line is still parsed; the
        // required flag just stays None.
        assert_eq!(refs.len(), 4);
        assert_eq!(refs[0].name, "getFoo");
        assert_eq!(refs[1].name, "no-id");
        assert_eq!(refs[2].name, "getBaz");
        assert!(refs[2].required.is_none());
        assert_eq!(refs[3].name, "getQux");
    }
}

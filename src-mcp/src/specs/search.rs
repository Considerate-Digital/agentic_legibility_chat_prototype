//! Full-text search across the loaded specs, backed by `rg --json` with a
//! graceful in-process fallback.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Result};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::loader::SpecIndex;
use super::model::DocKind;

/// One match returned by the search tool.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub file: PathBuf,
    pub line: u64,
    pub text: String,
}

/// Run a case-insensitive search for `query` across `live_resources_dir`.
///
/// If `kind` is provided, restrict the search to the corresponding
/// subdirectory. Falls back to an in-process substring scan over the
/// in-memory `SpecIndex` when `rg` is not available, so the tool still
/// works on minimal systems.
pub async fn search(
    live_resources_dir: &Path,
    index: &SpecIndex,
    query: &str,
    kind: Option<DocKind>,
) -> Result<Vec<SearchHit>> {
    let search_root = match kind {
        Some(k) => live_resources_dir.join(k.subdir()),
        None => live_resources_dir.to_path_buf(),
    };
    if !search_root.exists() {
        tracing::debug!("search root does not exist: {}", search_root.display());
        return Ok(Vec::new());
    }

    match search_with_rg(&search_root, query).await {
        Ok(hits) => Ok(hits),
        Err(RgError::NotInstalled) => {
            tracing::warn!("rg not found on PATH; falling back to in-process substring search");
            Ok(substring_fallback(index, query, kind))
        }
        Err(RgError::Other(e)) => Err(e),
    }
}

#[derive(Debug)]
enum RgError {
    NotInstalled,
    Other(anyhow::Error),
}

async fn search_with_rg(root: &Path, query: &str) -> std::result::Result<Vec<SearchHit>, RgError> {
    let mut cmd = tokio::process::Command::new("rg");
    cmd.args([
        "--json",
        "-i",
        "--no-heading",
        "--no-messages",
        "--",
        query,
    ])
    .arg(root)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            RgError::NotInstalled
        } else {
            RgError::Other(anyhow::anyhow!(e))
        }
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| RgError::Other(anyhow::anyhow!("rg stdout unavailable")))?;

    let mut hits = Vec::new();
    let mut reader = BufReader::new(stdout).lines();
    while let Some(line) = reader
        .next_line()
        .await
        .map_err(|e| RgError::Other(anyhow::anyhow!(e)))?
    {
        if let Ok(msg) = serde_json::from_str::<RgMessage>(&line) {
            if let RgMessage::Match { data } = msg {
                if let Some(matched) = data.submatches.first() {
                    let text = data
                        .lines
                        .text
                        .trim_end()
                        .to_string();
                    hits.push(SearchHit {
                        file: PathBuf::from(data.path.text),
                        line: data.line_number.unwrap_or(0),
                        text,
                    });
                    let _ = matched; // unused after we already pulled lines.text
                }
            }
        }
    }
    let status = child.wait().await.map_err(|e| RgError::Other(anyhow!(e)))?;
    if !status.success() && hits.is_empty() {
        // rg returns 1 when nothing matches, which is not an error for us.
        // Anything else is propagated.
        if let Some(code) = status.code() {
            if code != 1 {
                return Err(RgError::Other(anyhow::anyhow!(
                    "rg exited with status {code}"
                )));
            }
        }
    }
    Ok(hits)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum RgMessage {
    #[serde(rename = "match")]
    Match {
        data: RgMatchData,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct RgMatchData {
    path: RgText,
    lines: RgText,
    line_number: Option<u64>,
    #[serde(default)]
    submatches: Vec<RgSubmatch>,
}

#[derive(Debug, Deserialize)]
struct RgSubmatch {
    #[serde(default)]
    #[allow(dead_code)]
    match_text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RgText {
    text: String,
}

fn substring_fallback(index: &SpecIndex, query: &str, kind: Option<DocKind>) -> Vec<SearchHit> {
    let q = query.to_lowercase();
    let mut hits = Vec::new();
    for doc in index.iter() {
        if let Some(k) = kind {
            if doc.kind != k {
                continue;
            }
        }
        for (idx, line) in doc.body.lines().enumerate() {
            if line.to_lowercase().contains(&q) {
                hits.push(SearchHit {
                    file: doc.rel_path.clone(),
                    line: (idx + 1) as u64,
                    text: line.to_string(),
                });
            }
        }
    }
    hits
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
    fn substring_fallback_finds_matches() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("services"),
            "x.md",
            "---\nname: X\n---\nthis body contains the word UNICORN inside it",
        );
        let idx = SpecIndex::scan(root).unwrap();
        let hits = substring_fallback(&idx, "unicorn", Some(DocKind::Service));
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("UNICORN"));
    }
}

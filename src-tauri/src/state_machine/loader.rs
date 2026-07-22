use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Context, Result};

use super::registry::{CardRegistry, StateRegistry, ToolRegistry};
use super::types::{CardDefinition, CardFrontmatter, StateDefinition, StateFrontmatter, ToolDefinition, ToolFrontmatter};

// Defaults are bundled with the app as Tauri resources under
// `src-tauri/resources/defaults/{states,cards,tools/state}/*.md`. They are
// copied to the user-config dirs on first launch (and on `reset_to_defaults`)
// only if those dirs are empty. After that, the user owns the files; the
// binary has no compile-time coupling to any `.md` content.

// ── Disk-copy helpers ─────────────────────────────────────────────────────

/// True if `dir` doesn't exist or contains no entries (dotfiles excluded).
/// Used by `seed_*` to decide whether to copy defaults on top.
fn is_dir_empty(dir: &Path) -> Result<bool> {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => {
            for entry in entries.by_ref() {
                let entry = entry?;
                if let Some(name) = entry.file_name().to_str() {
                    if !name.starts_with('.') {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e).with_context(|| format!("reading {:?}", dir)),
    }
}

/// Copy every `.md` file (and nothing else) from `src` to `dst`, skipping
/// dotfiles. Recurses one level so callers can pass
/// `defaults_root/tools/state/` and land files in `user tools_dir/state/`.
///
/// Overwrites by default — used by `reset_to_defaults`. `seed_*` guards
/// the call with `is_dir_empty` so first-run seeds are non-destructive.
fn copy_dir_files(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        // Source missing isn't fatal — the defaults tree might intentionally
        // omit a category. Log a warning so it's discoverable.
        eprintln!(
            "loader: defaults source {:?} does not exist; skipping copy",
            src
        );
        return Ok(());
    }

    std::fs::create_dir_all(dst)
        .with_context(|| format!("creating {:?}", dst))?;

    let read_dir = std::fs::read_dir(src)
        .with_context(|| format!("reading {:?}", src))?;

    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if file_name.starts_with('.') {
            continue;
        }

        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            // One-level recursion: `defaults_root/tools/state/` →
            // `user tools_dir/state/`. We don't recurse further because
            // the registry loaders don't either.
            copy_dir_files(&path, &dst.join(file_name))?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let dst_path = dst.join(file_name);
            std::fs::write(&dst_path, std::fs::read(&path).with_context(|| {
                format!("reading source {:?}", path)
            })?)
            .with_context(|| format!("writing {:?}", dst_path))?;
        }
    }

    Ok(())
}

/// Copy every `.md` file from `src` to `dst` only when `dst` is empty
/// (or missing). Used for first-run seeding: never clobbers user content.
fn seed_from(src: &Path, dst: &Path) -> Result<()> {
    if !is_dir_empty(dst)? {
        return Ok(());
    }
    copy_dir_files(src, dst)
}

/// Seed the cards directory from `defaults_root/cards/` if it's empty.
pub fn seed_cards_dir(defaults_root: &Path, cards_dir: &Path) -> Result<()> {
    seed_from(&defaults_root.join("cards"), cards_dir)
}

/// Create playground dirs and seed them from
/// `defaults_root/states/` and `defaults_root/tools/state/` if they're empty.
///
/// `tools_dir` is expected to contain a `state/` subdirectory holding the
/// default state-machine tool definitions (server-namespaced layout).
pub fn seed_playground_dirs(
    defaults_root: &Path,
    states_dir: &Path,
    tools_dir: &Path,
) -> Result<()> {
    seed_from(&defaults_root.join("states"), states_dir)?;
    seed_from(
        &defaults_root.join("tools").join("state"),
        &tools_dir.join("state"),
    )?;
    Ok(())
}

/// Wipe the playground dirs and re-copy from `defaults_root`. Used by the
/// "Reset to defaults" command in the Config panel — the user has opted in
/// to losing their edits.
///
/// Note: this removes and re-creates each top-level dir independently, so
/// cards/states/tools don't have to all be reset together.
pub fn reset_to_defaults(
    defaults_root: &Path,
    states_dir: &Path,
    tools_dir: &Path,
    cards_dir: &Path,
) -> Result<()> {
    for dir in [states_dir, tools_dir, cards_dir] {
        if dir.exists() {
            std::fs::remove_dir_all(dir)
                .with_context(|| format!("removing {:?}", dir))?;
        }
    }
    seed_playground_dirs(defaults_root, states_dir, tools_dir)?;
    seed_cards_dir(defaults_root, cards_dir)?;
    Ok(())
}

/// Load all .md files from dir as state definitions. Files that fail to parse are skipped with a warning.
pub fn load_state_registry(dir: &Path) -> Result<StateRegistry> {
    let mut states = HashMap::new();

    let read_dir = std::fs::read_dir(dir)
        .with_context(|| format!("reading states directory {:?}", dir))?;

    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {:?}", path))?;
        match parse_state_file(&content) {
            Ok(def) => {
                states.insert(def.frontmatter.name.clone(), def);
            }
            Err(e) => {
                eprintln!("Warning: skipping state file {:?}: {}", path, e);
            }
        }
    }

    Ok(StateRegistry { states })
}

/// Load all .md files from `dir/<server>/*.md` as tool definitions, keyed by
/// `server::tool_name`. Subdirectories become the server namespace. Files that
/// fail to parse are skipped with a warning.
pub fn load_tool_registry(dir: &Path) -> Result<ToolRegistry> {
    let mut tools = HashMap::new();

    let read_dir = std::fs::read_dir(dir)
        .with_context(|| format!("reading tools directory {:?}", dir))?;

    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        // Skip dotfiles, the per-kind "files" set isn't relevant here.
        if file_name.starts_with('.') {
            continue;
        }

        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            // Recurse one level: tools/<server>/*.md
            let server = file_name.to_string();
            let inner = std::fs::read_dir(&path)
                .with_context(|| format!("reading tools server subdir {:?}", path))?;
            for inner_entry in inner {
                let inner_entry = inner_entry?;
                let inner_path = inner_entry.path();
                if inner_path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                let content = std::fs::read_to_string(&inner_path)
                    .with_context(|| format!("reading {:?}", inner_path))?;
                match parse_tool_file(&content) {
                    Ok(def) => {
                        // `server_tool` matches the LLM-facing pattern
                        // ^[a-zA-Z0-9_-]+$ so we don't need to rewrite names
                        // at the OpenAI boundary.
                        let key = format!("{}_{}", server, def.frontmatter.name);
                        tools.insert(key, def);
                    }
                    Err(e) => {
                        eprintln!("Warning: skipping tool file {:?}: {}", inner_path, e);
                    }
                }
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            // Legacy flat file (no server subdir). Treat as `state_tool` so
            // existing installations keep working until migrated.
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {:?}", path))?;
            match parse_tool_file(&content) {
                Ok(def) => {
                    eprintln!(
                        "Note: {:?} is at the tools/ root; please move it under tools/state/ (treating as state_{} for now)",
                        path, def.frontmatter.name
                    );
                    let key = format!("state_{}", def.frontmatter.name);
                    tools.insert(key, def);
                }
                Err(e) => {
                    eprintln!("Warning: skipping tool file {:?}: {}", path, e);
                }
            }
        }
    }

    Ok(ToolRegistry { tools })
}

/// Load all .md files from dir as card definitions.
pub fn load_card_registry(dir: &Path) -> Result<CardRegistry> {
    let mut cards = HashMap::new();

    let read_dir = std::fs::read_dir(dir)
        .with_context(|| format!("reading cards directory {:?}", dir))?;

    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {:?}", path))?;
        match parse_card_file(&content) {
            Ok(def) => {
                cards.insert(def.frontmatter.name.clone(), def);
            }
            Err(e) => {
                eprintln!("Warning: skipping card file {:?}: {}", path, e);
            }
        }
    }

    Ok(CardRegistry { cards })
}

pub fn parse_card_file(content: &str) -> Result<CardDefinition> {
    let (yaml, body) = split_frontmatter(content)
        .ok_or_else(|| anyhow!("missing YAML frontmatter (expected --- ... ---)"))?;
    let frontmatter: CardFrontmatter = serde_yaml::from_str(yaml)
        .context("invalid card frontmatter YAML")?;

    // Extract the last ```css ... ``` block from the body
    let (generation_instructions, css) = split_css_block(body);

    Ok(CardDefinition {
        frontmatter,
        generation_instructions: generation_instructions.trim().to_string(),
        css,
    })
}

/// Split body into (prose, Option<css>): finds the last ```css ... ``` fenced block.
fn split_css_block(body: &str) -> (String, Option<String>) {
    // Look for the last occurrence of ```css\n ... \n```
    let needle = "\n```css\n";
    if let Some(start) = body.rfind(needle) {
        let after_fence = &body[start + needle.len()..];
        if let Some(end) = after_fence.find("\n```") {
            let css = after_fence[..end].to_string();
            let prose = body[..start].to_string();
            return (prose, Some(css));
        }
    }
    (body.to_string(), None)
}

pub fn parse_state_file(content: &str) -> Result<StateDefinition> {
    let (yaml, body) = split_frontmatter(content)
        .ok_or_else(|| anyhow!("missing YAML frontmatter (expected --- ... ---)"))?;
    let frontmatter: StateFrontmatter = serde_yaml::from_str(yaml)
        .context("invalid state frontmatter YAML")?;
    Ok(StateDefinition {
        frontmatter,
        system_prompt: body.to_string(),
    })
}

pub fn parse_tool_file(content: &str) -> Result<ToolDefinition> {
    let (yaml, body) = split_frontmatter(content)
        .ok_or_else(|| anyhow!("missing YAML frontmatter (expected --- ... ---)"))?;
    let frontmatter: ToolFrontmatter = serde_yaml::from_str(yaml)
        .context("invalid tool frontmatter YAML")?;
    Ok(ToolDefinition {
        frontmatter,
        extended_description: body.to_string(),
    })
}

fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let content = content.trim_start();
    let rest = content.strip_prefix("---\n").or_else(|| content.strip_prefix("---\r\n"))?;
    let end_pos = rest.find("\n---")?;
    let yaml = &rest[..end_pos];
    let after = &rest[end_pos + 4..];
    let body = after
        .trim_start_matches('\r')
        .trim_start_matches('\n');
    Some((yaml, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Bundled defaults live at `<crate-root>/resources/defaults/`. The path
    /// is resolved at compile time via `env!("CARGO_MANIFEST_DIR")`, so
    /// tests don't need any setup beyond a clean `cargo test` invocation.
    fn defaults_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/defaults")
    }

    /// Walk every `.md` file under `dir` and assert it parses with a non-
    /// empty `name`. Substitutes the old `all_embedded_*_parse` tests —
    /// we no longer have in-memory copies to iterate; the bundled tree
    /// on disk *is* the canonical set.
    fn assert_all_markdown_parses_as_states(dir: &Path) {
        let mut found = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let content = std::fs::read_to_string(&path).unwrap();
            let def = parse_state_file(&content)
                .unwrap_or_else(|e| panic!("failed to parse {:?}: {}", path, e));
            assert!(!def.frontmatter.name.is_empty(), "empty name in {:?}", path);
            assert!(
                !def.frontmatter.description.is_empty(),
                "empty description in {:?}",
                path
            );
            found += 1;
        }
        assert!(found > 0, "no state files found in {:?}", dir);
    }

    fn assert_all_markdown_parses_as_tools(dir: &Path) {
        let mut found = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let content = std::fs::read_to_string(&path).unwrap();
            let def = parse_tool_file(&content)
                .unwrap_or_else(|e| panic!("failed to parse {:?}: {}", path, e));
            assert!(!def.frontmatter.name.is_empty(), "empty name in {:?}", path);
            found += 1;
        }
        assert!(found > 0, "no tool files found in {:?}", dir);
    }

    #[test]
    fn all_bundled_states_parse() {
        assert_all_markdown_parses_as_states(&defaults_root().join("states"));
    }

    #[test]
    fn all_bundled_tools_parse() {
        assert_all_markdown_parses_as_tools(
            &defaults_root().join("tools").join("state"),
        );
    }

    #[test]
    fn load_from_seeded_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let states_dir = dir.path().join("states");
        let tools_dir = dir.path().join("tools");
        seed_playground_dirs(&defaults_root(), &states_dir, &tools_dir).unwrap();

        let state_registry = load_state_registry(&states_dir).unwrap();
        // After the states-cards-reduction, the canonical 3-state set is
        // Advice, Plan, Execute. Assert each is present.
        assert!(state_registry.get("Advice").is_some());
        assert!(state_registry.get("Plan").is_some());
        assert!(state_registry.get("Execute").is_some());

        let tool_registry = load_tool_registry(&tools_dir).unwrap();
        // Storage keys are namespaced by server (`state_<tool>`) so two
        // servers can own a tool with the same bare name without collision.
        assert!(tool_registry.get("state_change_state").is_some());
        assert!(tool_registry.get("state_fetch").is_some());

        // `to_llm_tools` accepts bare names (what state .md files use) and
        // emits the same bare name as `function.name` so the LLM can call
        // it verbatim.
        let defs = tool_registry.to_llm_tools(&[
            "fetch".into(),
            "change_state".into(),
        ]);
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].function.name, "fetch");
        assert_eq!(defs[1].function.name, "change_state");
    }

    #[test]
    fn seed_is_non_destructive_when_user_has_content() {
        // First seed populates the temp dir.
        let dir = tempfile::tempdir().unwrap();
        let states_dir = dir.path().join("states");
        let tools_dir = dir.path().join("tools");
        seed_playground_dirs(&defaults_root(), &states_dir, &tools_dir).unwrap();

        // User edits a state file.
        let user_state = states_dir.join("advice.md");
        let original = std::fs::read_to_string(&user_state).unwrap();
        std::fs::write(&user_state, format!("{}\n\n# USER EDIT\n", original)).unwrap();

        // Second seed must not clobber the user edit.
        seed_playground_dirs(&defaults_root(), &states_dir, &tools_dir).unwrap();
        let after = std::fs::read_to_string(&user_state).unwrap();
        assert!(
            after.contains("USER EDIT"),
            "seed overwrote user content"
        );
    }

    #[test]
    fn reset_to_defaults_wipes_and_recopies() {
        let dir = tempfile::tempdir().unwrap();
        let states_dir = dir.path().join("states");
        let tools_dir = dir.path().join("tools");
        let cards_dir = dir.path().join("cards");
        let defaults = defaults_root();

        // Seed, then pollute, then reset.
        seed_playground_dirs(&defaults, &states_dir, &tools_dir).unwrap();
        seed_cards_dir(&defaults, &cards_dir).unwrap();
        let states_before = std::fs::read_to_string(&states_dir.join("advice.md")).unwrap();
        std::fs::write(
            &states_dir.join("advice.md"),
            format!("{}\n\n# JUNK\n", states_before),
        )
        .unwrap();

        reset_to_defaults(&defaults, &states_dir, &tools_dir, &cards_dir).unwrap();

        let states_after = std::fs::read_to_string(&states_dir.join("advice.md")).unwrap();
        assert!(!states_after.contains("JUNK"), "reset left junk behind");
        assert!(states_after.contains(&states_before[..50]));

        // Cards and tools dirs also re-seeded.
        assert!(cards_dir.join("action_checklist.md").exists());
        assert!(tools_dir.join("state").join("change_state.md").exists());
    }

    #[test]
    fn can_transition_advice_to_plan() {
        let dir = tempfile::tempdir().unwrap();
        let states_dir = dir.path().join("states");
        let tools_dir = dir.path().join("tools");
        seed_playground_dirs(&defaults_root(), &states_dir, &tools_dir).unwrap();
        let registry = load_state_registry(&states_dir).unwrap();
        // Per states-cards-reduction plan: Advice → {Plan, Execute}.
        assert!(registry.can_transition("Advice", "Plan"));
    }

    #[test]
    fn cannot_transition_advice_to_idle() {
        // Idle is gone after the states-cards-reduction. Assert a transition
        // to a non-existent state is rejected (rather than allowed by
        // default), which is the load-bearing invariant for this test.
        let dir = tempfile::tempdir().unwrap();
        let states_dir = dir.path().join("states");
        let tools_dir = dir.path().join("tools");
        seed_playground_dirs(&defaults_root(), &states_dir, &tools_dir).unwrap();
        let registry = load_state_registry(&states_dir).unwrap();
        assert!(!registry.can_transition("Advice", "Idle"));
    }
}

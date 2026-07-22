# Agentic Legibility Chat

A Tauri desktop app that runs a small LLM-driven state machine for guiding a caseworker or claimant through a UK government service journey. The LLM operates inside one of three states (**Advice → Plan → Execute**), each with its own system prompt and tool set defined in markdown files, and can render structured "cards" (service overviews, step checklists, progress summaries) in place of plain chat text.

## What this is

- A Rust/Tauri backend (`src-tauri`) that owns the state machine, talks to an OpenAI-compatible LLM API, and dispatches tool calls to a single bundled MCP (Model Context Protocol) sidecar process.
- A Svelte/Vite frontend (`ui`) that renders the chat window, state indicator, card bubbles, config panel, and first-run setup wizard.
- One bundled MCP sidecar binary, **`legibility-chat-mcp`** (`src-mcp` in this repo, stdio JSON-RPC), which always exposes `fetch`, `report_service_step`, and `ui_input` (the latter two are actually intercepted by the Tauri host before they'd reach this server). If the user configures a `live_resources_dir` (see below), it additionally exposes 12 read-only spec-lookup tools (`list_services`, `get_plan`, `search_specs`, etc.) plus `get_memory`/`add_memory` for persistent cross-session notes, all backed by that directory.

## Architecture

### The state machine

Three states, defined as markdown files with YAML frontmatter under `src-tauri/resources/defaults/states/`:

- **Advice** — general orientation, can transition to Plan or Execute
- **Plan** — lays out the steps of a service journey
- **Execute** — walks through a specific step

Each state file's frontmatter declares `valid_transitions` and the `tools` available in that state (tool names are looked up in the tool registry below; `change_state` is auto-injected into every state, see `state_machine/registry.rs`). The LLM calls `change_state` as a tool to move between states; `commands/chat.rs` intercepts that call on the host side rather than forwarding it to an MCP server.

### Tools

Markdown files under `src-tauri/resources/defaults/tools/state/`, one per tool, each with frontmatter (name, description, JSON schema) plus prose instructing the LLM on when/why to call it. Two categories:

- **Host-intercepted**: `change_state`, `ui_input`, `report_service_step` — handled directly by the Tauri backend for UI-side effects, never actually dispatched to an MCP server.
- **MCP-dispatched**: `fetch` — routed to `legibility-chat-mcp`'s real implementation (a `ureq`-based HTTP call). When a `live_resources_dir` is configured, `legibility-chat-mcp` additionally exposes 12 spec-lookup tools (`list_services`, `get_plan`, `search_specs`, etc.) plus `get_memory`/`add_memory`, supplied at runtime rather than from markdown — see `src-tauri/src/mcp/spec_tools.rs` and `src-mcp/src/tools/spec_tools.rs`.

### Cards

Markdown files under `src-tauri/resources/defaults/cards/` (`service_overview`, `step_list`, `step_detail`, `steps_grouped`, `step_checklist`, `input_preview`, `progress_summary`). Each has frontmatter (`name`, `description`, `relevant_states`), prose generation instructions, and an example HTML template. When `cards_enabled` is on, a card-selector LLM call picks a card by name/description, then a second LLM call renders the card's `generation_instructions` into HTML, which `CardBubble.svelte` renders via `{@html}`. Cards deliberately share one fixed CSS class palette (from a sibling project's stylesheet, `../../service_creator/src/app.css`, referenced by class name only — not bundled here) rather than each shipping its own `<style>` block; when authoring a new card, only use documented classes from that palette.

### Bundled resources and overrides

`src-tauri/resources/defaults/{states,tools/state,cards}/*.md` are compiled into the app bundle (see `tauri.conf.json`'s `bundle.resources`) and copied to `~/.config/legibility-chat/{states,tools,cards}/` on first run, or on "Reset to defaults" (`state_machine/loader.rs`). At runtime, `AppConfig.states_override_dir` / `tools_override_dir` / `cards_override_dir` can point the app at a different directory instead — useful for iterating on prompts without rebuilding.

### Config and first-run wizard

App config lives at `~/.config/legibility-chat/config.json` (`src-tauri/src/config.rs`), not environment variables — there's no `.env` file to set up. Key fields:

| Field | Purpose |
|---|---|
| `provider.{base_url,api_key,model}` | Main LLM provider — any OpenAI-compatible endpoint (OpenAI, Anthropic via the `/v1` shim, OpenRouter, etc.) |
| `analyser` | Optional cheaper/faster model override for the state-evaluation call; falls back to `provider` if unset |
| `states_override_dir` / `tools_override_dir` / `cards_override_dir` | Optional runtime overrides for the bundled markdown |
| `live_resources_dir` | Path to a directory with `endpoints/`, `services/`, `plans/` subdirs (and, once the LLM starts recording facts, a `memory.md` at its root). When set, `legibility-chat-mcp` is restarted with the directory wired in and its spec-lookup + memory tools become available; when unset, only the always-on tools (`fetch`, `change_state`, `ui_input`, `report_service_step`) run |
| `cards_enabled` | Whether the card-selector pipeline runs at all (default on) |

`SetupWizard.svelte` drives first-run configuration of these fields through `commands/wizard.rs`.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- [Node.js](https://nodejs.org/) 18+ and [pnpm](https://pnpm.io/)
- Tauri's native dependencies for your OS (e.g. WebKit2GTK on Linux — see the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/))

## Local development

```bash
pnpm install --dir ui
./dev.sh
```

`dev.sh` builds the `legibility-chat-mcp` sidecar binary, copies it into `src-tauri/binaries/`, frees port 5173, and launches `tauri dev`.

## Validation

```bash
cargo build --workspace          # both Rust crates: legibility-chat, legibility-chat-mcp
cargo test --workspace           # unit tests + doctests
pnpm --dir ui check              # svelte-check + tsc, no separate lint/test scripts exist yet
```

There is no `pnpm lint`, `pnpm test`, or browser test suite configured for `ui/` at present — `check` (type-checking only) is the only frontend validation gate.

## Production build

```bash
./dev.sh   # or the manual sidecar-build steps above
cd src-tauri && cargo tauri build
```

Bundled artifacts land in `src-tauri/target/release/bundle/` (`.AppImage`/`.deb` on Linux, `.dmg`/`.app` on macOS, NSIS `.exe` on Windows — signing/notarization not configured here).

## Project structure

```
src-tauri/
  src/
    commands/          Tauri command handlers (chat, config, state, wizard, files, live_resources)
    state_machine/      loader (markdown parsing, seeding, reset), registry, types
    mcp/                 router, legibility_chat_client (legibility-chat-mcp), spec_tools (gated tool-name list)
    llm/                 provider-agnostic client, OpenAI + Anthropic SSE parsing, shared types
  resources/defaults/     bundled states/tools/cards markdown (see Architecture above)
src-mcp/                 legibility-chat-mcp sidecar: fetch/report_service_step/ui_input plus,
                          when live_resources_dir is set, spec-lookup + memory tools (context.rs, specs/, tools/spec_tools.rs)
ui/
  src/lib/                Svelte components (ChatWindow, CardBubble, StateSelector, SetupWizard, PlaygroundPanel, ...)
.claude/plans/            design docs for completed and in-flight features
```

## Maintenance notes and known constraints

- Card CSS classes are not self-contained — they assume a stylesheet from `../../service_creator/src/app.css` (also a sibling project) is present at runtime. There is no local fallback stylesheet in this repo.
- `relevant_states` in card frontmatter is parsed but not currently used to gate which cards are eligible in which state — it's descriptive metadata only.

## Follow-up work

- No frontend lint or automated test suite exists (`ui/package.json` only has `check`); consider adding one before the component count grows further.
- `Cargo.lock` is gitignored even though this is an application (not a library) workspace — Rust convention is normally to commit it for binaries, for reproducible builds.

### Authors and Support
This project was made by Alex and Jen at Considerate Digital. If you need support please [contact us](https://considerate.digital).

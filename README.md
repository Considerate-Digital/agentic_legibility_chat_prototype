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

Markdown files under `src-tauri/resources/defaults/cards/`: `action_checklist` (ActionChecklist — a checklist of concrete next actions), `case_progress` (CaseProgress — a row of stage cards showing progress through a plan), and `key_facts` (KeyFacts — 3–5 key facts, figures, deadlines, or criteria as labelled rows). Each has frontmatter (`name`, `description`, `relevant_states`) and prose generation instructions. When `cards_enabled` is on, a card-selector LLM call picks a card by name/description, then a second LLM call renders the card's generation instructions into HTML, which `CardBubble.svelte` renders via `{@html}`. 

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

## Mock server

`mock-server.js` (repo root) is a zero-dependency Node script (`node:http` only, no `npm install` needed) that stands in for the real upstream government APIs referenced by `endpoint:` URLs in a `live_resources_dir/endpoints/*.md` spec directory — so you can exercise the `fetch` tool and full Advice → Plan → Execute flow without live credentials or network access to the real services.

Run it standalone:

```bash
node mock-server.js [port]   # default port 8127
```

Or launch it alongside the app in one step:

```bash
./dev-with-mock.sh
```

This starts `mock-server.js` in the background, runs `./dev.sh` in the foreground, and stops the mock server when `dev.sh` exits (Ctrl-C, crash, or normal exit). Override the port with `MOCK_PORT=9000 ./dev-with-mock.sh`.

It covers five path prefixes matching the FLEX API domains used in the bundled specs — `/udp` (One Login User Data Platform), `/dvla` (DVLA driver/vehicle/share-code APIs), `/uns` (Unified Notification Service), `/local-council` (MHCLG local authority lookup), and `/example` (a generic example domain for todos/resources/headers). Each route is a `{ method, path: RegExp, status, handler }` entry in the `ROUTES` array; more specific paths must be listed before overlapping general ones, since the first regex match wins.

The mock only returns canned data for routes it already knows about — it does **not** read the `live_resources_dir/endpoints/*.md` files at runtime. To keep it in sync with the "expected" service endpoints declared there:

1. For each `.md` file under your `live_resources_dir/endpoints/`, note its frontmatter `method` and `endpoint` (a full URL, e.g. `https://flex.account.gov.uk/dvla/v1/driving-licence`).
2. In `mock-server.js`, add or update a `ROUTES` entry with the same `method` and a `path` regex matching the URL's path (everything after the host) under the matching prefix section (`/udp`, `/dvla`, `/uns`, `/local-council`, `/example`), with a `handler` returning representative JSON for that response shape.
3. Point the app's `fetch` calls at the mock instead of the real host by changing the affected endpoint spec's `endpoint:` frontmatter from `https://flex.account.gov.uk/...` to `http://localhost:8127/...` (or your chosen `[port]`) in your local `live_resources_dir` copy — `fetch` uses that URL verbatim, there's no built-in host rewriting.
4. If you remove or rename an endpoint spec, remove or update the corresponding `ROUTES` entry so the mock doesn't drift from what the specs actually describe.

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
mock-server.js            zero-dependency mock of the FLEX API endpoints (see Mock server above)
dev-with-mock.sh          runs mock-server.js alongside dev.sh
.claude/plans/            design docs for completed and in-flight features
```

## Maintenance notes and known constraints

- Card CSS classes are not self-contained — they assume a stylesheet from `../../service_creator/src/app.css` (also a sibling project) is present at runtime. There is no local fallback stylesheet in this repo.
- `relevant_states` in card frontmatter is parsed but not currently used to gate which cards are eligible in which state — it's descriptive metadata only.

## Follow-up work

- No frontend lint or automated test suite exists (`ui/package.json` only has `check`); consider adding one before the component count grows further.

### Authors and Support
This project was made by Alex and Jen at Considerate Digital. If you need support please [contact us](https://considerate.digital).

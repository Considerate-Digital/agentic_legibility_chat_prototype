<script lang="ts">
  import { untrack } from 'svelte'
  import { fade } from 'svelte/transition'
  import { invoke } from '@tauri-apps/api/core'
  import type { AppConfig, LiveResourcesSummary } from '../types'

  // ── Props ───────────────────────────────────────────────────────────────
  const {
    config,
    isFirstLaunch,
    onComplete,
    onClose,
  }: {
    config: AppConfig
    isFirstLaunch: boolean
    onComplete: (c: AppConfig) => void
    onClose: () => void
  } = $props()

  // ── Wizard state ────────────────────────────────────────────────────────
  let step = $state(1)
  let draft = $state<AppConfig>(untrack(() => JSON.parse(JSON.stringify(config))))

  // ── Provider preset (Step 2) ────────────────────────────────────────────
  type ProviderPreset = 'openrouter' | 'custom'
  const OPENROUTER_BASE_URL = 'https://openrouter.ai/api/v1'
  const OPENROUTER_DEFAULT_MODEL = 'openai/gpt-4o-mini'

  function detectPreset(c: AppConfig): ProviderPreset {
    return c.provider.base_url === OPENROUTER_BASE_URL ? 'openrouter' : 'custom'
  }

  let providerPreset = $state<ProviderPreset>(untrack(() => detectPreset(config)))

  function applyPreset(preset: ProviderPreset) {
    providerPreset = preset
    if (preset === 'openrouter') {
      draft.provider.base_url = OPENROUTER_BASE_URL
      if (!draft.provider.model) {
        draft.provider.model = OPENROUTER_DEFAULT_MODEL
      }
    } else {
      // Custom: leave URL blank, user fills in
      draft.provider.base_url = ''
    }
  }

  let connectionStatus = $state<'idle' | 'checking' | 'success' | 'error'>('idle')
  let connectionError = $state('')

  async function testConnection() {
    connectionStatus = 'checking'
    connectionError = ''
    try {
      await invoke('test_llm_connection', {
        baseUrl: draft.provider.base_url,
        apiKey: draft.provider.api_key,
      })
      connectionStatus = 'success'
    } catch (e) {
      connectionStatus = 'error'
      connectionError = String(e)
    }
  }

  // Re-test if the user edits after a successful test, so the badge
  // doesn't lie about a now-stale configuration. We schedule the reset
  // in a microtask so we don't clobber a 'checking' state that the
  // user just initiated.
  $effect(() => {
    // Touch the fields we care about
    void draft.provider.base_url
    void draft.provider.api_key
    void draft.provider.model
    queueMicrotask(() => {
      if (connectionStatus !== 'checking') {
        connectionStatus = 'idle'
        connectionError = ''
      }
    })
  })

  // ── Analyser (Step 3) ───────────────────────────────────────────────────
  let analyserExpanded = $state(untrack(() => !!config.analyser?.model))
  let useSameAsMain = $state(untrack(() => !config.analyser?.model))

  function toggleAnalyser(value: boolean) {
    useSameAsMain = value
    if (value) {
      draft.analyser = undefined
    } else if (!draft.analyser) {
      draft.analyser = { model: '', base_url: undefined, api_key: undefined }
    }
  }

  // ── Live resources (Step 4) ─────────────────────────────────────────────
  let pickerBusy = $state(false)
  let scanStatus = $state<'idle' | 'scanning' | 'success' | 'invalid' | 'error'>('idle')
  let scanError = $state('')
  let liveResourcesSummary = $state<LiveResourcesSummary | null>(
    untrack(() => null),
  )

  async function browseLiveResources() {
    pickerBusy = true
    scanError = ''
    try {
      const picked = await invoke<string | null>('pick_live_resources_dir')
      if (picked !== null) {
        draft.live_resources_dir = picked
        await scanPicked()
      }
    } catch (e) {
      scanError = String(e)
      scanStatus = 'error'
    } finally {
      pickerBusy = false
    }
  }

  async function scanPicked() {
    const path = draft.live_resources_dir
    if (!path) {
      liveResourcesSummary = null
      scanStatus = 'idle'
      return
    }
    scanStatus = 'scanning'
    scanError = ''
    try {
      const summary = await invoke<LiveResourcesSummary>('scan_live_resources_dir', {
        path,
      })
      liveResourcesSummary = summary
      scanStatus = summary.is_valid ? 'success' : 'invalid'
    } catch (e) {
      scanError = String(e)
      scanStatus = 'error'
      liveResourcesSummary = null
    }
  }

  function clearLiveResources() {
    draft.live_resources_dir = undefined
    liveResourcesSummary = null
    scanStatus = 'idle'
    scanError = ''
  }

  // If the user already has a directory on first-launch entry, scan it
  // immediately so the preview shows up without an extra click.
  $effect(() => {
    const path = draft.live_resources_dir
    if (path && !liveResourcesSummary) {
      scanPicked()
    }
  })

  // ── Finish (Step 5) ─────────────────────────────────────────────────────
  let healthLLM = $state<'idle' | 'checking' | 'success' | 'error'>('idle')
  let healthSpecTools = $state<'idle' | 'checking' | 'success' | 'error'>('idle')
  let healthLLMError = $state('')
  let healthSpecToolsError = $state('')

  async function runHealthChecks() {
    healthLLM = 'checking'
    healthLLMError = ''
    healthSpecTools = 'checking'
    healthSpecToolsError = ''
    try {
      await invoke('test_llm_connection', {
        baseUrl: draft.provider.base_url,
        apiKey: draft.provider.api_key,
      })
      healthLLM = 'success'
    } catch (e) {
      healthLLM = 'error'
      healthLLMError = String(e)
    }
    // spec tools health: query the router via the get_setup_status command
    // (it reports spec_tools_ready). If the user just picked a path,
    // the router rebuild happens async on the Rust side; we'll see it
    // when status updates.
    try {
      const status = await invoke<{ spec_tools_ready: boolean }>('get_setup_status')
      if (status.spec_tools_ready) {
        healthSpecTools = 'success'
      } else {
        healthSpecTools = 'error'
        healthSpecToolsError = 'spec tools are not ready yet — try again in a moment'
      }
    } catch (e) {
      healthSpecTools = 'error'
      healthSpecToolsError = String(e)
    }
  }

  // ── Step gating ─────────────────────────────────────────────────────────
  // Per-step validity. Blocks Next until each step's required fields are
  // populated. Step 4 additionally requires the scan to confirm at least
  // one of endpoints/services/plans has .md files (live_resources_dir
  // is required by user policy — no "skip for now").
  let stepValid = $derived.by(() => {
    switch (step) {
      case 1:
        return true
      case 2:
        return (
          !!draft.provider.base_url.trim() &&
          !!draft.provider.api_key.trim() &&
          !!draft.provider.model.trim() &&
          connectionStatus === 'success'
        )
      case 3:
        return useSameAsMain || !!draft.analyser?.model.trim()
      case 4:
        return (
          !!draft.live_resources_dir &&
          liveResourcesSummary?.is_valid === true
        )
      case 5:
        return true
      default:
        return false
    }
  })

  // ── Navigation ──────────────────────────────────────────────────────────
  function next() {
    if (step < 5 && stepValid) step += 1
  }

  function back() {
    if (step > 1) step -= 1
  }

  function done() {
    onComplete(JSON.parse(JSON.stringify(draft)))
  }

  // ── Keyboard navigation ─────────────────────────────────────────────────
  // Esc dismisses the wizard (only when not first-launch — on first
  // launch there's no escape hatch and the user must complete the flow).
  // Enter advances when the current step is valid and we're not on the
  // final step; on the final step it confirms.
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !isFirstLaunch) {
      e.preventDefault()
      onClose()
    } else if (e.key === 'Enter') {
      // Don't intercept Enter inside <textarea> or when modifiers are held.
      const target = e.target as HTMLElement | null
      if (
        target?.tagName === 'TEXTAREA' ||
        target?.tagName === 'BUTTON' ||
        e.shiftKey ||
        e.ctrlKey ||
        e.metaKey ||
        e.altKey
      ) {
        return
      }
      if (step < 5 && stepValid) {
        e.preventDefault()
        next()
      } else if (step === 5) {
        e.preventDefault()
        done()
      }
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- Full-screen overlay -->
<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-gray-900/50 backdrop-blur-sm p-4"
  role="dialog"
  aria-modal="true"
  aria-labelledby="wizard-title"
>
  <div class="bg-white rounded-2xl shadow-2xl w-full max-w-2xl flex flex-col max-h-[90vh]">
    <!-- Header -->
    <div class="flex items-center justify-between px-6 py-4 border-b border-gray-200">
      <div>
        <h2 id="wizard-title" class="text-lg font-semibold text-gray-900">
          {step === 1 ? 'Welcome to Agentic Legibility Chat' : 'Agentic Legibility Chat setup'}
        </h2>
        <div class="mt-2 flex items-center gap-1.5" aria-label="Progress">
          {#each [1, 2, 3, 4, 5] as n}
            <span
              class="h-1.5 rounded-full transition-all"
              class:bg-blue-600={n === step}
              class:bg-blue-200={n < step}
              class:bg-gray-200={n > step}
              style="width: {n === step ? '2rem' : '1.5rem'}"
            ></span>
          {/each}
          <span class="ml-2 text-xs text-gray-500">{step} of 5</span>
        </div>
      </div>
      {#if !isFirstLaunch}
        <button
          onclick={onClose}
          class="text-gray-400 hover:text-gray-600 text-xl leading-none"
          aria-label="Close setup wizard"
        >
          ✕
        </button>
      {/if}
    </div>

    <!-- Body (scrollable) -->
    <div class="flex-1 overflow-y-auto p-6">
      {#key step}
      <div in:fade={{ duration: 120, delay: 60 }}>
      {#if step === 1}
        <!-- ── Step 1: Welcome ───────────────────────────────────────────── -->
        <div class="space-y-4">
          <p class="text-sm text-gray-700">
            A UK government services assistant. State-chat pairs an LLM
            with two MCP servers — one for state-machine workflow tools,
            one for browsing live UK gov endpoint, service, and plan specs.
          </p>
          <p class="text-sm text-gray-700">To work properly you'll need:</p>
          <ol class="list-decimal pl-6 space-y-2 text-sm text-gray-700">
            <li>
              An LLM provider. We recommend
              <a
                href="https://openrouter.ai"
                target="_blank"
                rel="noopener"
                class="text-blue-600 hover:underline">OpenRouter</a
              >
              — one key, access to many models.
            </li>
            <li>
              A Live Resources directory on disk: a folder containing
              <code class="bg-gray-100 px-1 rounded text-xs">endpoints/</code>,
              <code class="bg-gray-100 px-1 rounded text-xs">services/</code>, and
              <code class="bg-gray-100 px-1 rounded text-xs">plans/</code>
              subdirectories with <code class="bg-gray-100 px-1 rounded text-xs">.md</code> files.
              This unlocks the
              <code class="bg-gray-100 px-1 rounded text-xs">list_services</code>,
              <code class="bg-gray-100 px-1 rounded text-xs">search_specs</code>, and
              <code class="bg-gray-100 px-1 rounded text-xs">get_endpoint</code>
              tools.
            </li>
          </ol>
          <p class="text-xs text-gray-500 pt-2">
            Takes about a minute.
          </p>
        </div>
      {:else if step === 2}
        <!-- ── Step 2: LLM provider ─────────────────────────────────────── -->
        <div class="space-y-4">
          <div>
            <label class="block text-xs font-medium text-gray-600" for="preset"
              >Provider</label
            >
            <select
              id="preset"
              value={providerPreset}
              onchange={(e) =>
                applyPreset((e.target as HTMLSelectElement).value as ProviderPreset)}
              class="mt-1 w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              <option value="openrouter">OpenRouter</option>
              <option value="custom">Custom (any OpenAI-compatible endpoint)</option>
            </select>
            <p class="mt-1 text-xs text-gray-400">
              Picking a provider pre-fills the URL and model — you can edit them.
            </p>
          </div>

          <div>
            <label class="block text-xs font-medium text-gray-600" for="base-url"
              >Base URL</label
            >
            <input
              id="base-url"
              bind:value={draft.provider.base_url}
              type="text"
              placeholder="https://api.openai.com/v1"
              class="mt-1 w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
          </div>

          <div>
            <label class="block text-xs font-medium text-gray-600" for="api-key"
              >API Key</label
            >
            <input
              id="api-key"
              bind:value={draft.provider.api_key}
              type="password"
              placeholder="sk-or-v1-…"
              class="mt-1 w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
          </div>

          <div>
            <label class="block text-xs font-medium text-gray-600" for="model"
              >Model</label
            >
            <input
              id="model"
              bind:value={draft.provider.model}
              type="text"
              placeholder={providerPreset === 'openrouter'
                ? 'openai/gpt-4o-mini'
                : 'gpt-4o'}
              class="mt-1 w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
            <p class="mt-1 text-xs text-gray-400">
              For OpenRouter, prefix with the upstream provider: <code
                class="bg-gray-100 px-1 rounded text-xs">openai/gpt-4o-mini</code
              >, <code class="bg-gray-100 px-1 rounded text-xs">anthropic/claude-3.5-sonnet</code>.
            </p>
          </div>

          <div class="border-t border-gray-100 pt-3 flex items-center gap-3">
            <button
              type="button"
              onclick={testConnection}
              disabled={connectionStatus === 'checking' ||
                !draft.provider.base_url ||
                !draft.provider.api_key}
              class="text-xs px-3 py-1.5 rounded-lg bg-gray-100 hover:bg-gray-200 disabled:bg-gray-50 disabled:text-gray-400 disabled:cursor-not-allowed"
            >
              {connectionStatus === 'checking' ? 'Testing…' : 'Test connection'}
            </button>
            {#if connectionStatus === 'success'}
              <span class="text-xs text-green-700">✓ Reachable</span>
            {:else if connectionStatus === 'error'}
              <span class="text-xs text-red-600">{connectionError}</span>
            {/if}
          </div>
        </div>
      {:else if step === 3}
        <!-- ── Step 3: State analyser model ─────────────────────────────── -->
        <div class="space-y-4">
          <p class="text-sm text-gray-700">
            The state analyser evaluates which conversational state to enter
            before each response. It runs a lot, so a cheap model is ideal.
          </p>

          <label class="flex items-start gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={useSameAsMain}
              onchange={(e) => toggleAnalyser((e.target as HTMLInputElement).checked)}
              class="mt-1"
            />
            <div>
              <div class="text-sm font-medium text-gray-800">
                Use the same model as the main provider
              </div>
              <div class="text-xs text-gray-500">
                Recommended. Disable to pick a separate, cheaper model.
              </div>
            </div>
          </label>

          {#if !useSameAsMain}
            <div class="space-y-3 pl-6 border-l-2 border-gray-200">
              <div>
                <label class="block text-xs font-medium text-gray-600" for="analyser-model"
                  >Analyser model</label
                >
                <input
                  id="analyser-model"
                  value={draft.analyser?.model ?? ''}
                  oninput={(e) => {
                    if (!draft.analyser) {
                      draft.analyser = {
                        model: '',
                        base_url: undefined,
                        api_key: undefined,
                      }
                    }
                    draft.analyser.model = (e.target as HTMLInputElement).value
                  }}
                  type="text"
                  placeholder="gpt-4o-mini"
                  class="mt-1 w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
                />
              </div>
              <div>
                <label class="block text-xs font-medium text-gray-600" for="analyser-base-url"
                  >Analyser base URL (optional)</label
                >
                <input
                  id="analyser-base-url"
                  value={draft.analyser?.base_url ?? ''}
                  oninput={(e) => {
                    if (!draft.analyser) {
                      draft.analyser = {
                        model: '',
                        base_url: undefined,
                        api_key: undefined,
                      }
                    }
                    const v = (e.target as HTMLInputElement).value.trim()
                    draft.analyser.base_url = v ? v : undefined
                  }}
                  type="text"
                  placeholder="Uses main provider URL"
                  class="mt-1 w-full text-xs border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
                />
              </div>
              <div>
                <label class="block text-xs font-medium text-gray-600" for="analyser-api-key"
                  >Analyser API key (optional)</label
                >
                <input
                  id="analyser-api-key"
                  value={draft.analyser?.api_key ?? ''}
                  oninput={(e) => {
                    if (!draft.analyser) {
                      draft.analyser = {
                        model: '',
                        base_url: undefined,
                        api_key: undefined,
                      }
                    }
                    const v = (e.target as HTMLInputElement).value.trim()
                    draft.analyser.api_key = v ? v : undefined
                  }}
                  type="password"
                  placeholder="Uses main API key"
                  class="mt-1 w-full text-xs border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
                />
              </div>
            </div>
          {/if}
        </div>
      {:else if step === 4}
        <!-- ── Step 4: Live resources directory ─────────────────────────── -->
        <div class="space-y-4">
          <p class="text-sm text-gray-700">
            Pick the directory that holds your UK government endpoint,
            service, and plan specs. The wizard will scan it to confirm
            the layout.
          </p>

          <div class="flex items-center gap-2">
            <input
              bind:value={draft.live_resources_dir}
              type="text"
              placeholder="/home/you/.../live_resources"
              class="flex-1 text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-emerald-500"
            />
            <button
              type="button"
              onclick={browseLiveResources}
              disabled={pickerBusy}
              class="text-sm px-3 py-2 rounded-lg bg-emerald-600 text-white hover:bg-emerald-700 disabled:bg-gray-300 disabled:cursor-not-allowed"
            >
              {pickerBusy ? '…' : 'Browse…'}
            </button>
            {#if draft.live_resources_dir}
              <button
                type="button"
                onclick={clearLiveResources}
                class="text-sm px-3 py-2 rounded-lg border border-gray-300 text-gray-600 hover:bg-gray-100"
                title="Clear"
              >
                Clear
              </button>
            {/if}
          </div>

          {#if scanStatus === 'scanning'}
            <div class="text-xs text-gray-500">Scanning…</div>
          {:else if scanStatus === 'success' && liveResourcesSummary}
            <div class="bg-green-50 border border-green-200 rounded-lg px-4 py-3 text-sm">
              <div class="font-medium text-green-900 mb-1">
                ✓ {liveResourcesSummary.path}
              </div>
              <ul class="text-xs text-green-800 space-y-0.5">
                <li>{liveResourcesSummary.endpoints} endpoints</li>
                <li>{liveResourcesSummary.services} services</li>
                <li>{liveResourcesSummary.plans} plans</li>
              </ul>
            </div>
          {:else if scanStatus === 'invalid' && liveResourcesSummary}
            <div class="bg-amber-50 border border-amber-200 rounded-lg px-4 py-3 text-sm">
              <div class="font-medium text-amber-900 mb-1">
                {liveResourcesSummary.path}
              </div>
              <p class="text-xs text-amber-800">
                This directory doesn't contain any
                <code class="bg-amber-100 px-1 rounded">endpoints/</code>,
                <code class="bg-amber-100 px-1 rounded">services/</code>, or
                <code class="bg-amber-100 px-1 rounded">plans/</code>
                subdirectories with
                <code class="bg-amber-100 px-1 rounded">.md</code> files.
                Pick a different folder.
              </p>
            </div>
          {:else if scanStatus === 'error'}
            <div class="bg-red-50 border border-red-200 rounded-lg px-4 py-3 text-sm text-red-700">
              {scanError}
            </div>
          {/if}

          <p class="text-xs text-gray-500 pt-1">
            This step is required. The state prompts direct the assistant
            to call
            <code class="bg-gray-100 px-1 rounded">list_services</code> and
            friends; without this directory those calls fail.
          </p>
        </div>
      {:else if step === 5}
        <!-- ── Step 5: Finish & health check ─────────────────────────────── -->
        <div class="space-y-4">
          <p class="text-sm text-gray-700">
            Quick health check before we close. Saving will write the
            configuration and start both MCP servers.
          </p>

          <button
            type="button"
            onclick={runHealthChecks}
            disabled={healthLLM === 'checking' || healthSpecTools === 'checking'}
            class="text-xs px-3 py-1.5 rounded-lg bg-gray-100 hover:bg-gray-200 disabled:bg-gray-50 disabled:text-gray-400"
          >
            {healthLLM === 'checking' || healthSpecTools === 'checking'
              ? 'Checking…'
              : 'Run health check'}
          </button>

          <div class="space-y-2 mt-3">
            <div class="flex items-start gap-2">
              {#if healthLLM === 'success'}
                <span class="text-green-600">✓</span>
              {:else if healthLLM === 'error'}
                <span class="text-red-600">✗</span>
              {:else if healthLLM === 'checking'}
                <span class="text-gray-400">…</span>
              {:else}
                <span class="text-gray-300">·</span>
              {/if}
              <div class="text-sm">
                <div class="font-medium text-gray-800">LLM provider</div>
                <div class="text-xs text-gray-500">
                  {draft.provider.model} via {draft.provider.base_url}
                </div>
                {#if healthLLMError}
                  <div class="text-xs text-red-600 mt-0.5">{healthLLMError}</div>
                {/if}
              </div>
            </div>
            <div class="flex items-start gap-2">
              {#if healthSpecTools === 'success'}
                <span class="text-green-600">✓</span>
              {:else if healthSpecTools === 'error'}
                <span class="text-red-600">✗</span>
              {:else if healthSpecTools === 'checking'}
                <span class="text-gray-400">…</span>
              {:else}
                <span class="text-gray-300">·</span>
              {/if}
              <div class="text-sm">
                <div class="font-medium text-gray-800">Spec tools</div>
                <div class="text-xs text-gray-500">{draft.live_resources_dir}</div>
                {#if healthSpecToolsError}
                  <div class="text-xs text-red-600 mt-0.5">
                    {healthSpecToolsError}
                  </div>
                {/if}
              </div>
            </div>
          </div>
        </div>
      {/if}
      </div>
      {/key}
    </div>

    <!-- Footer -->
    <div class="px-6 py-4 border-t border-gray-200 flex items-center justify-between">
      <button
        type="button"
        onclick={back}
        disabled={step === 1}
        class="text-sm px-4 py-2 rounded-lg text-gray-600 hover:bg-gray-100 disabled:opacity-40 disabled:cursor-not-allowed"
      >
        ← Back
      </button>

      <div class="flex items-center gap-2">
        {#if step === 1 && !isFirstLaunch}
          <button
            type="button"
            onclick={onClose}
            class="text-sm px-4 py-2 rounded-lg text-gray-600 hover:bg-gray-100"
          >
            Skip to chat
          </button>
        {/if}
        {#if step < 5}
          <button
            type="button"
            onclick={next}
            disabled={!stepValid}
            class="text-sm px-4 py-2 rounded-lg bg-blue-600 text-white hover:bg-blue-700 disabled:bg-gray-300 disabled:cursor-not-allowed"
          >
            Next →
          </button>
        {:else}
          <button
            type="button"
            onclick={done}
            class="text-sm px-4 py-2 rounded-lg bg-blue-600 text-white hover:bg-blue-700"
          >
            Done ✓
          </button>
        {/if}
      </div>
    </div>
  </div>
</div>

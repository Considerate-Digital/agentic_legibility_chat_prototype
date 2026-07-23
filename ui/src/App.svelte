<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen, type UnlistenFn } from '@tauri-apps/api/event'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import type { AppConfig, SetupStatus, StateView, StateSummary, Message, ServiceStepEvent, UiInputRequest } from './types'
  import StateIndicator from './lib/StateIndicator.svelte'
  import UiInputForm from './lib/UiInputForm.svelte'
  import StateSelector from './lib/StateSelector.svelte'
  import ChatWindow from './lib/ChatWindow.svelte'
  import ConfigPanel from './lib/ConfigPanel.svelte'
  import PlaygroundPanel from './lib/PlaygroundPanel.svelte'
  import SetupWizard from './lib/SetupWizard.svelte'

  // ── App state ────────────────────────────────────────────────────────────
  let currentState = $state<StateView | null>(null)
  let allStates = $state<StateSummary[]>([])
  let config = $state<AppConfig | null>(null)
  let messages = $state<Message[]>([])
  let streamingContent = $state('')
  let isStreaming = $state(false)
  let serviceStep = $state<ServiceStepEvent | null>(null)
  let uiInputRequest = $state<UiInputRequest | null>(null)
  let showConfig = $state(false)
  let showPlayground = $state(false)

  // Chat view-mode toggle: 'cards' renders styled <CardBubble>s, 'text'
  // renders the underlying markdown. Default is 'text' so first-time
  // users see the same plain prose they typed, without unexplained
  // custom styling. Persisted in localStorage so the choice survives a
  // reload — typical web-app expectation for view preferences.
  let cardView = $state<'cards' | 'text'>(
    typeof localStorage !== 'undefined'
      ? (() => {
          const v = localStorage.getItem('cardView')
          return v === 'cards' || v === 'text' ? v : 'text'
        })()
      : 'text'
  )

  // Persist on change. Wrapped in try/catch because some browsers throw
  // on localStorage.setItem in private mode / quota-exceeded scenarios;
  // the in-memory state still works, the toggle just won't persist.
  $effect(() => {
    if (typeof localStorage === 'undefined') return
    try {
      localStorage.setItem('cardView', cardView)
    } catch {
      // ignore — see comment above
    }
  })

  // Setup wizard state. `null` until the first get_setup_status call
  // resolves; `showWizard` is derived (first-launch detection); `wasForced`
  // tracks whether the wizard was triggered automatically vs. by a click,
  // so the wizard knows whether to show its "skip to chat" escape hatch.
  let setupStatus = $state<SetupStatus | null>(null)
  let showWizard = $state(false)
  let wizardForced = $state(false)
  let playgroundWidth = $state(680)
  let errorMsg = $state('')

  const MIN_PLAYGROUND_WIDTH = 320
  const MAX_PLAYGROUND_WIDTH = 1200

  function startResize(e: MouseEvent) {
    e.preventDefault()
    const startX = e.clientX
    const startWidth = playgroundWidth

    function onMove(ev: MouseEvent) {
      playgroundWidth = Math.min(
        MAX_PLAYGROUND_WIDTH,
        Math.max(MIN_PLAYGROUND_WIDTH, startWidth + (startX - ev.clientX)),
      )
    }

    function onUp() {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
    }

    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }

  let nextId = 0
  const uid = () => String(++nextId)

  // ── Tauri event listeners ────────────────────────────────────────────────
  let unlisteners: UnlistenFn[] = []

  // Intercept clicks on markdown-rendered links and open them in the system browser
  function handleDocClick(e: MouseEvent) {
    const anchor = (e.target as Element).closest<HTMLAnchorElement>('a[data-external]')
    if (!anchor?.href) return
    e.preventDefault()
    openUrl(anchor.href).catch(console.error)
  }

  onMount(async () => {
    document.addEventListener('click', handleDocClick)
    try {
      ;[currentState, allStates, config] = await Promise.all([
        invoke<StateView>('get_state'),
        invoke<StateSummary[]>('get_all_states'),
        invoke<AppConfig>('get_config'),
      ])
    } catch (e) {
      errorMsg = String(e)
    }

    // Fetch initial setup status; if either critical field is missing,
    // force the wizard on first launch. The user can re-open the wizard
    // later via the Setup button in the top bar.
    try {
      const status = await invoke<SetupStatus>('get_setup_status')
      setupStatus = status
      if (!status.has_provider || !status.has_live_resources_dir) {
        showWizard = true
        wizardForced = true
      }
    } catch (e) {
      // Non-fatal: if status fetch fails we just don't show the wizard.
      console.warn('get_setup_status failed:', e)
    }

    unlisteners = await Promise.all([
      listen<{ delta: string }>('llm-chunk', (ev) => {
        isStreaming = true
        streamingContent += ev.payload.delta
      }),

      listen<{ finish_reason: string; card?: { name: string; css?: string; content?: string; context?: string } }>('llm-done', (ev) => {
        if (streamingContent) {
          const card = ev.payload.card
          if (card) {
            messages = [
              ...messages,
              {
                id: uid(),
                role: 'card',
                content: card.content ?? streamingContent,
                toolName: card.name,
                toolArgs: { css: card.css, rawContent: streamingContent, context: card.context },
              },
            ]
          } else {
            messages = [
              ...messages,
              { id: uid(), role: 'assistant', content: streamingContent },
            ]
          }
          streamingContent = ' '
        }
        isStreaming = false
      }),

      listen<StateView>('state-changed', (ev) => {
        currentState = ev.payload
      }),

      listen<{ name: string; css: string }[]>('card-css-reloaded', (ev) => {
        for (const { name, css } of ev.payload) {
          // Convert "EligibilityOverview" → "eligibility-overview" (matches CardBubble slugify)
          const slug = name
            .replace(/([A-Z])/g, (m: string, p1: string, offset: number) =>
              (offset > 0 ? '-' : '') + p1.toLowerCase())
            .replace(/[^a-z0-9-]/g, '-')
            .replace(/-+/g, '-')
            .replace(/^-|-$/g, '')
          const el = document.getElementById(`card-style-${slug}`) as HTMLStyleElement | null
          if (el) el.textContent = css
        }
      }),

      listen<StateSummary[]>('playground-reloaded', async (ev) => {
        allStates = ev.payload
        // Re-fetch current state in case it changed due to deletion fallback
        try {
          currentState = await invoke<StateView>('get_state')
        } catch {
          // ignore
        }
      }),

      listen<{ name: string; server?: string; args: unknown; result: string }>('tool-called', (ev) => {
        messages = [
          ...messages,
          {
            id: uid(),
            role: 'tool-call',
            content: ev.payload.result,
            toolName: ev.payload.name,
            toolServer: ev.payload.server,
            toolArgs: ev.payload.args,
            toolResult: ev.payload.result,
          },
        ]
      }),

      listen<{ spec_tools_enabled: boolean }>('mcp-router-rebuilt', async (_ev) => {
        // Refresh the setup status so the top-bar indicator and the
        // banner reflect that spec tools are now available.
        try {
          setupStatus = await invoke<SetupStatus>('get_setup_status')
        } catch (e) {
          console.warn('get_setup_status refresh failed:', e)
        }
      }),

      listen<{ message: string }>('error', (ev) => {
        errorMsg = ev.payload.message
        isStreaming = false
      }),

      listen<ServiceStepEvent>('service-step-changed', (ev) => {
        serviceStep = ev.payload
      }),

      listen<UiInputRequest>('ui-input-requested', (ev) => {
        uiInputRequest = ev.payload
      }),
    ])
  })

  onDestroy(() => {
    document.removeEventListener('click', handleDocClick)
    unlisteners.forEach((fn) => fn())
  })

  // ── Actions ──────────────────────────────────────────────────────────────
  async function sendMessage(content: string) {
    if (!content.trim() || isStreaming) return
    errorMsg = ''
    messages = [...messages, { id: uid(), role: 'user', content }]
    isStreaming = true
    try {
      await invoke('send_message', { content })
    } catch (e) {
      errorMsg = String(e)
      isStreaming = false
    }
  }

  async function changeState(name: string) {
    try {
      currentState = await invoke<StateView>('set_state', { target: name })
    } catch (e) {
      errorMsg = String(e)
    }
  }

  async function saveConfig(updated: AppConfig) {
    try {
      const previousDir = config?.live_resources_dir ?? null
      await invoke('set_config', { config: updated })
      // If live_resources_dir changed, restart the state sidecar to match the new path.
      // This is a no-op when the value hasn't changed.
      if ((updated.live_resources_dir ?? null) !== previousDir) {
        await invoke('set_live_resources_dir', { path: updated.live_resources_dir ?? null })
      }
      config = updated
      showConfig = false
      // Refresh setup status so the top-bar indicator reflects the new
      // configuration (spec_tools_ready comes back async via the
      // mcp-router-rebuilt listener, but has_provider / has_live_resources_dir
      // are deterministic from the config we just saved).
      try {
        setupStatus = await invoke<SetupStatus>('get_setup_status')
      } catch {
        // non-fatal
      }
    } catch (e) {
      errorMsg = String(e)
    }
  }

  // Called by the wizard's "Done" button. Same wiring as saveConfig, but
  // closes the wizard instead of the config panel and refreshes status.
  async function wizardComplete(updated: AppConfig) {
    await saveConfig(updated)
    showWizard = false
    wizardForced = false
  }

  function openWizard() {
    showWizard = true
    wizardForced = false
  }

  function closeWizard() {
    showWizard = false
    wizardForced = false
  }

  // Incomplete-setup banner visibility: shown after the user has dismissed
  // the wizard but the configuration is still missing critical fields.
  let setupBanner = $derived(
    setupStatus !== null &&
      (!setupStatus.has_provider || !setupStatus.has_live_resources_dir) &&
      !showWizard,
  )

  async function clearChat() {
    await invoke('clear_conversation')
    messages = []
    streamingContent = ''
    isStreaming = false
  }
</script>

<div class="flex flex-col h-full bg-gray-50 text-gray-900">
  <!-- Top bar -->
  <header class="flex items-center gap-3 px-4 py-2 bg-white border-b border-gray-200 shadow-sm">
    <div class="flex-1 min-w-0">
      {#if currentState}
        <StateIndicator state={currentState} />
      {/if}
    </div>
    <button
      onclick={clearChat}
      class="text-xs text-gray-500 hover:text-gray-700 px-2 py-1 rounded hover:bg-gray-100"
    >
      Clear
    </button>
    <button
      onclick={openWizard}
      class="relative text-xs px-2 py-1 rounded hover:bg-gray-100 text-gray-500 hover:text-gray-700"
      title="Run the setup wizard"
    >
      ⚙ Setup
      {#if setupStatus && (!setupStatus.has_provider || !setupStatus.has_live_resources_dir)}
        <span
          class="absolute top-0.5 right-0.5 block h-2 w-2 rounded-full bg-red-500 ring-2 ring-white"
          aria-label="Setup incomplete"
        ></span>
      {/if}
    </button>
    <!--
      View-mode toggle. Switches the chat between rendering structured
      cards (with their custom CSS chrome) and plain markdown text.
      Aria-pressed makes the active state explicit for screen readers.
    -->
    <button
      type="button"
      onclick={() => (cardView = cardView === 'cards' ? 'text' : 'cards')}
      title={cardView === 'cards'
        ? 'Switch to text view — render responses as plain markdown'
        : 'Switch to card view — render responses with structured card styling'}
      aria-label={cardView === 'cards' ? 'Switch to text view' : 'Switch to card view'}
      aria-pressed={cardView === 'text'}
      class="text-xs px-2 py-1 rounded hover:bg-gray-100 {cardView === 'text'
        ? 'text-blue-600 bg-blue-50'
        : 'text-gray-500 hover:text-gray-700'}"
    >
      {cardView === 'cards' ? '🎴 Cards' : '📄 Text'}
    </button>
    <button
      onclick={() => { showConfig = !showConfig; if (showConfig) showPlayground = false }}
      class="text-xs px-2 py-1 rounded hover:bg-gray-100 {showConfig ? 'text-blue-600 bg-blue-50' : 'text-gray-500 hover:text-gray-700'}"
    >
      ⚙ Config
    </button>
    <button
      onclick={() => { showPlayground = !showPlayground; if (showPlayground) showConfig = false }}
      class="text-xs px-2 py-1 rounded hover:bg-gray-100 {showPlayground ? 'text-blue-600 bg-blue-50' : 'text-gray-500 hover:text-gray-700'}"
    >
      ✎ Playground
    </button>
  </header>

  {#if errorMsg}
    <div class="bg-red-50 border-b border-red-200 px-4 py-2 text-sm text-red-700">
      {errorMsg}
      <button onclick={() => (errorMsg = '')} class="ml-2 text-red-500 hover:text-red-700">✕</button>
    </div>
  {/if}

  {#if setupBanner && setupStatus}
    <div class="bg-amber-50 border-b border-amber-200 px-4 py-2 text-sm text-amber-900 flex items-center gap-3">
      <span>
        Setup incomplete —
        {#if !setupStatus.has_provider && !setupStatus.has_live_resources_dir}
          LLM provider and Live Resources directory are missing.
        {:else if !setupStatus.has_provider}
          LLM provider is missing.
        {:else}
          Live Resources directory is missing.
        {/if}
      </span>
      <button
        onclick={openWizard}
        class="text-xs px-2 py-1 rounded bg-amber-600 text-white hover:bg-amber-700"
      >
        Open Setup
      </button>
    </div>
  {/if}

  <div class="flex flex-1 overflow-hidden">
    <!-- Left sidebar: state selector -->
    <aside class="w-44 flex-shrink-0 bg-white border-r border-gray-200 overflow-y-auto">
      <StateSelector
        {allStates}
        currentStateName={currentState?.name ?? ''}
        onSelect={changeState}
      />
    </aside>

    <!-- Main chat area -->
    <main class="flex-1 flex flex-col overflow-hidden relative">
      <ChatWindow
        {messages}
        {streamingContent}
        {isStreaming}
        {cardView}
        onSend={sendMessage}
      />
      {#if uiInputRequest}
        <UiInputForm
          request={uiInputRequest}
          onSubmit={() => { uiInputRequest = null }}
        />
      {/if}
    </main>

    <!-- Config panel -->
    {#if showConfig && config}
      <aside class="w-80 flex-shrink-0 bg-white border-l border-gray-200 overflow-y-auto">
        <ConfigPanel {config} onSave={saveConfig} onClose={() => (showConfig = false)} />
      </aside>
    {/if}

    <!-- Playground panel -->
    {#if showPlayground}
      <div
        role="separator"
        aria-orientation="vertical"
        class="w-1 flex-shrink-0 bg-gray-200 hover:bg-blue-400 active:bg-blue-500 cursor-col-resize transition-colors"
        onmousedown={startResize}
      ></div>
      <aside
        style="width: {playgroundWidth}px"
        class="flex-shrink-0 border-l border-gray-200 overflow-hidden flex flex-col"
      >
        <PlaygroundPanel onClose={() => (showPlayground = false)} />
      </aside>
    {/if}
  </div>
</div>

<!-- Setup wizard overlay. First launch: wizardForced=true (no close
     button, chat inaccessible). After dismissal: user can re-open via
     the top-bar Setup button. -->
{#if showWizard && config}
  <SetupWizard
    {config}
    isFirstLaunch={wizardForced}
    onComplete={wizardComplete}
    onClose={closeWizard}
  />
{/if}

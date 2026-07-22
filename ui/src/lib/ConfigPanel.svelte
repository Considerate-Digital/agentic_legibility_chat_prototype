<script lang="ts">
  import { untrack } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import type { AppConfig } from '../types'

  function ensureAnalyser(d: AppConfig) {
    if (!d.analyser) d.analyser = { model: '', base_url: undefined, api_key: undefined }
  }

  function clearAnalyserIfEmpty(d: AppConfig) {
    if (d.analyser && !d.analyser.model && !d.analyser.base_url && !d.analyser.api_key) {
      d.analyser = undefined
    }
  }

  const {
    config,
    onSave,
    onClose,
  }: {
    config: AppConfig
    onSave: (c: AppConfig) => void
    onClose: () => void
  } = $props()

  // untrack: intentional one-time snapshot — draft is an independent editable copy
  let draft = $state<AppConfig>(untrack(() => JSON.parse(JSON.stringify(config))))

  let pickerBusy = $state(false)
  let pickerError = $state('')

  let resetBusy = $state(false)
  let resetError = $state('')

  async function browseSpecsDir() {
    pickerBusy = true
    pickerError = ''
    try {
      const picked = await invoke<string | null>('pick_live_resources_dir')
      if (picked !== null) {
        draft.live_resources_dir = picked
      }
    } catch (e) {
      pickerError = String(e)
    } finally {
      pickerBusy = false
    }
  }

  function clearSpecsDir() {
    draft.live_resources_dir = undefined
  }

  async function resetToDefaults() {
    // Two-step confirmation: the first prompt is a plain confirm(); if the
    // user types anything other than the required word, we abort. Belt
    // and braces because this wipes user-edited .md files.
    const ok1 = confirm(
      'Reset states, tools, and cards to the bundled defaults?\n\n' +
        'This will DELETE every .md file in your states/, tools/, and ' +
        'cards/ directories and replace them with the shipped defaults. ' +
        'Any customisations you made will be lost.',
    )
    if (!ok1) return

    resetBusy = true
    resetError = ''
    try {
      await invoke('reset_to_defaults')
      // Close the panel — the parent will receive `playground-reloaded`
      // and re-fetch state/card lists.
      onClose()
    } catch (e) {
      resetError = String(e)
    } finally {
      resetBusy = false
    }
  }

  function save() {
    clearAnalyserIfEmpty(draft)
    onSave(draft)
  }
</script>

<div class="flex flex-col h-full">
  <div class="flex items-center justify-between px-4 py-3 border-b border-gray-200">
    <h2 class="text-sm font-semibold text-gray-700">LLM Configuration</h2>
    <button onclick={onClose} class="text-gray-400 hover:text-gray-600 text-lg leading-none">✕</button>
  </div>

  <div class="flex-1 overflow-y-auto p-4 space-y-4">
    <label class="block">
      <span class="text-xs font-medium text-gray-600">Base URL</span>
      <input
        bind:value={draft.provider.base_url}
        type="text"
        placeholder="https://api.openai.com/v1"
        class="mt-1 w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
      <p class="mt-1 text-xs text-gray-400">
        Use the API root (not the full endpoint). The app always appends <code class="bg-gray-100 px-1 rounded">/chat/completions</code>.
      </p>
      <p class="mt-1 text-xs text-gray-400">
        OpenAI: <code class="bg-gray-100 px-1 rounded">https://api.openai.com/v1</code><br />
        Anthropic: <code class="bg-gray-100 px-1 rounded">https://api.anthropic.com/v1</code><br />
        OpenRouter: <code class="bg-gray-100 px-1 rounded">https://openrouter.ai/api/v1</code>
      </p>
    </label>

    <label class="block">
      <span class="text-xs font-medium text-gray-600">API Key</span>
      <input
        bind:value={draft.provider.api_key}
        type="password"
        placeholder="sk-…"
        class="mt-1 w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
    </label>

    <label class="block">
      <span class="text-xs font-medium text-gray-600">Model</span>
      <input
        bind:value={draft.provider.model}
        type="text"
        placeholder="gpt-4o"
        class="mt-1 w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
      <p class="mt-1 text-xs text-gray-400">
        e.g. <code class="bg-gray-100 px-1 rounded">gpt-4o</code>, <code class="bg-gray-100 px-1 rounded">claude-3-5-sonnet-20241022</code>.
        For OpenRouter, prefix the model with its provider: <code class="bg-gray-100 px-1 rounded">anthropic/claude-3.5-sonnet</code>, <code class="bg-gray-100 px-1 rounded">openai/gpt-4o</code>, <code class="bg-gray-100 px-1 rounded">google/gemini-pro-1.5</code>.
      </p>
    </label>

    <div class="border-t border-gray-100 pt-4">
      <p class="text-xs font-semibold text-gray-600 mb-1">State analyser model</p>
      <p class="text-xs text-gray-400 mb-3">
        A cheap/fast model used only for state evaluation before each response.
        Falls back to the main model if left blank.
      </p>

      <label class="block mb-3">
        <span class="text-xs text-gray-500">Analyser model</span>
        <input
          value={draft.analyser?.model ?? ''}
          oninput={(e) => { ensureAnalyser(draft); draft.analyser!.model = (e.target as HTMLInputElement).value }}
          type="text"
          placeholder="gpt-4o-mini"
          class="mt-1 w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      </label>

      <label class="block mb-3">
        <span class="text-xs text-gray-500">Analyser base URL (optional)</span>
        <input
          value={draft.analyser?.base_url ?? ''}
          oninput={(e) => { ensureAnalyser(draft); draft.analyser!.base_url = (e.target as HTMLInputElement).value || undefined }}
          type="text"
          placeholder="Uses main provider URL"
          class="mt-1 w-full text-xs border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      </label>

      <label class="block">
        <span class="text-xs text-gray-500">Analyser API key (optional)</span>
        <input
          value={draft.analyser?.api_key ?? ''}
          oninput={(e) => { ensureAnalyser(draft); draft.analyser!.api_key = (e.target as HTMLInputElement).value || undefined }}
          type="password"
          placeholder="Uses main API key"
          class="mt-1 w-full text-xs border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      </label>
    </div>

    <div class="border-t border-gray-100 pt-4">
      <p class="text-xs font-medium text-gray-600 mb-2">Runtime overrides (optional)</p>

      <label class="block mb-3">
        <span class="text-xs text-gray-500">States directory</span>
        <input
          bind:value={draft.states_override_dir}
          type="text"
          placeholder="~/.config/legibility-chat/states"
          class="mt-1 w-full text-xs border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      </label>

      <label class="block">
        <span class="text-xs text-gray-500">Tools directory</span>
        <input
          bind:value={draft.tools_override_dir}
          type="text"
          placeholder="~/.config/legibility-chat/tools"
          class="mt-1 w-full text-xs border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      </label>
      <label class="block">
        <span class="text-xs text-gray-500">Cards directory</span>
        <input
          bind:value={draft.cards_override_dir}
          type="text"
          placeholder="~/.config/legibility-chat/cards"
          class="mt-1 w-full text-xs border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      </label>

      <label class="flex items-center gap-2 mt-2">
        <input type="checkbox" bind:checked={draft.cards_enabled} />
        <span class="text-xs text-gray-500">
          Let the assistant rewrite responses into UI cards
        </span>
      </label>
      <p class="mt-1 text-xs text-gray-400">
        When off, responses are shown as streamed — no extra rewrite call, no
        card formatting.
      </p>

      <label class="block mt-3">
        <span class="text-xs text-gray-500">Live resources directory (spec tools)</span>
        <div class="mt-1 flex gap-2">
          <input
            bind:value={draft.live_resources_dir}
            type="text"
            placeholder="Pick a directory containing endpoints/, services/, plans/"
            class="flex-1 text-xs border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-emerald-500"
          />
          <button
            type="button"
            onclick={browseSpecsDir}
            disabled={pickerBusy}
            class="text-xs px-3 py-2 rounded-lg bg-emerald-600 text-white hover:bg-emerald-700 disabled:bg-gray-300 disabled:cursor-not-allowed"
          >
            {pickerBusy ? '…' : 'Browse…'}
          </button>
          {#if draft.live_resources_dir}
            <button
              type="button"
              onclick={clearSpecsDir}
              class="text-xs px-3 py-2 rounded-lg border border-gray-300 text-gray-600 hover:bg-gray-100"
              title="Detach spec tools"
            >
              Clear
            </button>
          {/if}
        </div>
        {#if pickerError}
          <p class="mt-1 text-xs text-red-600">{pickerError}</p>
        {/if}
        <p class="mt-1 text-xs text-gray-400">
          When unset, only the legibility-chat-mcp tools are available. Saving a path
          here will restart the state sidecar pointed at it.
        </p>
      </label>

      {#if !draft.live_resources_dir}
        <div class="mt-3 px-3 py-2 rounded-lg bg-amber-50 border border-amber-200 text-xs text-amber-800">
          Pick a <code>live_resources/</code> directory to enable spec search
          (the spec-lookup tools).
        </div>
      {/if}

      <!-- Reset to defaults: destructive action. Wipes user-edited .md
           files in states/, tools/, cards/ and re-copies from the bundled
           resource. Hidden behind a confirm() prompt in resetToDefaults(). -->
      <div class="mt-4 pt-3 border-t border-gray-100">
        <button
          type="button"
          onclick={resetToDefaults}
          disabled={resetBusy}
          class="w-full text-xs px-3 py-2 rounded-lg border border-red-200 text-red-700 hover:bg-red-50 disabled:opacity-50 disabled:cursor-not-allowed"
          title="Replace states, tools, and cards with the bundled defaults"
        >
          {resetBusy ? 'Resetting…' : 'Reset states / tools / cards to defaults'}
        </button>
        {#if resetError}
          <p class="mt-1 text-xs text-red-600">{resetError}</p>
        {/if}
        <p class="mt-1 text-xs text-gray-400">
          Useful when iterating on the bundled prompts. Wipes your
          customisations — back up first if you care.
        </p>
      </div>

    </div>
  </div>

  <div class="p-4 border-t border-gray-200">
    <button
      onclick={save}
      class="w-full bg-blue-600 text-white rounded-lg py-2 text-sm font-medium hover:bg-blue-700 transition-colors"
    >
      Save
    </button>
  </div>
</div>

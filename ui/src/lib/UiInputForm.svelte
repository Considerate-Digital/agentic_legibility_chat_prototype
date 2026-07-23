<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import type { UiInputRequest } from '../types'

  const {
    request,
    onSubmit,
  }: {
    request: UiInputRequest
    onSubmit: () => void
  } = $props()

  let value = $state('')
  let selectedOption = $state('')
  let submitting = $state(false)

  const isSelect = $derived(request.input_type === 'select' && (request.options?.length ?? 0) > 0)

  async function submit() {
    const submitted = isSelect ? selectedOption : value.trim()
    if (!submitted || submitting) return
    submitting = true
    await invoke('submit_ui_input', { value: submitted })
    onSubmit()
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !isSelect) {
      e.preventDefault()
      submit()
    }
  }
</script>

<div class="absolute inset-x-0 bottom-0 z-10 border-t border-blue-200 bg-blue-50 px-4 py-3 shadow-lg">
  <p class="mb-2 text-xs font-semibold uppercase tracking-wide text-blue-600">Input required</p>
  <label class="mb-1 block text-sm font-medium text-gray-700">
    {request.description}
  </label>

  {#if isSelect}
    <div class="mb-2 flex flex-wrap gap-2">
      {#each request.options as opt}
        <button
          onclick={() => { selectedOption = opt }}
          class="rounded-lg border px-3 py-1.5 text-sm transition-colors
                 {selectedOption === opt
                   ? 'border-blue-500 bg-blue-600 text-white'
                   : 'border-gray-300 bg-white text-gray-700 hover:border-blue-400'}"
        >
          {opt}
        </button>
      {/each}
    </div>
  {:else}
    <input
      type={request.input_type === 'number' ? 'number' : request.input_type === 'date' ? 'date' : request.input_type === 'email' ? 'email' : 'text'}
      bind:value
      onkeydown={onKeydown}
      placeholder={request.description}
      disabled={submitting}
      class="mb-2 w-full rounded-xl border border-gray-300 px-3 py-2 text-sm
             focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent
             disabled:bg-gray-50"
    />
  {/if}

  <div class="flex justify-end">
    <button
      onclick={submit}
      disabled={submitting || (isSelect ? !selectedOption : !value.trim())}
      class="rounded-xl bg-blue-600 px-4 py-2 text-sm font-medium text-white
             hover:bg-blue-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
    >
      {submitting ? 'Sending…' : 'Submit'}
    </button>
  </div>
</div>

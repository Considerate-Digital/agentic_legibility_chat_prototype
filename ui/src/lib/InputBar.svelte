<script lang="ts">
  const {
    isStreaming,
    onSend,
  }: {
    isStreaming: boolean
    onSend: (content: string) => void
  } = $props()

  let value = $state('')

  function submit() {
    const trimmed = value.trim()
    if (!trimmed || isStreaming) return
    onSend(trimmed)
    value = ''
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      submit()
    }
  }
</script>

<div class="flex items-end gap-2 p-3">
  <textarea
    bind:value
    onkeydown={onKeydown}
    disabled={isStreaming}
    rows="1"
    placeholder="Describe your situation… (Enter to send, Shift+Enter for new line)"
    class="flex-1 resize-none rounded-xl border border-gray-300 px-3 py-2 text-sm
           focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent
           disabled:bg-gray-50 disabled:text-gray-400
           max-h-40 overflow-y-auto"
    style="field-sizing: content;"
  ></textarea>
  <button
    onclick={submit}
    disabled={isStreaming || !value.trim()}
    class="flex-shrink-0 bg-blue-600 text-white rounded-xl px-4 py-2 text-sm font-medium
           hover:bg-blue-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
  >
    {isStreaming ? '…' : 'Send'}
  </button>
</div>

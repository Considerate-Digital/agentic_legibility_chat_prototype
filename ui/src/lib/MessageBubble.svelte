<script lang="ts">
  import type { Message } from '../types'
  import { renderMarkdown } from './markdown'
  import CardBubble from './CardBubble.svelte'

  const {
    msg,
    streaming = false,
    cardView = 'cards',
  }: {
    msg: Message
    streaming?: boolean
    /**
     * Visual rendering mode for `role === 'card'` messages:
     * - `'cards'` (default): render the styled `<CardBubble>` with its
     *   custom CSS chrome.
     * - `'text'`: render the underlying markdown text in a plain prose
     *   bubble — same styling as a regular assistant message. Used when
     *   the user has flipped the top-bar toggle to text mode.
     *
     * Streaming bubbles ignore `cardView` because they have no card
     * metadata yet (just raw markdown being typed out).
     */
    cardView?: 'cards' | 'text'
  } = $props()

  let toolExpanded = $state(false)

  // Re-render markdown whenever content changes (important during streaming)
  const renderedHtml = $derived(
    msg.role === 'assistant' || streaming
      ? renderMarkdown(msg.content)
      : ''
  )
</script>

{#if msg.role === 'user'}
  <div class="flex justify-end">
    <div class="max-w-[80%] bg-blue-600 text-white rounded-2xl rounded-tr-sm px-4 py-2.5 text-sm whitespace-pre-wrap">
      {msg.content}
    </div>
  </div>

{:else if msg.role === 'assistant'}
  <div class="flex">
    <div class="max-w-[80%] bg-white  rounded-2xl rounded-tl-sm px-4 py-2.5">
      <!-- prose class applies @tailwindcss/typography styles -->
      <div class="prose">
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        {@html renderedHtml}
        {#if streaming}
          <span class="inline-block w-0.5 h-4 bg-gray-500 animate-pulse ml-0.5 align-middle"></span>
        {/if}
      </div>
    </div>
  </div>

{:else if msg.role === 'tool-call'}
  <div class="flex justify-center">
    <details
      class="max-w-[90%] w-full bg-gray-50 border border-gray-200 rounded-lg text-xs"
      bind:open={toolExpanded}
    >
      <summary class="px-3 py-2 cursor-pointer text-gray-500 hover:text-gray-700 select-none list-none flex items-center justify-between">
        <span class="flex items-center gap-2">
          🔧 <span class="font-mono font-medium text-gray-700">{msg.toolName}</span>
          {#if msg.toolServer}
            <span
              class="inline-block w-2 h-2 rounded-full bg-blue-500"
              title="Source: {msg.toolServer} MCP server"
              aria-label="Source: {msg.toolServer} MCP server"
            ></span>
          {/if}
        </span>
        <span class="text-gray-400 text-[10px]">{toolExpanded ? '▲ collapse' : '▼ expand'}</span>
      </summary>
      {#if toolExpanded}
        <div class="px-3 pb-3 space-y-2 border-t border-gray-200 pt-2">
          {#if msg.toolArgs && Object.keys(msg.toolArgs as object).length > 0}
            <div>
              <p class="text-gray-400 font-medium mb-1">Arguments</p>
              <pre class="bg-white border border-gray-100 rounded p-2 overflow-x-auto text-gray-700 text-[11px]">{JSON.stringify(msg.toolArgs, null, 2)}</pre>
            </div>
          {/if}
          <div>
            <p class="text-gray-400 font-medium mb-1">Result</p>
            <pre class="bg-white border border-gray-100 rounded p-2 overflow-x-auto text-gray-700 whitespace-pre-wrap text-[11px]">{msg.toolResult}</pre>
          </div>
        </div>
      {/if}
    </details>
  </div>

{:else if msg.role === 'card'}
  {#if cardView === 'cards'}
    <CardBubble message={msg} />
  {:else}
    <!-- Text-mode fallback: render the card's underlying markdown in a
         plain bubble. Same prose styling as a regular assistant message.
         `msg.content` was set by the llm-done handler to the card's
         markdown payload, so it's the natural text representation. -->
    <div class="flex">
      <div class="max-w-[80%] bg-white border border-gray-200 rounded-2xl rounded-tl-sm px-4 py-2.5 shadow-sm">
        <div class="prose">
          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
          {@html renderMarkdown(msg.content)}
        </div>
      </div>
    </div>
  {/if}

{:else if msg.role === 'error'}
  <div class="flex justify-center">
    <div class="bg-red-50 border border-red-200 rounded-lg px-3 py-2 text-xs text-red-700">
      {msg.content}
    </div>
  </div>
{/if}

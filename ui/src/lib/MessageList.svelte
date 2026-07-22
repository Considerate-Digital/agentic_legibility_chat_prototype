<script lang="ts">
  import type { Message } from '../types'
  import MessageBubble from './MessageBubble.svelte'

  const {
    messages,
    streamingContent,
    isStreaming,
    cardView = 'cards',
  }: {
    messages: Message[]
    streamingContent: string
    isStreaming: boolean
    /**
     * Card-vs-text rendering mode for `role === 'card'` messages.
     * Forwarded to each `<MessageBubble>` and to the streaming bubble
     * (which is a no-op there since it has no card metadata).
     */
    cardView?: 'cards' | 'text'
  } = $props()

  let listEl = $state<HTMLDivElement | null>(null)
  // True while the user is at (or near) the bottom — auto-scroll follows content
  let isAtBottom = $state(true)

  function onScroll() {
    if (!listEl) return
    const { scrollTop, scrollHeight, clientHeight } = listEl
    // Consider "at bottom" if within 60px of the end
    isAtBottom = scrollHeight - scrollTop - clientHeight < 60
  }

  function scrollToBottom() {
    if (listEl) listEl.scrollTop = listEl.scrollHeight
  }

  $effect(() => {
    // Re-run whenever messages or streaming content change
    void messages.length
    void streamingContent
    if (isAtBottom) {
      // Use a microtask so the DOM has updated before we measure
      queueMicrotask(scrollToBottom)
    }
  })
</script>

<div
  bind:this={listEl}
  onscroll={onScroll}
  class="flex flex-col gap-3 p-4 h-full overflow-y-auto"
>
  {#if messages.length === 0 && !isStreaming}
    <div class="flex flex-1 items-center justify-center text-gray-400 text-sm py-16">
      Start a conversation — describe your situation with UK government services.
    </div>
  {/if}

  {#each messages as msg (msg.id)}
    <MessageBubble {msg} {cardView} />
  {/each}

  <!-- Streaming assistant bubble -->
  {#if isStreaming && streamingContent}
    <div class="flex">
      <div class="max-w-[80%] bg-white border border-gray-200 rounded-2xl rounded-tl-sm px-4 py-2.5 shadow-sm">
        <MessageBubble msg={{ id: '__streaming__', role: 'assistant', content: streamingContent }} streaming />
      </div>
    </div>
  {:else if isStreaming}
    <div class="flex">
      <div class="bg-white border border-gray-200 rounded-2xl rounded-tl-sm px-4 py-2.5 text-sm shadow-sm text-gray-400 flex items-center gap-1.5">
        <span class="w-1.5 h-1.5 bg-gray-400 rounded-full animate-bounce [animation-delay:0ms]"></span>
        <span class="w-1.5 h-1.5 bg-gray-400 rounded-full animate-bounce [animation-delay:150ms]"></span>
        <span class="w-1.5 h-1.5 bg-gray-400 rounded-full animate-bounce [animation-delay:300ms]"></span>
      </div>
    </div>
  {/if}

  <!-- Scroll-to-bottom button shown when user has scrolled up -->
  {#if !isAtBottom}
    <button
      onclick={() => { isAtBottom = true; scrollToBottom() }}
      class="sticky bottom-2 self-center bg-blue-600 text-white text-xs px-3 py-1.5 rounded-full shadow-md hover:bg-blue-700 transition-colors z-10"
    >
      ↓ Jump to latest
    </button>
  {/if}
</div>

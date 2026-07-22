<script lang="ts">
  import type { Message } from '../types'
  import MessageList from './MessageList.svelte'
  import InputBar from './InputBar.svelte'

  const {
    messages,
    streamingContent,
    isStreaming,
    onSend,
    cardView = 'cards',
  }: {
    messages: Message[]
    streamingContent: string
    isStreaming: boolean
    onSend: (content: string) => void
    /**
     * Card-vs-text rendering mode. Forwarded to `<MessageList>` which
     * passes it to each `<MessageBubble>`.
     */
    cardView?: 'cards' | 'text'
  } = $props()
</script>

<div class="flex flex-col h-full">
  <div class="flex-1 overflow-y-auto">
    <MessageList {messages} {streamingContent} {isStreaming} {cardView} />
  </div>
  <div class="border-t border-gray-200 bg-white">
    <InputBar {isStreaming} onSend={onSend} />
  </div>
</div>

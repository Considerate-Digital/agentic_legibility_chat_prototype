<script lang="ts">
  import { renderMarkdown, renderCardMarkdown } from './markdown'
  import type { Message } from '../types'

  const { message }: { message: Message } = $props()

  function slugify(name: string): string {
    return name
      .replace(/([A-Z])/g, (m, p1, offset) => (offset > 0 ? '-' : '') + p1.toLowerCase())
      .replace(/[^a-z0-9-]/g, '-')
      .replace(/-+/g, '-')
      .replace(/^-|-$/g, '')
  }

  const args = $derived(message.toolArgs as { css?: string; rawContent?: string; context?: string } | undefined)
  const slug = $derived(slugify(message.toolName ?? ''))
  const css = $derived(args?.css)
  const rawContent = $derived(args?.rawContent ?? '')
  const context = $derived(args?.context ?? '')
  const hasSplit = $derived(!!rawContent && rawContent !== message.content)

  const rendered = $derived(renderCardMarkdown(message.content))
  const renderedRaw = $derived(rawContent ? renderMarkdown(rawContent) : '')

  let splitView = $state(false)
</script>

{#if css}
  {@html `<style id="card-style-${slug}">${css}</style>`}
{/if}

<div class="mx-4 my-2 rounded-lg card-{slug}">
  <!-- Header -->
  <div class="card-{slug}-header px-3 py-1.5 text-xs font-semibold border-b flex items-center justify-between">
    <span>🃏 {message.toolName}</span>
    {#if hasSplit}
      <button
        onclick={() => (splitView = !splitView)}
        title={splitView ? 'Collapse split view' : 'Show original response alongside card'}
        class="ml-2 opacity-60 hover:opacity-100 transition-opacity text-sm leading-none"
      >{splitView ? '⊟' : '⊞'}</button>
    {/if}
  </div>

  <!-- Context summary — always shown when present -->
  {#if context}
    <div class="px-3 py-2 text-sm text-gray-600 italic border-b border-gray-100 bg-white/50">
      {context}
    </div>
  {/if}

  <!-- Body — single or split -->
  {#if splitView}
    <div class="flex">
      <div class="flex-1 border-r border-gray-200 px-3 py-2 prose prose-sm max-w-none bg-white/70">
        <p class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-2 not-prose">Original</p>
        {@html renderedRaw}
      </div>
      <div class="flex-1 card-{slug}-body px-3 py-2 prose prose-sm max-w-none">
        <p class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-2 not-prose">Card</p>
        {@html rendered}
      </div>
    </div>
  {:else}
    <div class="card-{slug}-body px-3 py-2 prose prose-sm max-w-none">
      {@html rendered}
    </div>
  {/if}
</div>

<script lang="ts">
  import type { StateView } from '../types'
  import { stateColors } from './stateColors'

  const { state }: { state: StateView } = $props()

  const badgeClass = $derived(stateColors(state.name).badge)
</script>

<div class="flex items-center gap-3 min-w-0">
  <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-semibold border flex-shrink-0 {badgeClass}">
    {state.name}
  </span>
  <span class="text-sm text-gray-500 truncate">{state.description}</span>
  {#if state.valid_transitions.length > 0}
    <div class="hidden sm:flex gap-1 flex-shrink-0">
      {#each state.valid_transitions as t}
        <span class="text-xs text-gray-400 bg-gray-100 rounded px-1.5 py-0.5">{t}</span>
      {/each}
    </div>
  {/if}
</div>

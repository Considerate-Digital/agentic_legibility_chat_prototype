<script lang="ts">
  import type { StateSummary } from '../types'
  import { stateColors } from './stateColors'

  const {
    allStates,
    currentStateName,
    onSelect,
  }: {
    allStates: StateSummary[]
    currentStateName: string
    onSelect: (name: string) => void
  } = $props()
</script>

<nav class="p-2">
  <p class="text-xs font-semibold text-gray-400 uppercase tracking-wider px-2 py-1">States</p>
  {#each allStates as s}
    <button
      onclick={() => onSelect(s.name)}
      class="w-full text-left px-2 py-1.5 rounded text-sm flex items-center gap-2 transition-colors
        {s.name === currentStateName
          ? 'bg-gray-100 text-gray-900 font-medium'
          : 'text-gray-600 hover:bg-gray-50 hover:text-gray-800'}"
      title={s.description}
    >
      <span class="w-2 h-2 rounded-full flex-shrink-0 {stateColors(s.name).dot}"></span>
      {s.name}
    </button>
  {/each}
</nav>

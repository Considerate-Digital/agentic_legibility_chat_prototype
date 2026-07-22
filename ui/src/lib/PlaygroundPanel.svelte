<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { EditorView, basicSetup } from 'codemirror'
  import { markdown } from '@codemirror/lang-markdown'
  import { oneDark } from '@codemirror/theme-one-dark'
  import type { PlaygroundFiles } from '../types'

  const { onClose }: { onClose: () => void } = $props()

  // ── State ──────────────────────────────────────────────────────────────
  let files = $state<PlaygroundFiles>({ states: [], tools: [], cards: [] })
  let selectedKind = $state<'state' | 'tool' | 'card' | null>(null)
  let selectedFile = $state<string | null>(null)
  let isDirty = $state(false)
  let isSaving = $state(false)
  let errorMsg = $state('')
  let isNewFile = $state(false)
  let newFilename = $state('')

  // ── CodeMirror ─────────────────────────────────────────────────────────
  let editorEl: HTMLDivElement
  let view: EditorView | null = null

  const STATE_TEMPLATE = `---
name: MyState
description: "What this state is for"
valid_transitions: [Idle]
tools: []
---

You are a helpful assistant.
`

  const TOOL_TEMPLATE = `---
name: my_tool
description: "What this tool does"
parameters:
  - name: input
    type: string
    description: "The input value"
    required: true
---

Extended description of what this tool does and when to use it.
`

  const CARD_TEMPLATE = `---
name: MyCard
description: "When to show this card"
relevant_states: []
---

Generate a card that...

\`\`\`css
.card-my-card {
  border: 2px solid #6366f1;
  background: #eef2ff;
  border-radius: 8px;
  overflow: hidden;
}
.card-my-card-header {
  background: #e0e7ff;
  color: #4338ca;
  font-weight: 600;
  font-size: 0.75rem;
  padding: 4px 12px;
  border-bottom: 1px solid #c7d2fe;
}
.card-my-card-body {
  padding: 8px 12px;
}
\`\`\`
`

  onMount(async () => {
    view = new EditorView({
      doc: '',
      extensions: [
        basicSetup,
        markdown(),
        oneDark,
        EditorView.lineWrapping,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) isDirty = true
        }),
      ],
      parent: editorEl,
    })

    await loadFileList()
  })

  onDestroy(() => {
    view?.destroy()
  })

  async function loadFileList() {
    try {
      files = await invoke<PlaygroundFiles>('list_playground_files')
    } catch (e) {
      errorMsg = String(e)
    }
  }

  async function openFile(kind: 'state' | 'tool' | 'card', filename: string) {
    try {
      const content = await invoke<string>('read_playground_file', { kind, filename })
      selectedKind = kind
      selectedFile = filename
      isNewFile = false
      isDirty = false
      setEditorContent(content)
      errorMsg = ''
    } catch (e) {
      errorMsg = String(e)
    }
  }

  function openNewFile(kind: 'state' | 'tool' | 'card') {
    selectedKind = kind
    selectedFile = null
    isNewFile = true
    newFilename = ''
    isDirty = false
    setEditorContent(
      kind === 'state' ? STATE_TEMPLATE :
      kind === 'card' ? CARD_TEMPLATE :
      TOOL_TEMPLATE
    )
    errorMsg = ''
  }

  async function saveFile() {
    if (!selectedKind || !view) return

    const filename = isNewFile ? ensureMdExtension(newFilename.trim()) : selectedFile!
    if (!filename) {
      errorMsg = 'Enter a filename'
      return
    }

    isSaving = true
    errorMsg = ''
    try {
      const content = view.state.doc.toString()
      await invoke('write_playground_file', { kind: selectedKind, filename, content })
      await loadFileList()
      selectedFile = filename
      isNewFile = false
      isDirty = false
    } catch (e) {
      errorMsg = String(e)
    } finally {
      isSaving = false
    }
  }

  async function deleteFile() {
    if (!selectedKind || !selectedFile) return
    if (!confirm(`Delete ${selectedFile}?`)) return

    try {
      await invoke('delete_playground_file', { kind: selectedKind, filename: selectedFile })
      await loadFileList()
      selectedFile = null
      selectedKind = null
      setEditorContent('')
      isDirty = false
      errorMsg = ''
    } catch (e) {
      errorMsg = String(e)
    }
  }

  function setEditorContent(content: string) {
    if (!view) return
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: content },
    })
    // Reset dirty tracking after programmatic update
    setTimeout(() => { isDirty = false }, 0)
  }

  function ensureMdExtension(name: string): string {
    return name.endsWith('.md') ? name : name + '.md'
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault()
      saveFile()
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex flex-col h-full" onkeydown={handleKeydown}>
  <!-- Header -->
  <div class="flex items-center justify-between px-3 py-2 border-b border-gray-200 bg-white">
    <span class="text-sm font-semibold text-gray-700">Playground</span>
    <button onclick={onClose} class="text-gray-400 hover:text-gray-600 text-lg leading-none">✕</button>
  </div>

  {#if errorMsg}
    <div class="bg-red-50 border-b border-red-200 px-3 py-1.5 text-xs text-red-700">
      {errorMsg}
    </div>
  {/if}

  <div class="flex flex-1 overflow-hidden">
    <!-- File list sidebar -->
    <div class="w-44 flex-shrink-0 border-r border-gray-200 bg-gray-50 overflow-y-auto flex flex-col">
      <!-- States section -->
      <div class="p-2">
        <div class="flex items-center justify-between px-1 mb-1">
          <span class="text-xs font-semibold text-gray-400 uppercase tracking-wider">States</span>
          <button
            onclick={() => openNewFile('state')}
            class="text-gray-400 hover:text-gray-600 text-sm leading-none"
            title="New state"
          >+</button>
        </div>
        {#each files.states as filename}
          <button
            onclick={() => openFile('state', filename)}
            class="w-full text-left px-2 py-1 rounded text-xs transition-colors truncate
              {selectedFile === filename && selectedKind === 'state'
                ? 'bg-blue-100 text-blue-800'
                : 'text-gray-600 hover:bg-gray-100'}"
          >{filename}</button>
        {/each}
      </div>

      <!-- Tools section -->
      <div class="p-2 border-t border-gray-200">
        <div class="flex items-center justify-between px-1 mb-1">
          <span class="text-xs font-semibold text-gray-400 uppercase tracking-wider">Tools</span>
          <button
            onclick={() => openNewFile('tool')}
            class="text-gray-400 hover:text-gray-600 text-sm leading-none"
            title="New tool"
          >+</button>
        </div>
        {#each files.tools as filename}
          <button
            onclick={() => openFile('tool', filename)}
            class="w-full text-left px-2 py-1 rounded text-xs transition-colors truncate
              {selectedFile === filename && selectedKind === 'tool'
                ? 'bg-blue-100 text-blue-800'
                : 'text-gray-600 hover:bg-gray-100'}"
          >{filename}</button>
        {/each}
      </div>

      <!-- Cards section -->
      <div class="p-2 border-t border-gray-200">
        <div class="flex items-center justify-between px-1 mb-1">
          <span class="text-xs font-semibold text-gray-400 uppercase tracking-wider">Cards</span>
          <button
            onclick={() => openNewFile('card')}
            class="text-gray-400 hover:text-gray-600 text-sm leading-none"
            title="New card"
          >+</button>
        </div>
        {#each files.cards as filename}
          <button
            onclick={() => openFile('card', filename)}
            class="w-full text-left px-2 py-1 rounded text-xs transition-colors truncate
              {selectedFile === filename && selectedKind === 'card'
                ? 'bg-blue-100 text-blue-800'
                : 'text-gray-600 hover:bg-gray-100'}"
          >{filename}</button>
        {/each}
      </div>
    </div>

    <!-- Editor area -->
    <div class="flex-1 flex flex-col overflow-hidden bg-[#282c34]">
      <!-- Editor toolbar -->
      <div class="flex items-center gap-2 px-3 py-1.5 bg-[#21252b] border-b border-[#181a1f]">
        {#if isNewFile}
          <input
            bind:value={newFilename}
            placeholder="filename.md"
            class="flex-1 text-xs bg-[#282c34] text-gray-200 border border-gray-600 rounded px-2 py-1 outline-none focus:border-blue-400"
          />
        {:else if selectedFile}
          <span class="text-xs text-gray-400 flex-1 truncate">
            {selectedKind}/{selectedFile}
            {#if isDirty}<span class="text-amber-400 ml-1">●</span>{/if}
          </span>
        {:else}
          <span class="text-xs text-gray-500 flex-1">Select a file to edit</span>
        {/if}

        {#if selectedKind !== null}
          <button
            onclick={saveFile}
            disabled={isSaving}
            class="text-xs px-2 py-1 bg-blue-600 hover:bg-blue-700 text-white rounded disabled:opacity-50"
          >{isSaving ? 'Saving…' : 'Save'}</button>

          {#if !isNewFile && selectedFile}
            <button
              onclick={deleteFile}
              class="text-xs px-2 py-1 text-red-400 hover:text-red-300 hover:bg-red-900/20 rounded"
            >Delete</button>
          {/if}
        {/if}
      </div>

      <!-- CodeMirror mount point -->
      <div
        bind:this={editorEl}
        class="flex-1 overflow-auto text-sm [&_.cm-editor]:h-full [&_.cm-editor]:outline-none [&_.cm-scroller]:overflow-auto"
      ></div>
    </div>
  </div>
</div>

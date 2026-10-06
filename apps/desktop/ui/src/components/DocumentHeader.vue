<script setup lang="ts">
import { ChevronDown, FolderOpen, PanelLeftClose, Redo2, Save, Undo2 } from '@lucide/vue'

import { useEditorSession } from '@/editor/useEditor'

const editor = useEditorSession()
</script>

<template>
  <header class="flex flex-col gap-1 border-b px-3 py-2 hairline">
    <div class="flex items-center gap-1">
      <button
        type="button"
        class="flex min-w-0 flex-1 items-center gap-1 rounded px-1 py-0.5 text-left hover:bg-shell-900"
        :title="editor.view.value?.displayName ?? 'Untitled'"
      >
        <span class="truncate font-medium">{{
          editor.view.value?.displayName ?? 'Untitled'
        }}</span>
        <ChevronDown class="size-3.5 shrink-0 text-ink-500" aria-hidden="true" />
      </button>

      <button
        type="button"
        class="rounded p-1 text-ink-300 hover:bg-shell-900 disabled:text-ink-700"
        :disabled="editor.busy.value || !editor.canOpen.value"
        :title="editor.canOpen.value ? 'Open a document' : 'This build cannot read files'"
        aria-label="Open"
        @click="editor.open()"
      >
        <FolderOpen class="size-3.5" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="rounded p-1 text-ink-300 hover:bg-shell-900 disabled:text-ink-700"
        :disabled="editor.busy.value"
        title="Save"
        aria-label="Save"
        @click="editor.save()"
      >
        <Save class="size-3.5" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="rounded p-1 text-ink-700"
        disabled
        title="Collapsing the panel is not implemented yet"
        aria-label="Collapse panel"
      >
        <PanelLeftClose class="size-3.5" aria-hidden="true" />
      </button>
    </div>

    <div class="flex items-center gap-2 text-[11px]">
      <span class="text-ink-500">Drafts</span>
      <span class="text-ink-100">Preview</span>
      <!-- The dot is the only success signal: a save is otherwise indistinguishable
           from the absence of an edit. -->
      <span
        v-if="editor.dirty.value"
        class="ml-auto size-1.5 rounded-full bg-accent-400"
        title="Unsaved changes"
        aria-label="Unsaved changes"
      />

      <span class="ml-auto flex items-center gap-1">
        <button
          type="button"
          class="rounded p-1 text-ink-300 hover:bg-shell-900 disabled:text-ink-700"
          :disabled="editor.busy.value"
          title="Undo"
          aria-label="Undo"
          @click="editor.undo()"
        >
          <Undo2 class="size-3.5" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="rounded p-1 text-ink-300 hover:bg-shell-900 disabled:text-ink-700"
          :disabled="editor.busy.value"
          title="Redo"
          aria-label="Redo"
          @click="editor.redo()"
        >
          <Redo2 class="size-3.5" aria-hidden="true" />
        </button>
      </span>
    </div>
  </header>
</template>

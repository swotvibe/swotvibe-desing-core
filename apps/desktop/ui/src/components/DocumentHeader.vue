<script setup lang="ts">
import { FilePlus2, FolderOpen, Redo2, Save, Undo2 } from '@lucide/vue'

import { useEditorSession } from '@/editor/useEditor'

const editor = useEditorSession()
</script>

<template>
  <header class="flex flex-col gap-1 border-b px-3 py-3 hairline">
    <div class="flex items-center gap-1">
      <span class="min-w-0 flex-1 truncate font-medium" :title="editor.view.value?.displayName ?? 'Untitled'">{{ editor.view.value?.displayName ?? 'Untitled' }}</span>
      <button v-if="editor.canNew" type="button" aria-label="New document" title="New document" class="rounded p-1.5 hover:bg-shell-800" @click="editor.newDocument()"><FilePlus2 class="size-4" /></button>

      <button
        type="button"
        class="rounded p-1.5 text-ink-300 hover:bg-shell-900 disabled:text-ink-700"
        :disabled="!editor.canOpen.value"
        :title="editor.canOpen.value ? 'Open a document' : 'This build cannot read files'"
        aria-label="Open"
        @click="editor.open()"
      >
        <FolderOpen class="size-3.5" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="rounded p-1.5 text-ink-300 hover:bg-shell-900 disabled:text-ink-700"
        :disabled="!editor.activePageId.value"
        title="Save (Ctrl+S)"
        aria-label="Save"
        @click="editor.save()"
      >
        <Save class="size-3.5" aria-hidden="true" />
      </button>

    </div>

    <div class="flex items-center gap-2 text-[11px]">
      <span class="flex items-center gap-1.5 text-ink-500" role="status">
        <span class="size-1.5 rounded-full" :class="editor.dirty.value ? 'bg-accent-400' : 'bg-shell-600'" aria-hidden="true" />
        <span :aria-label="editor.dirty.value ? 'Unsaved changes' : undefined">{{
          !editor.activePageId.value ? 'No document' : editor.busy.value ? 'Updating…' : editor.dirty.value ? 'Unsaved changes' : 'All changes saved'
        }}</span>
      </span>

      <span class="ml-auto flex items-center gap-1">
        <button
          type="button"
          class="rounded p-1.5 text-ink-300 hover:bg-shell-900 disabled:text-ink-700"
          :disabled="editor.busy.value || !editor.canUndo.value"
          title="Undo (Ctrl+Z)"
          aria-label="Undo"
          @click="editor.undo()"
        >
          <Undo2 class="size-3.5" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="rounded p-1.5 text-ink-300 hover:bg-shell-900 disabled:text-ink-700"
          :disabled="editor.busy.value || !editor.canRedo.value"
          title="Redo (Ctrl+Shift+Z)"
          aria-label="Redo"
          @click="editor.redo()"
        >
          <Redo2 class="size-3.5" aria-hidden="true" />
        </button>
      </span>
    </div>
  </header>
</template>

<script setup lang="ts">
import { Save } from '@lucide/vue'
import { useEditorSession } from '@/editor/useEditor'
const editor = useEditorSession()
</script>
<template>
  <header class="flex items-center justify-between border-b px-3 py-3 hairline">
    <span class="font-medium">Properties</span>
    <button type="button" class="flex items-center gap-2 rounded-md bg-accent-600 px-3 py-1.5 text-white hover:bg-accent-500 disabled:opacity-50"
      :disabled="!editor.activePageId.value" @click="editor.save()" title="Save document (Ctrl+S)">
      <Save class="size-3.5" /> Save
    </button>
  </header>
</template>

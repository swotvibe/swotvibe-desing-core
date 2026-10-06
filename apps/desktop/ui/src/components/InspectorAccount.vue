<script setup lang="ts">
import { Play, Share2 } from '@lucide/vue'

import { useEditorSession } from '@/editor/useEditor'

const editor = useEditorSession()
</script>

<template>
  <header class="flex items-center gap-1 px-3 py-2">
    <span
      class="flex size-6 items-center justify-center rounded-full bg-shell-700 text-[10px] font-semibold"
      title="Account"
    >
      AV
    </span>

    <button
      type="button"
      class="rounded p-1.5 text-ink-300 hover:bg-shell-900"
      title="Present"
      aria-label="Present"
    >
      <Play class="size-3.5" aria-hidden="true" />
    </button>

    <button
      type="button"
      class="ml-auto flex items-center gap-1.5 rounded-md bg-accent-600 px-2.5 py-1 text-[11px] font-medium text-white hover:bg-accent-500 disabled:opacity-60"
      :disabled="editor.busy.value"
      @click="editor.save()"
    >
      <Share2 class="size-3" aria-hidden="true" />
      Share
    </button>
  </header>
</template>

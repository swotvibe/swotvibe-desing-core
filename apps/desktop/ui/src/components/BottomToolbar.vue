<script setup lang="ts">
import {
  Aperture,
  Circle,
  Hand,
  MessageSquare,
  MousePointer2,
  PenTool,
  Square,
  Type,
  ZoomIn,
  ZoomOut,
} from '@lucide/vue'

import { useEditorSession } from '@/editor/useEditor'
import type { ToolId } from '@/editor/context'

/**
 * The floating tool palette.
 *
 * Only `select` is wired to the document service. Unavailable tools are shown
 * disabled rather than omitted: a palette that changes shape as features land
 * is harder to learn than one that is honest about what it cannot do yet.
 */
const editor = useEditorSession()

interface PaletteTool {
  id: ToolId
  label: string
  icon: unknown
  available: boolean
}

const tools: PaletteTool[] = [
  { id: 'move', label: 'Move', icon: MousePointer2, available: true },
  { id: 'select', label: 'Select', icon: Aperture, available: true },
  { id: 'frame', label: 'Frame', icon: Square, available: false },
  { id: 'shape', label: 'Shape', icon: Circle, available: false },
  { id: 'pen', label: 'Pen', icon: PenTool, available: false },
  { id: 'text', label: 'Text', icon: Type, available: false },
  { id: 'comment', label: 'Comment', icon: MessageSquare, available: false },
  { id: 'hand', label: 'Hand', icon: Hand, available: true },
]

function choose(tool: PaletteTool): void {
  if (!tool.available) return
  editor.activeTool.value = tool.id
}
</script>

<template>
  <div
    class="pointer-events-auto absolute bottom-4 left-1/2 flex -translate-x-1/2 items-center gap-0.5 rounded-full border border-white/5 bg-shell-800/95 px-1.5 py-1 shadow-[0_10px_30px_rgba(0,0,0,0.5)] backdrop-blur"
    role="toolbar"
    aria-label="Tools"
  >
    <button
      v-for="tool in tools"
      :key="tool.id"
      type="button"
      class="rounded-full p-2 transition-colors"
      :class="
        editor.activeTool.value === tool.id && tool.available
          ? 'bg-accent-500 text-white'
          : tool.available
            ? 'text-ink-300 hover:bg-shell-700'
            : 'text-ink-700'
      "
      :disabled="!tool.available"
      :aria-pressed="editor.activeTool.value === tool.id"
      :title="tool.available ? tool.label : `${tool.label} is not available yet`"
      :aria-label="tool.label"
      @click="choose(tool)"
    >
      <component :is="tool.icon" class="size-4" aria-hidden="true" />
    </button>

    <span class="mx-1 h-5 w-px bg-white/10" aria-hidden="true" />

    <button
      type="button"
      class="rounded-full p-2 text-ink-300 hover:bg-shell-700"
      title="Zoom out"
      aria-label="Zoom out"
      @click="editor.setZoom(editor.zoom.value / 1.25)"
    >
      <ZoomOut class="size-4" aria-hidden="true" />
    </button>
    <button
      type="button"
      class="rounded-full p-2 text-ink-300 hover:bg-shell-700"
      title="Zoom in"
      aria-label="Zoom in"
      @click="editor.setZoom(editor.zoom.value * 1.25)"
    >
      <ZoomIn class="size-4" aria-hidden="true" />
    </button>
  </div>
</template>

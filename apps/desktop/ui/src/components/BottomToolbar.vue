<script setup lang="ts">
import {
  Circle,
  Hand,
  MousePointer2,
  Square,
  Trash2,
  Type,
  ZoomIn,
  ZoomOut,
} from '@lucide/vue'

import type { ToolId } from '@/editor/context'
import { useEditorSession } from '@/editor/useEditor'

/** Creation is one action; Select drags layers and Hand pans the canvas. */
const editor = useEditorSession()

/** The node kind each creation tool creates, for the tools that create one. */
const CREATES: Partial<Record<ToolId, string>> = {
  frame: 'frame',
  shape: 'shape',
  text: 'text',
}

/** The selection tool also moves layers by dragging. */
const SELECTS = 'select'

interface PaletteTool {
  id: ToolId
  label: string
  icon: unknown
  /** `true` when the palette can act, `false` when nothing is behind it yet. */
  creates: boolean
}

const tools: PaletteTool[] = [
  { id: 'select', label: 'Select', icon: MousePointer2, creates: false },
  { id: 'frame', label: 'Frame', icon: Square, creates: true },
  { id: 'shape', label: 'Shape', icon: Circle, creates: true },
  { id: 'text', label: 'Text', icon: Type, creates: true },
  { id: 'hand', label: 'Hand', icon: Hand, creates: false },
]

/** Whether a tool can be used at all in this build. */
function usable(tool: PaletteTool): boolean {
  if (tool.creates) {
    const kind = CREATES[tool.id]
    return kind !== undefined && editor.canCreate(kind)
  }
  return tool.id === SELECTS || tool.id === 'move' || tool.id === 'hand'
}

/** Why a tool is unavailable, in words a person can act on. */
function reason(tool: PaletteTool): string {
  if (usable(tool)) {
    const kind = CREATES[tool.id]
    return kind ? `Add a ${kind}` : tool.label
  }
  if (tool.creates) {
    return `${tool.label} is not available in this build`
  }
  return `${tool.label} is not available yet`
}

function choose(tool: PaletteTool): void {
  if (!usable(tool)) return
  const kind = CREATES[tool.id]
  if (kind) {
    // A creation tool acts and then hands control back to Select: leaving it
    // armed would make the next click on the canvas ambiguous.
    void editor.createNode(kind).then(() => {
      editor.activeTool.value = 'select'
    })
    return
  }
  editor.activeTool.value = tool.id
}
</script>

<template>
  <div
    class="pointer-events-auto absolute bottom-12 left-1/2 flex -translate-x-1/2 items-center gap-0.5 rounded-full border border-white/5 bg-shell-800/95 px-1.5 py-1 shadow-[0_10px_30px_rgba(0,0,0,0.5)] backdrop-blur"
    role="toolbar"
    aria-label="Tools"
  >
    <button
      v-for="tool in tools"
      :key="tool.id"
      type="button"
      class="rounded-full p-2 transition-colors"
      :class="
        editor.activeTool.value === tool.id && usable(tool)
          ? 'bg-accent-500 text-white'
          : usable(tool)
            ? 'text-ink-300 hover:bg-shell-700'
            : 'text-ink-700'
      "
      :disabled="!usable(tool) || (tool.creates && (editor.busy.value || !editor.activePageId.value))"
      :aria-pressed="editor.activeTool.value === tool.id"
      :title="reason(tool)"
      :aria-label="tool.label"
      @click="choose(tool)"
    >
      <component :is="tool.icon" class="size-4" aria-hidden="true" />
    </button>

    <button
      type="button"
      class="rounded-full p-2 text-ink-300 hover:bg-shell-700 disabled:text-ink-700"
      :disabled="editor.selectedId.value === null || editor.busy.value"
      :title="
        editor.selectedId.value
          ? 'Delete the selected node'
          : 'Select a node to delete'
      "
      aria-label="Delete"
      @click="editor.deleteSelected()"
    >
      <Trash2 class="size-4" aria-hidden="true" />
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

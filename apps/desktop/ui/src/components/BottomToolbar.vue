<script setup lang="ts">
import {
  Aperture,
  Circle,
  Hand,
  MessageSquare,
  MousePointer2,
  PenTool,
  Square,
  Trash2,
  Type,
  ZoomIn,
  ZoomOut,
} from '@lucide/vue'

import type { ToolId } from '@/editor/context'
import { useEditorSession } from '@/editor/useEditor'

/**
 * The floating tool palette.
 *
 * A creation tool is enabled only when the running build can create that kind,
 * which is asked once at start-up rather than discovered by failing. A tool that
 * is genuinely wired but cannot apply anywhere — because no document is open —
 * reports that when used, instead of being silently disabled and leaving the
 * person to guess why.
 */
const editor = useEditorSession()

/** The node kind each creation tool creates, for the tools that create one. */
const CREATES: Partial<Record<ToolId, string>> = {
  frame: 'frame',
  shape: 'shape',
  text: 'text',
}

interface PaletteTool {
  id: ToolId
  label: string
  icon: unknown
  /** `true` when the palette can act, `false` when nothing is behind it yet. */
  creates: boolean
}

const tools: PaletteTool[] = [
  { id: 'move', label: 'Move', icon: MousePointer2, creates: false },
  { id: 'select', label: 'Select', icon: Aperture, creates: false },
  { id: 'frame', label: 'Frame', icon: Square, creates: true },
  { id: 'shape', label: 'Shape', icon: Circle, creates: true },
  { id: 'pen', label: 'Pen', icon: PenTool, creates: false },
  { id: 'text', label: 'Text', icon: Type, creates: true },
  { id: 'comment', label: 'Comment', icon: MessageSquare, creates: false },
  { id: 'hand', label: 'Hand', icon: Hand, creates: false },
]

/** Whether a tool can be used at all in this build. */
function usable(tool: PaletteTool): boolean {
  if (!tool.creates) {
    // Move, select and hand change session state only, so they always work.
    return tool.id === 'move' || tool.id === 'select' || tool.id === 'hand'
  }
  const kind = CREATES[tool.id]
  return kind !== undefined && editor.canCreate(kind)
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
        editor.activeTool.value === tool.id && usable(tool)
          ? 'bg-accent-500 text-white'
          : usable(tool)
            ? 'text-ink-300 hover:bg-shell-700'
            : 'text-ink-700'
      "
      :disabled="!usable(tool)"
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
      :disabled="editor.selectedId.value === null"
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

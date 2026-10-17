<script setup lang="ts">
import { onBeforeUnmount, onMounted } from 'vue'

import BottomToolbar from '@/components/BottomToolbar.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import DocumentHeader from '@/components/DocumentHeader.vue'
import LeftSidebar from '@/components/LeftSidebar.vue'
import RightInspector from '@/components/RightInspector.vue'
import StageCanvas from '@/components/StageCanvas.vue'
import { provideEditorSession, useEditorBridge } from '@/editor/useEditor'

/*
 * The shell owns the session.
 *
 * It is created here, once, and every panel below injects the same one. If a
 * panel created its own, the layer list and the canvas would disagree about what
 * is selected.
 */
const editor = provideEditorSession(useEditorBridge())

/**
 * Keyboard shortcuts for the operations that have one.
 *
 * Bound on the window rather than on a panel so the shortcut works wherever the
 * focus is, except inside a text field: pressing Delete while typing a layer
 * name must delete a character, not the node.
 */
function onKeydown(event: KeyboardEvent): void {
  if (event.defaultPrevented || event.isComposing || event.keyCode === 229 || editor.awaitingConfirmation.value) return
  const target = event.target as HTMLElement | null
  const typing =
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target?.isContentEditable === true
  const key = event.key.toLowerCase()
  if ((event.ctrlKey || event.metaKey) && (key === 's' || key === 'o')) {
    event.preventDefault()
    if (typing) target?.blur()
    if (key === 's') void editor.save()
    else void editor.whenIdle().then(() => editor.open())
    return
  }
  if (typing || event.altKey) return
  if (event.ctrlKey || event.metaKey) {
    if (key === 'z' || key === 'y') {
      event.preventDefault()
      if (key === 'y' || event.shiftKey) { if (editor.canRedo.value) void editor.redo() }
      else if (editor.canUndo.value) void editor.undo()
    }
    return
  }
  if (key === 'v') editor.activeTool.value = 'select'
  if (key === 'h') editor.activeTool.value = 'hand'
  if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) {
    const node = editor.selected.value
    if (node && !node.props.positionIsLayoutDecided) {
      event.preventDefault()
      const step = event.shiftKey ? 10 : 1
      void editor.nudge(event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0,
        event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0)
    }
  }

  if (event.key === 'Delete' || event.key === 'Backspace') {
    if (editor.selectedId.value === null) return
    // Backspace is only a delete here because nothing in the shell is
    // navigable backwards; if that changes, this binding should narrow.
    event.preventDefault()
    void editor.deleteSelected()
    return
  }

  if (event.key === 'Escape') {
    editor.select(null)
    editor.activeTool.value = 'select'
  }
}

function beforeUnload(event: BeforeUnloadEvent): void {
  if (!editor.dirty.value) return
  event.preventDefault()
  event.returnValue = ''
}

onMounted(() => {
  void editor.load()
  window.addEventListener('keydown', onKeydown)
  window.addEventListener('beforeunload', beforeUnload)
})

onBeforeUnmount(() => {
  editor.dispose()
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('beforeunload', beforeUnload)
})

/*
 * The session is exposed so a test can drive the shell's own state — the same
 * object the panels inject — instead of reaching past it or rebuilding it. It
 * changes nothing at run time: a root component has no parent to consume it.
 */
defineExpose({ editor })
</script>

<template>
  <!-- Panels keep readable field widths; only the canvas and panels scroll. -->
  <div class="grid h-dvh grid-cols-[12rem_minmax(0,1fr)_18rem] xl:grid-cols-[13rem_minmax(0,1fr)_19rem]">

    <div class="flex min-h-0 flex-col border-r hairline">
      <DocumentHeader />
      <LeftSidebar class="min-h-0 flex-1" />
    </div>

    <div class="relative min-h-0 min-w-0">
      <StageCanvas />
      <BottomToolbar />
    </div>

    <RightInspector class="border-l hairline" />
  </div>

  <!-- The question sits above everything, including the canvas. -->
  <ConfirmDialog />
</template>

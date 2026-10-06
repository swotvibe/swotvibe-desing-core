<script setup lang="ts">
import { onBeforeUnmount, onMounted } from 'vue'

import BottomToolbar from '@/components/BottomToolbar.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import DocumentHeader from '@/components/DocumentHeader.vue'
import LeftSidebar from '@/components/LeftSidebar.vue'
import RightInspector from '@/components/RightInspector.vue'
import SideRail from '@/components/SideRail.vue'
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
  const target = event.target as HTMLElement | null
  const typing =
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target?.isContentEditable === true
  if (typing) return

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
  }
}

onMounted(() => {
  void editor.load()
  window.addEventListener('keydown', onKeydown)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
})

/*
 * The session is exposed so a test can drive the shell's own state — the same
 * object the panels inject — instead of reaching past it or rebuilding it. It
 * changes nothing at run time: a root component has no parent to consume it.
 */
defineExpose({ editor })
</script>

<template>
  <!--
    Four fixed columns: a tool rail, the document and layer panel, the canvas,
    and the inspector. Only the canvas and the two scrollable panels scroll, so
    the workspace does not move when a document changes size.
  -->
  <div class="grid h-screen grid-cols-[3.25rem_15rem_1fr_17.5rem]">
    <SideRail />

    <div class="flex min-h-0 flex-col border-r hairline">
      <DocumentHeader />
      <LeftSidebar class="min-h-0 flex-1" />
    </div>

    <div class="relative min-h-0">
      <StageCanvas />
      <BottomToolbar />
    </div>

    <RightInspector class="border-l hairline" />
  </div>

  <!-- The question sits above everything, including the canvas. -->
  <ConfirmDialog />
</template>

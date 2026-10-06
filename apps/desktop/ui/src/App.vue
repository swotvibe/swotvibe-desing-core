<script setup lang="ts">
import { onMounted } from 'vue'

import BottomToolbar from '@/components/BottomToolbar.vue'
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

onMounted(() => {
  void editor.load()
})
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
</template>

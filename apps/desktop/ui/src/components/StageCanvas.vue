<script setup lang="ts">
import { Code2 } from '@lucide/vue'
import { computed, ref } from 'vue'

import { toCss } from '@/editor/color'
import { ARTBOARD, useEditorSession } from '@/editor/useEditor'

/**
 * The canvas.
 *
 * Nodes are drawn as positioned boxes rather than from a rasterized preview:
 * the shell needs a selectable element per node, and this build has no
 * rasterized payload. Each box uses the geometry the layout pass resolved, not
 * a second opinion about where a node is — that is why the bridge exposes
 * `layout` instead of leaving the interface to guess.
 */
const editor = useEditorSession()

const stage = ref<HTMLElement | null>(null)

const scale = computed(() => editor.zoom.value)
const canvasWidth = computed(() => ARTBOARD[0] * scale.value)
const canvasHeight = computed(() => ARTBOARD[1] * scale.value)
const zoomLabel = computed(() => `${Math.round(scale.value * 100)}%`)

/** Every node with the geometry the layout pass resolved for it. */
const boxes = computed(() =>
  (editor.layout.value?.nodes ?? []).map((node) => ({
    geometry: node,
    node: editor.nodesById.value.get(node.id),
  })),
)

function onStageClick(event: MouseEvent): void {
  const element = stage.value
  if (!element) return
  const bounds = element.getBoundingClientRect()
  // Screen units to page units. The canvas is the only place that may do this
  // conversion, because it is the only place that knows the zoom.
  const pageX = (event.clientX - bounds.left) / scale.value
  const pageY = (event.clientY - bounds.top) / scale.value
  void editor.selectAt(pageX, pageY)
}

function fillOf(id: string): string {
  return toCss(editor.nodesById.value.get(id)?.props.fill ?? null)
}

function radiusOf(id: string): string {
  const props = editor.nodesById.value.get(id)?.props
  if (!props) return '0px'
  if (props.shapeGeometry === 'ellipse') return '9999px'
  return `${(props.cornerRadius ?? 0) * scale.value}px`
}
</script>

<template>
  <section class="flex min-h-0 flex-col bg-shell-950">
    <header class="flex items-center gap-2 border-b px-3 py-2 hairline">
      <span class="text-[11px] text-ink-500">
        {{ editor.view.value?.pages[0]?.name ?? 'No page' }}
      </span>
      <span class="ml-auto flex items-center gap-2 text-[11px] text-ink-500">
        <span>{{ zoomLabel }}</span>
        <Code2 class="size-3.5" aria-hidden="true" />
      </span>
    </header>

    <div class="canvas-checker flex min-h-0 flex-1 items-start justify-center overflow-auto p-8">
      <!--
        The stage is the artboard. Its size comes from the layout result, so a
        document whose content overflows its requested page grows here too.
      -->
      <div
        ref="stage"
        data-test="stage"
        class="relative shrink-0 bg-white shadow-[0_8px_40px_rgba(0,0,0,0.45)]"
        :style="{ width: `${canvasWidth}px`, height: `${canvasHeight}px` }"
        @click="onStageClick"
      >
        <div
          v-for="box in boxes"
          :key="box.geometry.id"
          class="absolute"
          :style="{
            left: `${box.geometry.rect[0] * scale}px`,
            top: `${box.geometry.rect[1] * scale}px`,
            width: `${box.geometry.rect[2] * scale}px`,
            height: `${box.geometry.rect[3] * scale}px`,
            background: fillOf(box.geometry.id),
            borderRadius: radiusOf(box.geometry.id),
          }"
        >
          <!-- A text node draws its content; shapes need no child element. -->
          <span
            v-if="box.node?.props.textContent"
            class="block overflow-hidden text-ellipsis whitespace-nowrap"
            :style="{
              fontSize: `${(box.node.props.fontSize ?? 16) * scale}px`,
              lineHeight: `${box.geometry.rect[3] * scale}px`,
              color: toCss(box.node.props.fill),
              direction: box.node.props.textDirection === 'rtl' ? 'rtl' : 'ltr',
            }"
          >
            {{ box.node.props.textContent }}
          </span>
        </div>

        <!--
          The selection outline is drawn from the layout rectangle, so it tracks
          a transformed node rather than an approximation of one.
        -->
        <div
          v-if="editor.selectedRect.value"
          class="pointer-events-none absolute border border-accent-400"
          :style="{
            left: `${editor.selectedRect.value[0] * scale - 1}px`,
            top: `${editor.selectedRect.value[1] * scale - 1}px`,
            width: `${editor.selectedRect.value[2] * scale + 2}px`,
            height: `${editor.selectedRect.value[3] * scale + 2}px`,
          }"
          aria-hidden="true"
        />
      </div>
    </div>

    <footer class="flex items-center gap-3 border-t px-3 py-1.5 text-[11px] text-ink-500 hairline">
      <span>{{ editor.status.value }}</span>
      <span v-if="editor.failure.value" class="truncate text-red-400">
        {{ editor.failure.value.code }}: {{ editor.failure.value.message }}
      </span>
      <span class="ml-auto">rev {{ editor.revision.value }}</span>
    </footer>
  </section>
</template>

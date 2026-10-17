<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { toCss } from '@/editor/color'
import { ARTBOARD, useEditorSession } from '@/editor/useEditor'

const editor = useEditorSession()
const stage = ref<HTMLElement | null>(null)
const viewport = ref<HTMLElement | null>(null)
const scale = computed(() => editor.zoom.value)
const zoomLabel = computed(() => `${Math.round(scale.value * 100)}%`)
const pageSize = computed(() => editor.layout.value?.pageSize ?? ARTBOARD)
const canvasWidth = computed(() => pageSize.value[0] * scale.value)
const canvasHeight = computed(() => pageSize.value[1] * scale.value)
const delta = ref<[number, number]>([0, 0])
const panOffset = ref<[number, number]>([0, 0])
const resizeSize = ref<[number, number] | null>(null)
interface Gesture {
  pointer: number
  x: number
  y: number
  lastX: number
  lastY: number
  scrollX: number
  scrollY: number
  scale: number
  mode: 'pan' | 'move' | 'resize'
  node: string | null
  position: [number, number]
  size: [number, number]
  moved: boolean
  blocked: boolean
  ending: boolean
  ready: Promise<void>
}
let gesture: Gesture | null = null
const interacting = ref(false)
let committing = false
let suppressClick = false

const movingIds = computed(() => {
  const ids = new Set<string>()
  const add = (id: string) => {
    if (ids.has(id)) return
    ids.add(id)
    editor.nodesById.value.get(id)?.children.forEach(add)
  }
  if (editor.selectedId.value) add(editor.selectedId.value)
  return ids
})

const painted = computed(() => (editor.layout.value?.nodes ?? []).map((geometry) => {
  const node = editor.nodesById.value.get(geometry.id)
  const props = node?.props
  const isText = node?.kind === 'text'
  const [a, b, c, d, e, f] = geometry.world
  const moving = movingIds.value.has(geometry.id)
  const size = geometry.id === editor.selectedId.value && resizeSize.value ? resizeSize.value : geometry.size
  const dx = moving ? delta.value[0] : 0
  const dy = moving ? delta.value[1] : 0
  return {
    id: geometry.id,
    box: {
      left: '0px', top: '0px',
      width: `${size[0] * scale.value}px`, height: `${size[1] * scale.value}px`,
      transformOrigin: '0 0', transform: `matrix(${a}, ${b}, ${c}, ${d}, ${(e + dx) * scale.value}, ${(f + dy) * scale.value})`,
      ...(isText ? {} : { background: toCss(props?.fill ?? null), borderRadius: props?.shapeGeometry === 'ellipse' ? '50%' : `${(props?.cornerRadius ?? 0) * scale.value}px` }),
      ...(props?.stroke ? { outline: `${props.stroke.width * scale.value}px solid ${toCss(props.stroke.color)}` } : {}),
    },
    content: props?.textContent ?? '',
    direction: props?.textDirection === 'rtl' ? 'rtl' : props?.textDirection === 'ltr' ? 'ltr' : 'auto',
    text: isText ? { color: toCss(props?.fill ?? null), fontSize: `${(props?.fontSize ?? 16) * scale.value}px`,
      fontFamily: props?.fontFamily ? `"${props.fontFamily}", sans-serif` : 'sans-serif',
      fontWeight: props?.fontWeight ?? 400, textAlign: (props?.textAlign ?? 'start') as 'start' | 'center' | 'end', lineHeight: '1.25' } : null,
  }
}))

function coordinates(event: MouseEvent): [number, number] {
  const bounds = stage.value!.getBoundingClientRect()
  return [(event.clientX - bounds.left) / scale.value, (event.clientY - bounds.top) / scale.value]
}
function onStageClick(event: MouseEvent): void {
  if (suppressClick) { suppressClick = false; return }
  if (editor.activeTool.value === 'hand' || editor.busy.value || !stage.value) return
  void editor.selectAt(...coordinates(event))
}
function pointerDown(event: PointerEvent): void {
  if ((event.button !== 0 && event.button !== 1) || gesture || !viewport.value || editor.awaitingConfirmation.value) return
  const pan = editor.activeTool.value === 'hand' || event.button === 1
  if (!pan && (!stage.value || !stage.value.contains(event.target as Node))) return
  event.preventDefault()
  // Pointer capture prevents native blur. Commit an edited property before selecting another layer.
  const focused = document.activeElement
  if (focused instanceof HTMLInputElement || focused instanceof HTMLTextAreaElement) focused.blur()
  viewport.value.setPointerCapture?.(event.pointerId)
  const point = stage.value ? coordinates(event) : [0, 0] as [number, number]
  const current: Gesture = {
    pointer: event.pointerId, x: event.clientX, y: event.clientY, lastX: event.clientX, lastY: event.clientY,
    scrollX: panOffset.value[0], scrollY: panOffset.value[1], scale: scale.value,
    mode: pan ? 'pan' : (event.target as Element).closest('[data-resize]') ? 'resize' : 'move',
    node: null, position: [0, 0], size: [0, 0], moved: false, blocked: false, ending: false, ready: Promise.resolve(),
  }
  gesture = current
  interacting.value = true
  suppressClick = true
  current.ready = prepareGesture(current, point)
}
async function prepareGesture(current: Gesture, point: [number, number]): Promise<void> {
  if (current.mode === 'pan') return
  await editor.whenIdle()
  if (gesture !== current || editor.awaitingConfirmation.value) return
  if (current.mode !== 'resize' && !await editor.selectAt(...point)) return
  if (gesture !== current) return
  const selected = editor.selected.value
  if (!selected) return
  if (current.mode === 'resize') {
    current.node = selected.id
    current.size = editor.layout.value?.nodes.find((node) => node.id === selected.id)?.size ?? selected.props.size
  } else if (selected.props.positionIsLayoutDecided) {
    current.blocked = true
    if (current.moved) editor.status.value = 'This layer is positioned by its parent. Move the parent or change its dimensions.'
  } else {
    current.node = selected.id
    current.position = [selected.props.transform[4], selected.props.transform[5]]
  }
  updateGesture(current)
}
function updateGesture(current: Gesture): void {
  const dx = current.lastX - current.x
  const dy = current.lastY - current.y
  if (Math.hypot(dx, dy) < 3 && !current.moved) return
  current.moved = true
  if (current.mode === 'pan') {
    panOffset.value = [current.scrollX + dx, current.scrollY + dy]
  } else if (current.blocked) {
    editor.status.value = 'This layer is positioned by its parent. Move the parent or change its dimensions.'
  } else if (current.node && current.mode === 'resize') {
    const world = editor.layout.value?.nodes.find((node) => node.id === current.node)?.world
    if (!world) return
    const [a, b, c, d] = world
    const det = a * d - b * c
    if (Math.abs(det) < 1e-10) return
    resizeSize.value = [Math.max(1, current.size[0] + (d * dx - c * dy) / det / current.scale),
      Math.max(1, current.size[1] + (-b * dx + a * dy) / det / current.scale)]
  } else if (current.node) delta.value = [dx / current.scale, dy / current.scale]
}
function pointerMove(event: PointerEvent): void {
  if (!gesture || gesture.pointer !== event.pointerId || gesture.ending) return
  gesture.lastX = event.clientX
  gesture.lastY = event.clientY
  updateGesture(gesture)
}
async function pointerUp(event: PointerEvent): Promise<void> {
  const current = gesture
  if (!current || current.pointer !== event.pointerId || current.ending) return
  current.ending = true
  current.lastX = event.clientX
  current.lastY = event.clientY
  await current.ready
  if (gesture !== current) return
  updateGesture(current)
  gesture = null
  interacting.value = false
  if (viewport.value?.hasPointerCapture?.(event.pointerId)) viewport.value.releasePointerCapture(event.pointerId)
  committing = true
  try {
    if (current.mode === 'resize' && current.node && resizeSize.value
      && (resizeSize.value[0] !== current.size[0] || resizeSize.value[1] !== current.size[1])) {
      await editor.edit([{ kind: 'set-node-size', node: current.node, size: [...resizeSize.value] }])
    } else if (current.mode === 'move' && current.node && current.moved) {
      const parent = editor.nodesById.value.get(current.node)?.parent
      const world = parent ? editor.layout.value?.nodes.find((node) => node.id === parent)?.world : null
      let [dx, dy] = delta.value
      if (world) {
        const [a, b, c, d] = world
        const det = a * d - b * c
        if (Math.abs(det) < 1e-10) return
        ;[dx, dy] = [(d * dx - c * dy) / det, (-b * dx + a * dy) / det]
      }
      if (dx !== 0 || dy !== 0) await editor.edit([{ kind: 'set-node-position', node: current.node,
        position: [current.position[0] + dx, current.position[1] + dy] }])
    }
  } finally {
    committing = false
    delta.value = [0, 0]
    resizeSize.value = null
  }
}
function cancelPointer(): void {
  const current = gesture
  if (!current) return
  gesture = null
  interacting.value = false
  if (current.mode === 'pan') panOffset.value = [current.scrollX, current.scrollY]
  if (viewport.value?.hasPointerCapture?.(current.pointer)) viewport.value.releasePointerCapture(current.pointer)
  editor.select(editor.selectedId.value)
  delta.value = [0, 0]
  resizeSize.value = null
}
function lostCapture(): void { if (gesture && !gesture.ending) cancelPointer() }
function cancelKey(event: KeyboardEvent): void {
  if (event.isComposing || event.keyCode === 229) return
  if (gesture && event.key === 'Escape') {
    event.preventDefault()
    event.stopImmediatePropagation()
    cancelPointer()
  } else if (gesture && (['Delete', 'Backspace', 'ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'h', 'v'].includes(event.key) || event.ctrlKey || event.metaKey)) {
    cancelPointer()
  }
}
// Remove the temporary offset in the same update that installs committed geometry.
watch(() => editor.layout.value, () => {
  if (committing) { delta.value = [0, 0]; resizeSize.value = null }
}, { flush: 'sync' })
watch(() => editor.activePageId.value, async () => {
  cancelPointer()
  panOffset.value = [0, 0]
  await nextTick()
  fit()
})
onMounted(() => {
  window.addEventListener('keydown', cancelKey, true)
  window.addEventListener('blur', cancelPointer)
})
onBeforeUnmount(() => {
  cancelPointer()
  window.removeEventListener('keydown', cancelKey, true)
  window.removeEventListener('blur', cancelPointer)
})
function fit(): void {
  if (!viewport.value || gesture) return
  panOffset.value = [0, 0]
  viewport.value.scrollLeft = 0
  viewport.value.scrollTop = 0
  const width = viewport.value.clientWidth - 96
  const height = viewport.value.clientHeight - 120
  if (width > 0 && height > 0) {
    const next = Math.min(width / pageSize.value[0], height / pageSize.value[1], 2)
    const previous = scale.value
    editor.setZoom(next)
    skipZoomAnchor = scale.value !== previous
  }
}
// Preserve the page point under the cursor, including centering and scroll offsets.
let skipZoomAnchor = false
let zoomAnchor: [number, number] | null = null
let zoomRequest = 0
watch(scale, async (next, previous) => {
  if (skipZoomAnchor) { skipZoomAnchor = false; return }
  if (!stage.value || !viewport.value) return
  const request = ++zoomRequest
  const bounds = stage.value.getBoundingClientRect()
  const viewportBounds = viewport.value.getBoundingClientRect()
  const anchor = zoomAnchor ?? [viewportBounds.left + viewport.value.clientWidth / 2, viewportBounds.top + viewport.value.clientHeight / 2]
  zoomAnchor = null
  const pageX = (anchor[0]! - bounds.left) / previous
  const pageY = (anchor[1]! - bounds.top) / previous
  await nextTick()
  if (request !== zoomRequest || !stage.value) return
  const after = stage.value.getBoundingClientRect()
  panOffset.value = [panOffset.value[0] + anchor[0]! - (after.left + pageX * next),
    panOffset.value[1] + anchor[1]! - (after.top + pageY * next)]
}, { flush: 'pre' })
function wheel(event: WheelEvent): void {
  if (event.ctrlKey || event.metaKey) {
    event.preventDefault()
    if (gesture) return
    zoomAnchor = [event.clientX, event.clientY]
    const previous = scale.value
    editor.setZoom(scale.value * Math.exp(-event.deltaY * 0.002))
    if (scale.value === previous) zoomAnchor = null
  }
}
const selectionBox = computed(() => {
  const geometry = editor.layout.value?.nodes.find((node) => node.id === editor.selectedId.value)
  if (!geometry) return null
  const [w, h] = resizeSize.value ?? geometry.size
  const [a, b, c, d, e, f] = geometry.world
  const xs = [e, a * w + e, c * h + e, a * w + c * h + e]
  const ys = [f, b * w + f, d * h + f, b * w + d * h + f]
  return { left: (Math.min(...xs) + delta.value[0]) * scale.value,
    top: (Math.min(...ys) + delta.value[1]) * scale.value,
    width: (Math.max(...xs) - Math.min(...xs)) * scale.value,
    height: (Math.max(...ys) - Math.min(...ys)) * scale.value,
    handleX: (a * w + c * h + e + delta.value[0]) * scale.value,
    handleY: (b * w + d * h + f + delta.value[1]) * scale.value }
})
</script>

<template>
  <section class="flex h-full min-h-0 min-w-0 flex-col bg-shell-950">
    <header class="flex items-center gap-2 border-b px-3 py-2 hairline">
      <span class="text-[11px] text-ink-500">
        {{ editor.view.value?.pages.find((page) => page.id === editor.activePageId.value)?.name ?? 'No page' }}
      </span>
      <span class="ml-auto flex items-center gap-2 text-[11px] text-ink-500">
        <span>{{ zoomLabel }}</span>
        <button type="button" class="rounded px-2 py-1 hover:bg-shell-800" aria-label="Actual size" @click="editor.setZoom(1)">100%</button>
        <button type="button" class="rounded px-2 py-1 hover:bg-shell-800" aria-label="Fit page" @click="fit">Fit page</button>
      </span>
    </header>

    <div ref="viewport" data-test="viewport" class="canvas-checker relative min-h-0 min-w-0 flex-1 overflow-auto p-12 pb-28"
      :class="interacting ? 'cursor-grabbing' : editor.activeTool.value === 'hand' ? 'cursor-grab' : 'cursor-default'"
      style="touch-action: none"
      @pointerdown="pointerDown" @pointermove="pointerMove" @pointerup="pointerUp" @pointercancel="cancelPointer" @lostpointercapture="lostCapture" @wheel="wheel">
      <!--
        An empty document has nothing to click, so the canvas says what to do
        instead of showing a blank page that looks like a failure.
      -->
      <div
        v-if="!editor.activePageId.value"
        class="flex max-w-sm flex-col items-center gap-3 self-center text-center"
        data-test="empty-state"
      >
        <p class="text-[13px] text-ink-300">No document is open.</p>
        <p class="text-[11px] text-ink-500">
          Create a document, open a file, or explore the sample.
        </p>
        <div class="flex flex-wrap items-center gap-2">
          <button v-if="editor.canNew" type="button" class="rounded-md bg-accent-600 px-3 py-1.5 text-white" :disabled="editor.busy.value" @click="editor.newDocument()">New document</button>
          <button
            type="button"
            class="rounded-md bg-accent-600 px-3 py-1.5 text-[12px] font-medium text-white hover:bg-accent-500 disabled:opacity-50"
            :disabled="!editor.canOpen.value || editor.busy.value"
            :title="editor.canOpen.value ? 'Open a document' : 'This build cannot read files'"
            @click="editor.open()"
          >
            Open a file
          </button>
          <button
            v-if="editor.canOpenSample.value"
            type="button"
            class="rounded-md border px-3 py-1.5 text-[12px] text-ink-100 hover:bg-shell-850 disabled:opacity-50 hairline"
            :disabled="editor.busy.value"
            @click="editor.openSample()"
          >
            Open the sample
          </button>
        </div>
      </div>

      <!--
        The stage is the artboard. Its size comes from the layout result, so a
        document whose content overflows its requested page grows here too.
      -->
      <div
        v-else
        ref="stage"
        data-test="stage"
        class="relative mx-auto shrink-0 select-none bg-white shadow-[0_8px_40px_rgba(0,0,0,0.45)]"
        :style="{ width: `${canvasWidth}px`, height: `${canvasHeight}px`, transform: `translate(${panOffset[0]}px, ${panOffset[1]}px)` }"
        @click="onStageClick"
      >
        <img v-if="editor.previewUrl.value && !resizeSize && delta[0] === 0 && delta[1] === 0"
          :src="editor.previewUrl.value" alt="" class="pointer-events-none absolute inset-0 h-full w-full" draggable="false" />
        <div
          v-for="box in painted"
          v-show="!editor.previewUrl.value || resizeSize || delta[0] !== 0 || delta[1] !== 0"
          :key="box.id"
          class="absolute"
          :style="box.box"
          :data-node-id="box.id"
        >
          <!--
            A text node draws its content. The line box is clipped at the
            layout's own box rather than given an ellipsis: the schema has no
            ellipsis property, and inventing one would claim a typographic
            behaviour the document never asked for. Where the browser's metrics
            differ from the text engine that resolved the box, the disagreement
            is a known limit of drawing text in DOM instead of from the
            renderer's own raster.
          -->
          <span
            v-if="box.text"
            class="block h-full overflow-hidden whitespace-pre-wrap break-words"
            :dir="box.direction"
            :style="box.text"
          >
            {{ box.content }}
          </span>
        </div>

        <!--
          The selection outline is drawn from the layout rectangle, so it tracks
          a transformed node rather than an approximation of one.
        -->
        <div
          v-if="selectionBox"
          class="pointer-events-none absolute border border-accent-400"
          :style="{
            left: `${selectionBox.left - 1}px`,
            top: `${selectionBox.top - 1}px`,
            width: `${selectionBox.width + 2}px`,
            height: `${selectionBox.height + 2}px`,
          }"
          aria-hidden="true"
        />
        <button v-if="selectionBox && editor.activeTool.value !== 'hand'" type="button" data-resize
          aria-label="Resize selected layer" title="Drag to resize; use W and H in Properties for exact dimensions"
          class="absolute size-3 cursor-nwse-resize border border-accent-500 bg-white" :disabled="editor.busy.value"
          :style="{ left: `${selectionBox.handleX - 6}px`, top: `${selectionBox.handleY - 6}px` }" @click.stop />
      </div>
    </div>

    <footer class="flex items-center gap-3 border-t px-3 py-1.5 text-[11px] text-ink-500 hairline">
      <span v-if="!editor.failure.value" role="status" class="min-w-0 flex-1 truncate">{{ editor.busy.value ? 'Working…' : editor.status.value }}</span>
      <span v-if="editor.failure.value" role="alert" class="min-w-0 flex-1 break-words text-red-400">
        {{ editor.failure.value.message }}
      </span>
      <button v-if="editor.failure.value" type="button" class="shrink-0 rounded px-2 py-1 hover:bg-shell-800" aria-label="Dismiss error" @click="editor.clearFailure()">Dismiss</button>
      <span v-else class="ml-auto shrink-0 tabular-nums">{{ Math.round(pageSize[0]) }} × {{ Math.round(pageSize[1]) }}</span>
    </footer>
  </section>
</template>

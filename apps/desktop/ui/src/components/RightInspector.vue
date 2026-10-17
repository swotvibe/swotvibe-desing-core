<script setup lang="ts">
import { Grid2x2, Minus, RotateCw, Spline } from '@lucide/vue'
import { computed, ref } from 'vue'

import { alphaChannel, alphaPercent, fromHex, toHex } from '@/editor/color'
import { useEditorSession } from '@/editor/useEditor'
import InspectorAccount from '@/components/InspectorAccount.vue'
import InspectorSection from '@/components/InspectorSection.vue'
import NumberField from '@/components/NumberField.vue'
import PropertyRow from '@/components/PropertyRow.vue'
import type { Rgba } from '@/bridge/types'

/**
 * The inspector.
 *
 * Every control is bound to the selected node's **stored** properties, never to
 * a local copy: what is shown is what the document holds, and a control that
 * cannot be applied is disabled rather than silently ignored.
 *
 * Sections whose data the current schema does not carry (constraints, effects)
 * are not rendered at all. A control that writes nothing is worse than a missing
 * one, because it looks like a feature.
 */
const editor = useEditorSession()

const open = ref({
  position: true,
  layout: true,
  appearance: true,
  fill: true,
  text: true,
})

const selected = editor.selected
const props = computed(() => selected.value?.props ?? null)

/** Nothing is editable without a selection, so the panel says so once. */
const hasSelection = computed(() => selected.value !== null)

const fillHex = computed(() => (props.value?.fill ? toHex(props.value.fill) : '#000000'))
const fillAlpha = computed(() => (props.value?.fill ? alphaPercent(props.value.fill) : 100))
const hasFill = computed(() => props.value?.fill != null)

const kindLabel = computed(() => selected.value?.kind ?? '—')

/** Editable translation in parent coordinates. */
const position = computed<[number, number] | null>(() => {
  if (!props.value) return null
  // Editable translation is relative to the parent; world bounds cannot be written back here.
  return [props.value.transform[4], props.value.transform[5]]
})
const resolvedSize = computed(() => {
  const geometry = editor.layout.value?.nodes.find((node) => node.id === selected.value?.id)
  return geometry?.size ?? props.value?.size ?? [0, 0]
})

/** Whether the parent's layout places this node, so position is not editable. */
const positionLocked = computed(() => props.value?.positionIsLayoutDecided ?? false)

/** Whether either axis fits its content, so the stored size is not what is used. */
const hugs = computed(
  () => props.value?.widthSizing === 'hug' || props.value?.heightSizing === 'hug',
)

/** Corner radius belongs to a shape, and the service refuses it elsewhere. */
const isShape = computed(() => props.value?.shapeGeometry === 'rect')

// Consistent field behavior: Enter commits one-line fields; Escape restores.
// Multi-line text keeps Enter for line breaks and commits with Ctrl/Cmd+Enter.
function fieldKey(event: KeyboardEvent, original: string): void {
  if (event.isComposing || event.keyCode === 229) return
  const input = event.target as HTMLInputElement | HTMLTextAreaElement
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    input.value = original
    input.blur()
  } else if (event.key === 'Enter' && (input instanceof HTMLInputElement || event.ctrlKey || event.metaKey)) {
    event.preventDefault()
    input.dispatchEvent(new Event('change', { bubbles: true }))
    input.blur()
  }
}
function onName(event: Event): void {
  const input = event.target as HTMLInputElement
  const name = input.value
  input.value = selected.value?.name ?? ''
  if (name !== input.value) void editor.renameSelected(name)
}
function onText(event: Event): void {
  const input = event.target as HTMLTextAreaElement
  const content = input.value
  input.value = props.value?.textContent ?? ''
  if (content !== input.value) void editor.setText(content, props.value?.fontSize ?? 16)
}
function onFillHex(event: Event): void {
  const input = event.target as HTMLInputElement
  const parsed = fromHex(input.value, props.value?.fill?.a ?? 255)
  input.value = fillHex.value
  if (!parsed || toHex(parsed) === fillHex.value) return
  void editor.setFill(parsed)
}

function onFillAlpha(event: Event): void {
  const input = event.target as HTMLInputElement
  const percent = input.value.trim() === '' ? NaN : Number(input.value)
  const current = props.value?.fill
  input.value = String(fillAlpha.value)
  if (!current || !Number.isFinite(percent) || percent === fillAlpha.value) return
  void editor.setFill({
    ...current,
    a: alphaChannel(percent),
  } satisfies Rgba)
}

function toggle(name: keyof typeof open.value): void {
  open.value = { ...open.value, [name]: !open.value[name] }
}
</script>

<template>
  <aside class="flex min-h-0 flex-col bg-shell-950">
    <InspectorAccount />

    <div class="min-h-0 flex-1 overflow-y-auto">
      <!-- The selected node's identity, so the panel is never ambiguous. -->
      <div class="flex items-center gap-2 border-b px-3 py-2 hairline">
        <input
          :value="selected?.name ?? ''"
          type="text"
          class="min-w-0 flex-1 rounded border border-transparent bg-transparent px-1 py-0.5 text-[12px] font-medium text-ink-100 placeholder:text-ink-700 hover:border-white/10 focus:border-accent-500 focus:outline-none"
          placeholder="Unnamed"
          aria-label="Node name"
          :disabled="!hasSelection || editor.busy.value"
          @change="onName"
          @keydown="fieldKey($event, selected?.name ?? '')"
        />
        <span class="rounded bg-shell-850 px-1.5 py-0.5 text-[10px] text-ink-500">
          {{ kindLabel }}
        </span>
      </div>

      <p v-if="!hasSelection" class="px-3 py-4 text-[11px] text-ink-500">
        Select a layer or click the canvas to edit its properties.
      </p>

      <template v-else>
        <InspectorSection
          title="Position"
          :open="open.position"
          @toggle="toggle('position')"
        >
          <template #actions>
            <span class="flex items-center gap-0.5 text-ink-500">
              <component :is="Grid2x2" class="size-3.5" aria-hidden="true" />
            </span>
          </template>

          <PropertyRow label="Position">
            <div class="flex flex-1 items-center gap-1">
              <NumberField
                label="X"
                :model-value="position ? position[0] : ''"
                :disabled="positionLocked || editor.busy.value"
                @commit="(value) => editor.setPosition(value, position?.[1] ?? 0)"
              />
              <NumberField
                label="Y"
                :model-value="position ? position[1] : ''"
                :disabled="positionLocked || editor.busy.value"
                @commit="(value) => editor.setPosition(position?.[0] ?? 0, value)"
              />
            </div>
          </PropertyRow>

          <p v-if="positionLocked" class="px-3 pb-1 text-[10px] text-ink-700">
            The position comes from layout. Move the parent to reposition this layer.
          </p>

          <PropertyRow label="Rotation">
            <span
              class="flex flex-1 items-center gap-1 rounded border bg-shell-900 px-2 py-1 hairline"
            >
              <RotateCw class="size-3 text-ink-700" aria-hidden="true" />
              <span class="tabular-nums">{{ editor.selectedRotation.value }}°</span>
            </span>
          </PropertyRow>
        </InspectorSection>

        <InspectorSection title="Layout" :open="open.layout" @toggle="toggle('layout')">
          <PropertyRow label="Layout">
            <span
              class="flex flex-1 items-center gap-1 rounded border bg-shell-900 px-2 py-1 hairline"
            >
              <Spline class="size-3 text-ink-700" aria-hidden="true" />
              <span>{{ props?.frameLayout ?? 'None' }}</span>
            </span>
          </PropertyRow>

          <PropertyRow label="W / H">
            <div class="flex flex-1 items-center gap-1">
              <NumberField
                label="W"
                :min="0"
                :disabled="editor.busy.value"
                :model-value="resolvedSize[0] ?? 0"
                @commit="(value) => editor.setSize(value, resolvedSize[1] ?? 0)"
              />
              <NumberField
                label="H"
                :min="0"
                :disabled="editor.busy.value"
                :model-value="resolvedSize[1] ?? 0"
                @commit="(value) => editor.setSize(resolvedSize[0] ?? 0, value)"
              />
            </div>
          </PropertyRow>

          <p v-if="hugs" class="px-3 pb-1 text-[10px] text-ink-700">
            This layer fits its content. Editing dimensions switches both axes to fixed size.
          </p>

          <PropertyRow label="Sizing">
            <span
              class="flex flex-1 items-center gap-1 rounded border bg-shell-900 px-2 py-1 text-[11px] hairline"
            >
              <span>{{ props?.widthSizing }}</span>
              <Minus class="size-3 text-ink-700" aria-hidden="true" />
              <span>{{ props?.heightSizing }}</span>
            </span>
          </PropertyRow>

        </InspectorSection>

        <InspectorSection
          v-if="isShape"
          title="Appearance"
          :open="open.appearance"
          @toggle="toggle('appearance')"
        >

          <PropertyRow label="Corner radius">
            <NumberField
              label="R"
              :model-value="props?.cornerRadius ?? ''"
              :disabled="!isShape || editor.busy.value"
              :min="0"
              @commit="(value) => editor.setCornerRadius(value)"
            />
          </PropertyRow>

        </InspectorSection>

        <InspectorSection v-if="selected?.kind === 'text'" title="Text" :open="open.text" @toggle="toggle('text')">
          <div class="space-y-2 px-3 py-2">
            <textarea :key="selected.id" :value="props?.textContent ?? ''" aria-label="Text content" rows="3"
              title="Ctrl+Enter to apply; Escape to cancel"
              class="w-full resize-y rounded border bg-shell-900 p-2 hairline focus:outline-accent-500" :disabled="editor.busy.value"
              @change="onText" @keydown="fieldKey($event, props?.textContent ?? '')" />
            <NumberField label="Font size" :model-value="props?.fontSize ?? 16" :min="1" :disabled="editor.busy.value"
              @commit="editor.setText(props?.textContent ?? '', $event)" />
          </div>
        </InspectorSection>

        <InspectorSection title="Fill" :open="open.fill" @toggle="toggle('fill')">
          <div class="flex items-center gap-2 px-3 py-1">
            <input
              :value="fillHex"
              type="color"
              class="size-5 shrink-0 cursor-pointer rounded border-0 bg-transparent p-0"
              aria-label="Fill colour"
              :disabled="!hasFill || editor.busy.value"
              @change="onFillHex"
            />
            <input
              :value="fillHex"
              type="text"
              class="min-w-0 flex-1 rounded border bg-shell-900 px-2 py-1 font-mono text-[11px] uppercase hairline focus:border-accent-500 focus:outline-none"
              aria-label="Fill hex value"
              @keydown="fieldKey($event, fillHex)"
              :disabled="!hasFill || editor.busy.value"
              @change="onFillHex"
            />
            <input
              :value="fillAlpha"
              type="number"
              min="0"
              max="100"
              class="w-12 rounded border bg-shell-900 px-1.5 py-1 text-right tabular-nums hairline focus:border-accent-500 focus:outline-none"
              aria-label="Fill opacity"
              @keydown="fieldKey($event, String(fillAlpha))"
              :disabled="!hasFill || editor.busy.value"
              @change="onFillAlpha"
            />
            <span class="text-[10px] text-ink-500">%</span>
          </div>

          <button type="button" class="mx-3 my-2 rounded border px-2 py-1 hairline hover:bg-shell-800" :disabled="editor.busy.value"
            @click="editor.setFill(hasFill ? null : { r: 76, g: 110, b: 245, a: 255 })">{{ hasFill ? 'Remove fill' : 'Add fill' }}</button>
          <p v-if="!hasFill" class="px-3 py-1 text-[10px] text-ink-700">
            No fill applied.
          </p>
        </InspectorSection>
      </template>
    </div>
  </aside>
</template>

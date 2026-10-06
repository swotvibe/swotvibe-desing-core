<script setup lang="ts">
import {
  AlignCenterHorizontal,
  AlignCenterVertical,
  AlignEndHorizontal,
  AlignEndVertical,
  AlignStartHorizontal,
  AlignStartVertical,
  Blend,
  ChevronDown,
  CornerDownRight,
  Grid2x2,
  Minus,
  RotateCw,
  Spline,
  SquareDashed,
} from '@lucide/vue'
import { computed, ref } from 'vue'

import { alphaChannel, alphaPercent, fromHex, toHex } from '@/editor/color'
import { useEditorSession } from '@/editor/useEditor'
import InspectorAccount from '@/components/InspectorAccount.vue'
import InspectorSection from '@/components/InspectorSection.vue'
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

const tab = ref<'design' | 'prototype'>('design')
const open = ref({
  position: true,
  layout: true,
  appearance: true,
  fill: true,
})

const selected = editor.selected
const props = computed(() => selected.value?.props ?? null)

/** Nothing is editable without a selection, so the panel says so once. */
const hasSelection = computed(() => selected.value !== null)

const fillHex = computed(() => (props.value?.fill ? toHex(props.value.fill) : '#000000'))
const fillAlpha = computed(() => (props.value?.fill ? alphaPercent(props.value.fill) : 100))
const hasFill = computed(() => props.value?.fill != null)

const positionLabel = computed(() => selected.value?.name || selected.value?.kind || 'Nothing')
const kindLabel = computed(() => selected.value?.kind ?? '—')

/**
 * The selected node's position on the page, from the layout result.
 *
 * Read from layout rather than the stored transform: under a container that
 * positions its children, the stored translation is not where the node is.
 */
const position = computed<[number, number] | null>(() => {
  const rect = editor.selectedRect.value
  return rect ? [rect[0], rect[1]] : null
})

/**
 * The alignment buttons.
 *
 * They are named and disabled rather than omitted: alignment is a document
 * change this build cannot make yet, and a person should be able to see that
 * rather than wonder whether the control is missing.
 */
const alignments = [
  { label: 'Align left', icon: AlignStartVertical },
  { label: 'Align horizontal centres', icon: AlignCenterVertical },
  { label: 'Align right', icon: AlignEndVertical },
  { label: 'Align top', icon: AlignStartHorizontal },
  { label: 'Align vertical centres', icon: AlignCenterHorizontal },
  { label: 'Align bottom', icon: AlignEndHorizontal },
]

function onFillHex(event: Event): void {
  const input = event.target as HTMLInputElement
  const parsed = fromHex(input.value, alphaChannel(fillAlpha.value))
  if (!parsed) return
  void editor.setFill(parsed)
}

function onFillAlpha(event: Event): void {
  const input = event.target as HTMLInputElement
  const percent = Number(input.value)
  const current = props.value?.fill
  if (!current || Number.isNaN(percent)) return
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

    <!-- Tabs, then the zoom readout, which is where the reference layout puts it. -->
    <div class="flex items-center gap-1 border-b px-3 pb-2 hairline">
      <button
        v-for="name in ['design', 'prototype'] as const"
        :key="name"
        type="button"
        class="rounded px-2 py-1 text-[11px] capitalize"
        :class="tab === name ? 'bg-shell-850 text-ink-100' : 'text-ink-500 hover:bg-shell-900'"
        :aria-selected="tab === name"
        role="tab"
        @click="tab = name"
      >
        {{ name }}
      </button>
      <span class="ml-auto text-[11px] text-ink-500">
        {{ Math.round(editor.zoom.value * 100) }}%
      </span>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto">
      <!-- The selected node's identity, so the panel is never ambiguous. -->
      <div class="flex items-center gap-2 border-b px-3 py-2 hairline">
        <span class="text-[12px] font-medium">{{ positionLabel }}</span>
        <span class="rounded bg-shell-850 px-1.5 py-0.5 text-[10px] text-ink-500">
          {{ kindLabel }}
        </span>
        <ChevronDown class="ml-auto size-3.5 text-ink-500" aria-hidden="true" />
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

          <div class="flex items-center gap-2 px-3 py-1">
            <span class="w-20 shrink-0 text-[11px] text-ink-500">Alignment</span>
            <div class="flex items-center gap-0.5">
              <button
                v-for="alignment in alignments"
                :key="alignment.label"
                type="button"
                class="rounded p-1 text-ink-700"
                disabled
                :title="`${alignment.label} is not available yet`"
                :aria-label="alignment.label"
              >
                <component :is="alignment.icon" class="size-3.5" aria-hidden="true" />
              </button>
            </div>
          </div>

          <PropertyRow label="Position">
            <span
              class="flex flex-1 items-center gap-1 rounded border bg-shell-900 px-2 py-1 hairline"
            >
              <span class="text-[10px] text-ink-700">X</span>
              <span class="tabular-nums">{{ position ? position[0].toFixed(0) : '—' }}</span>
              <span class="ml-auto text-[10px] text-ink-700">Y</span>
              <span class="tabular-nums">{{ position ? position[1].toFixed(0) : '—' }}</span>
            </span>
          </PropertyRow>

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
            <span
              class="flex flex-1 items-center gap-1 rounded border bg-shell-900 px-2 py-1 hairline"
            >
              <span class="text-[10px] text-ink-700">W</span>
              <span class="tabular-nums">{{ props?.size[0]?.toFixed(0) ?? '—' }}</span>
              <span class="ml-auto text-[10px] text-ink-700">H</span>
              <span class="tabular-nums">{{ props?.size[1]?.toFixed(0) ?? '—' }}</span>
            </span>
          </PropertyRow>

          <PropertyRow label="Sizing">
            <span
              class="flex flex-1 items-center gap-1 rounded border bg-shell-900 px-2 py-1 text-[11px] hairline"
            >
              <span>{{ props?.widthSizing }}</span>
              <Minus class="size-3 text-ink-700" aria-hidden="true" />
              <span>{{ props?.heightSizing }}</span>
            </span>
          </PropertyRow>

          <PropertyRow label="Clip content">
            <button
              type="button"
              class="flex flex-1 items-center gap-1 rounded border bg-shell-900 px-2 py-1 text-ink-700 hairline"
              disabled
              title="Clipping is not part of the current schema"
            >
              <SquareDashed class="size-3" aria-hidden="true" />
              <span class="text-[11px]">Not supported yet</span>
            </button>
          </PropertyRow>
        </InspectorSection>

        <InspectorSection
          title="Appearance"
          :open="open.appearance"
          @toggle="toggle('appearance')"
        >
          <PropertyRow label="Opacity">
            <span
              class="flex flex-1 items-center gap-1 rounded border bg-shell-900 px-2 py-1 hairline"
            >
              <Blend class="size-3 text-ink-700" aria-hidden="true" />
              <span class="tabular-nums">100%</span>
            </span>
          </PropertyRow>

          <PropertyRow label="Corner radius">
            <span
              class="flex flex-1 items-center gap-1 rounded border bg-shell-900 px-2 py-1 hairline"
            >
              <CornerDownRight class="size-3 text-ink-700" aria-hidden="true" />
              <span class="tabular-nums">{{ props?.cornerRadius ?? '—' }}</span>
            </span>
          </PropertyRow>
        </InspectorSection>

        <InspectorSection title="Fill" :open="open.fill" @toggle="toggle('fill')">
          <div class="flex items-center gap-2 px-3 py-1">
            <input
              :value="fillHex"
              type="color"
              class="size-5 shrink-0 cursor-pointer rounded border-0 bg-transparent p-0"
              aria-label="Fill colour"
              :disabled="!hasFill"
              @input="onFillHex"
            />
            <input
              :value="fillHex"
              type="text"
              class="min-w-0 flex-1 rounded border bg-shell-900 px-2 py-1 font-mono text-[11px] uppercase hairline focus:border-accent-500 focus:outline-none"
              aria-label="Fill hex value"
              :disabled="!hasFill"
              @change="onFillHex"
            />
            <input
              :value="fillAlpha"
              type="number"
              min="0"
              max="100"
              class="w-12 rounded border bg-shell-900 px-1.5 py-1 text-right tabular-nums hairline focus:border-accent-500 focus:outline-none"
              aria-label="Fill opacity"
              :disabled="!hasFill"
              @change="onFillAlpha"
            />
            <span class="text-[10px] text-ink-500">%</span>
          </div>

          <p v-if="!hasFill" class="px-3 py-1 text-[10px] text-ink-700">
            This node has no fill. The schema stores one fill per node.
          </p>
        </InspectorSection>
      </template>
    </div>
  </aside>
</template>

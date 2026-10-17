<script setup lang="ts">
import {
  Boxes,
  FileText,
  PenTool,
  Shapes,
  Sparkles,
  SwatchBook,
} from '@lucide/vue'
import { ref } from 'vue'

/**
 * The far-left rail.
 *
 * Items are declared as data so the rail stays a list rather than five copies of
 * the same button. Only the panels that exist today are enabled; the rest are
 * shown as unavailable rather than hidden, so the workspace does not silently
 * change shape when a panel is added.
 */
const items = [
  { id: 'file', label: 'File', icon: FileText, available: true },
  { id: 'agents', label: 'Agents', icon: Sparkles, available: false },
  { id: 'assets', label: 'Assets', icon: Boxes, available: false },
  { id: 'tools', label: 'Tools', icon: PenTool, available: false },
  { id: 'variables', label: 'Variables', icon: SwatchBook, available: false },
] as const

const active = ref<string>('file')
const brand = Shapes
</script>

<template>
  <nav
    class="flex flex-col items-center gap-1 border-r bg-shell-950 py-2 hairline"
    aria-label="Workspace panels"
  >
    <component :is="brand" class="mb-2 size-5 text-accent-400" aria-hidden="true" />

    <button
      v-for="item in items"
      :key="item.id"
      type="button"
      class="flex w-full flex-col items-center gap-1 rounded-md py-2 text-[10px] transition-colors"
      :class="
        active === item.id && item.available
          ? 'bg-shell-800 text-ink-100'
          : item.available
            ? 'text-ink-300 hover:bg-shell-900'
            : 'text-ink-700'
      "
      :disabled="!item.available"
      :aria-current="active === item.id ? 'page' : undefined"
      :title="item.available ? item.label : `${item.label} is not available yet`"
      @click="active = item.id"
    >
      <component :is="item.icon" class="size-4" aria-hidden="true" />
      <span>{{ item.label }}</span>
    </button>

    <div class="mt-auto flex flex-col items-center gap-1">
      <span
        class="flex size-7 items-center justify-center rounded-full bg-shell-700 text-[10px] font-semibold text-ink-100"
        title="Account"
      >
        AV
      </span>
    </div>
  </nav>
</template>

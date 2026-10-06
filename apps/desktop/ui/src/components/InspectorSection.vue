<script setup lang="ts">
import { ChevronDown } from '@lucide/vue'

/**
 * A collapsible inspector section.
 *
 * The open state is owned by the parent so a section can stay open across a
 * selection change, which is what a person expects while comparing nodes.
 */
defineProps<{
  title: string
  open: boolean
}>()

const emit = defineEmits<{
  toggle: []
}>()
</script>

<template>
  <section class="border-b hairline">
    <header class="flex items-center gap-1 px-3 py-2">
      <button
        type="button"
        class="flex flex-1 items-center gap-1 text-left text-[11px] font-semibold tracking-wide text-ink-300 uppercase"
        :aria-expanded="open"
        @click="emit('toggle')"
      >
        <ChevronDown
          class="size-3 text-ink-500 transition-transform"
          :class="open ? '' : '-rotate-90'"
          aria-hidden="true"
        />
        <span>{{ title }}</span>
      </button>
      <slot name="actions" />
    </header>
    <div v-if="open" class="pb-2">
      <slot />
    </div>
  </section>
</template>

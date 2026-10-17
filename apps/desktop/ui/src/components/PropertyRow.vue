<script setup lang="ts">
/**
 * One labelled control row.
 *
 * The label sits in a fixed-width column so the controls line up across
 * sections, which is what makes a long inspector scannable.
 */
defineProps<{
  label: string
}>()
</script>

<template>
  <div class="flex items-center gap-2 px-3 py-1">
    <span class="w-20 shrink-0 truncate text-[11px] text-ink-500" :title="label">{{ label }}</span>
    <div class="flex min-w-0 flex-1 items-center gap-1">
      <slot />
    </div>
  </div>
</template>

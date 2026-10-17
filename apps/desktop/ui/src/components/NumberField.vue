<script setup lang="ts">
/**
 * A numeric field for one property.
 *
 * Commits on `change` rather than on every keystroke: typing `12` would otherwise
 * send `1` first, and in a document that is a real edit with its own undo step.
 *
 * A half-typed or non-finite value is dropped and the field is restored to what
 * the document holds, so a field never shows a number that was not stored.
 */
const props = withDefaults(
  defineProps<{
    modelValue: number | string
    /** The short label shown before the field, e.g. `X`. */
    label: string
    disabled?: boolean
    min?: number
    /** A unit shown after the field, e.g. `°`. */
    suffix?: string
  }>(),
  { disabled: false, suffix: '' },
)

const emit = defineEmits<{
  commit: [value: number]
}>()

function onCommit(event: Event): void {
  const input = event.target as HTMLInputElement
  const parsed = input.value.trim() === '' ? NaN : Number(input.value)
  input.value = String(props.modelValue)
  if (!Number.isFinite(parsed) || (props.min !== undefined && parsed < props.min)) {
    input.value = String(props.modelValue)
    return
  }
  if (parsed !== Number(props.modelValue)) emit('commit', parsed)
}
function onKeydown(event: KeyboardEvent): void {
  if (event.isComposing || event.keyCode === 229) return
  const input = event.target as HTMLInputElement
  if (event.key === 'Enter') {
    event.preventDefault()
    onCommit(event)
    input.blur()
  } else if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    input.value = String(props.modelValue)
    input.blur()
  }
}
</script>

<template>
  <span
    class="flex min-w-0 flex-1 items-center gap-1 rounded border bg-shell-900 px-2 py-1.5 hairline focus-within:border-accent-500"
  >
    <span class="text-[10px] text-ink-700">{{ label }}</span>
    <input
      type="number"
      step="any"
      :min="min"
      class="w-full min-w-0 flex-1 bg-transparent text-right tabular-nums text-ink-100 outline-none disabled:text-ink-700"
      :value="modelValue"
      :disabled="disabled"
      :aria-label="label"
      @change="onCommit"
      @keydown="onKeydown"
    />
    <span v-if="suffix" class="text-[10px] text-ink-700">{{ suffix }}</span>
  </span>
</template>

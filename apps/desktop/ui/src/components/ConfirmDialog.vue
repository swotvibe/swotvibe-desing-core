<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { useEditorSession } from '@/editor/useEditor'

/**
 * The question asked before an action discards work.
 *
 * A design tool that replaces an open document or closes a window without asking
 * is worse than one that lacks a feature: the loss is silent, and there is no way
 * back because replacing a document discards its undo history too.
 *
 * Keys are bound while the question is open, capture-phase so they arrive before
 * a panel behind the dialog can act on them: Enter confirms, Escape declines. A
 * question that can only be answered with a mouse is a question a keyboard user
 * cannot answer. Focus moves to Cancel, so the safe answer is the default and a
 * stray Enter does not discard anything.
 */
const editor = useEditorSession()
const cancelButton = ref<HTMLButtonElement | null>(null)

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Enter') {
    event.preventDefault()
    // Enter takes the focused button; only an unfocused Enter defaults to cancel.
    if (document.activeElement === cancelButton.value) {
      editor.resolveConfirmation(false)
    } else {
      editor.resolveConfirmation(true)
    }
  } else if (event.key === 'Escape') {
    event.preventDefault()
    editor.resolveConfirmation(false)
  }
}

watch(
  () => editor.awaitingConfirmation.value,
  async (asking) => {
    if (asking) {
      window.addEventListener('keydown', onKeydown, true)
      await nextTick()
      cancelButton.value?.focus()
    } else {
      window.removeEventListener('keydown', onKeydown, true)
    }
  },
)

onMounted(() => {
  if (editor.awaitingConfirmation.value) {
    window.addEventListener('keydown', onKeydown, true)
  }
})

onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown, true))
</script>

<template>
  <!--
    Nothing renders until there is a question, so the dialog cannot be reached by
    a stray tab press while it is closed.
  -->
  <div
    v-if="editor.confirmation.value"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    role="presentation"
    @click.self="editor.resolveConfirmation(false)"
  >
    <div
      class="w-96 rounded-lg border border-white/10 bg-shell-850 p-4 shadow-[0_20px_60px_rgba(0,0,0,0.6)]"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-message"
      data-test="confirmation"
    >
      <h2 id="confirm-title" class="text-[13px] font-semibold text-ink-100">
        Unsaved changes
      </h2>
      <p id="confirm-message" class="mt-2 text-[12px] text-ink-300">
        {{ editor.confirmation.value.message }}
      </p>
      <p class="mt-2 text-[11px] text-ink-500">
        Save first if you want to keep them.
      </p>

      <div class="mt-4 flex justify-end gap-2">
        <button
          type="button"
          ref="cancelButton"
          class="rounded-md border px-3 py-1.5 text-[12px] text-ink-100 hover:bg-shell-800 focus:border-accent-500 focus:outline-none hairline"
          data-test="confirm-cancel"
          @click="editor.resolveConfirmation(false)"
        >
          Cancel
        </button>
        <button
          type="button"
          class="rounded-md bg-red-600 px-3 py-1.5 text-[12px] font-medium text-white hover:bg-red-500"
          data-test="confirm-accept"
          @click="editor.resolveConfirmation(true)"
        >
          {{ editor.confirmation.value.confirmLabel }}
        </button>
      </div>
    </div>
  </div>
</template>

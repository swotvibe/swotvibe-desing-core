<script setup lang="ts">
import { ChevronDown, ChevronRight } from '@lucide/vue'
import { computed, nextTick, ref, watch } from 'vue'

import { useEditorSession } from '@/editor/useEditor'
import type { NodeView } from '@/bridge/types'

/**
 * The document panel: pages, then the layer tree.
 *
 * Selection lives in the editor controller, not in this component, so the canvas
 * and the tree cannot disagree about what is selected.
 */
const editor = useEditorSession()

const layerQuery = ref('')
const collapsed = ref(new Set<string>())
const tree = ref<HTMLElement | null>(null)
const query = computed(() => layerQuery.value.trim().toLowerCase())

/** The kind's icon, kept as a name so the tree stays data rather than markup. */
const kindGlyph: Record<string, string> = {
  frame: '#',
  shape: '▣',
  text: 'T',
  image: '◫',
  group: '❏',
}

function toggle(id: string): void {
  const next = new Set(collapsed.value)
  if (next.has(id)) {
    next.delete(id)
  } else {
    next.add(id)
  }
  collapsed.value = next
}

interface LayerRow { node: NodeView; depth: number }

// Build each page tree once. Rewalking every subtree and ancestor for every row
// made filtering a deeply nested document quadratic and could exhaust the stack.
const pageRows = computed(() => {
  const rows: LayerRow[] = []
  const pending = editor.roots.value.map((node) => ({ node, depth: 0 })).reverse()
  const visited = new Set<string>()
  while (pending.length) {
    const row = pending.pop()!
    if (visited.has(row.node.id)) continue
    visited.add(row.node.id)
    rows.push(row)
    for (let i = row.node.children.length - 1; i >= 0; i--) {
      const node = editor.nodesById.value.get(row.node.children[i]!)
      if (node) pending.push({ node, depth: row.depth + 1 })
    }
  }
  return rows
})

const visibleRows = computed(() => {
  if (query.value) {
    const matches = new Set<string>()
    for (let i = pageRows.value.length - 1; i >= 0; i--) {
      const { node } = pageRows.value[i]!
      if (node.name.toLowerCase().includes(query.value) || matches.has(node.id)) {
        matches.add(node.id)
        if (node.parent) matches.add(node.parent)
      }
    }
    return pageRows.value.filter(({ node }) => matches.has(node.id))
  }
  const rows: LayerRow[] = []
  let hiddenBelow = Infinity
  for (const row of pageRows.value) {
    if (row.depth > hiddenBelow) continue
    rows.push(row)
    hiddenBelow = collapsed.value.has(row.node.id) ? row.depth : Infinity
  }
  return rows
})

watch(editor.selectedId, async (id) => {
  const next = new Set(collapsed.value)
  const visited = new Set<string>()
  let parent = id ? editor.nodesById.value.get(id)?.parent : null
  while (parent && !visited.has(parent)) {
    visited.add(parent)
    next.delete(parent)
    parent = editor.nodesById.value.get(parent)?.parent
  }
  collapsed.value = next
  await nextTick()
  if (id === editor.selectedId.value) {
    tree.value?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView?.({ block: 'nearest' })
  }
})
</script>

<template>
  <div class="flex min-h-0 flex-col">
    <!-- Pages -->
    <section class="border-b hairline">
      <header class="flex items-center gap-1 px-3 py-2">
        <h2 class="text-[11px] font-semibold tracking-wide text-ink-500 uppercase">Pages</h2>


      </header>

      <ul class="max-h-40 overflow-y-auto pb-2">
        <li v-for="page in editor.view.value?.pages ?? []" :key="page.id">
          <button type="button"
            @click="editor.selectPage(page.id)"
            class="flex w-full items-center gap-2 bg-shell-850 px-3 py-1.5 text-left"
            :aria-current="editor.activePageId.value === page.id ? 'page' : undefined"
            :class="editor.activePageId.value === page.id ? 'text-accent-400' : 'text-ink-500'"
          >
            <span class="size-2 rounded-sm bg-shell-700" aria-hidden="true" />
            <span class="truncate">{{ page.name }}</span>
          </button>
        </li>
      </ul>
    </section>

    <!-- Layers -->
    <section class="flex min-h-0 flex-1 flex-col">
      <header class="flex items-center gap-1 border-b px-3 py-2 hairline">
        <h2 class="text-[11px] font-semibold tracking-wide text-ink-500 uppercase">Layers</h2>
        <span class="ml-auto text-[10px] text-ink-700">
          {{ pageRows.length }}
        </span>
      </header>

      <div class="px-3 py-2">
        <label class="sr-only" for="layer-search">Filter layers</label>
        <input
          id="layer-search"
          v-model="layerQuery"
          type="search"
          placeholder="Filter layers"
          class="w-full rounded border bg-shell-950 px-2 py-1 text-[12px] text-ink-100 placeholder:text-ink-700 hairline focus:border-accent-500 focus:outline-none"
        />
      </div>

      <p v-if="visibleRows.length === 0" data-test="no-layer-results" role="status" class="px-3 py-4 text-[12px] leading-relaxed text-ink-500">
        {{ query ? `No layers match “${layerQuery.trim()}”.` : editor.activePageId.value ? 'This page has no layers. Add a frame, shape or text using the toolbar.' : 'Open or create a document to see its layers.' }}
      </p>
      <ul ref="tree" class="min-h-0 flex-1 overflow-y-auto pb-4" role="tree" aria-label="Layers">
        <li
          v-for="{ node, depth } in visibleRows"
          :key="node.id"
          role="treeitem"
          :aria-selected="node.id === editor.selectedId.value"
          :aria-level="depth + 1"
          :aria-expanded="node.children.length ? !!query || !collapsed.has(node.id) : undefined"
        >
          <div
            class="flex items-center gap-1 pr-2"
            :style="{ paddingLeft: `${0.5 + depth * 0.9}rem` }"
          >
            <button
              v-if="node.children.length > 0"
              type="button"
              class="rounded p-0.5 text-ink-500 hover:bg-shell-800"
              :aria-label="!query && collapsed.has(node.id) ? 'Expand' : 'Collapse'"
              :disabled="!!query"
              :title="query ? 'Clear the filter to collapse layers' : undefined"
              @click.stop="toggle(node.id)"
            >
              <component
                :is="!query && collapsed.has(node.id) ? ChevronRight : ChevronDown"
                class="size-3"
                aria-hidden="true"
              />
            </button>
            <span v-else class="w-4" aria-hidden="true" />

            <button
              type="button"
              class="flex min-w-0 flex-1 items-center gap-2 rounded py-1.5 pl-1 text-left"
              :class="
                node.id === editor.selectedId.value
                  ? 'bg-accent-600/25 text-ink-100'
                  : 'text-ink-300 hover:bg-shell-900'
              "
              @click="editor.select(node.id)"
            >
              <span class="w-3.5 shrink-0 text-center text-[10px] text-ink-500" aria-hidden="true">
                {{ kindGlyph[node.kind] ?? '•' }}
              </span>
              <span class="truncate">{{ node.name || node.id.slice(-6) }}</span>
              <!--
                A container is marked because it is not painted itself, not to
                imply it cannot be selected.
              -->
              <span
                v-if="node.children.length > 0"
                class="ml-auto shrink-0 text-[9px] text-ink-700"
                :title="`${node.children.length} children`"
              >
                ▾{{ node.children.length }}
              </span>
            </button>
          </div>
        </li>
      </ul>
    </section>
  </div>
</template>

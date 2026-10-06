<script setup lang="ts">
import { ChevronDown, ChevronRight, Plus, Search } from '@lucide/vue'
import { ref } from 'vue'

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

/**
 * Flattens the tree for rendering.
 *
 * A search filters to matching nodes and their ancestors rather than dropping
 * context, because a layer without its parent is hard to place.
 */
function visibleNodes(): NodeView[] {
  const query = layerQuery.value.trim().toLowerCase()
  const out: NodeView[] = []

  const matches = (node: NodeView): boolean =>
    query === '' || node.name.toLowerCase().includes(query)

  const subtreeMatches = (node: NodeView): boolean =>
    matches(node) || node.children.some((id) => subtreeMatchesById(id))

  const subtreeMatchesById = (id: string): boolean => {
    const node = editor.nodesById.value.get(id)
    return node ? subtreeMatches(node) : false
  }

  const walk = (node: NodeView): void => {
    if (!subtreeMatches(node)) return
    out.push(node)
    if (collapsed.value.has(node.id)) return
    for (const childId of node.children) {
      const child = editor.nodesById.value.get(childId)
      if (child) walk(child)
    }
  }

  for (const root of editor.roots.value) walk(root)
  return out
}

function depthOf(node: NodeView): number {
  let depth = 0
  let current = node.parent
  while (current) {
    depth += 1
    current = editor.nodesById.value.get(current)?.parent ?? null
  }
  return depth
}
</script>

<template>
  <div class="flex min-h-0 flex-col">
    <!-- Pages -->
    <section class="border-b hairline">
      <header class="flex items-center gap-1 px-3 py-2">
        <h2 class="text-[11px] font-semibold tracking-wide text-ink-500 uppercase">Pages</h2>
        <button
          type="button"
          class="ml-auto rounded p-1 text-ink-500 hover:bg-shell-900"
          title="Search pages"
          aria-label="Search pages"
        >
          <Search class="size-3.5" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="rounded p-1 text-ink-500 hover:bg-shell-900"
          title="Add page"
          aria-label="Add page"
          disabled
        >
          <Plus class="size-3.5" aria-hidden="true" />
        </button>
      </header>

      <ul class="pb-2">
        <li v-for="page in editor.view.value?.pages ?? []" :key="page.id">
          <!--
            One page today, so this is a label rather than a control. A button
            that looks clickable and does nothing is worse than a row that does
            not invite the click.
          -->
          <div
            class="flex w-full items-center gap-2 bg-shell-850 px-3 py-1.5 text-left"
            :aria-current="'page'"
          >
            <span class="size-2 rounded-sm bg-shell-700" aria-hidden="true" />
            <span class="truncate">{{ page.name }}</span>
          </div>
        </li>
      </ul>
    </section>

    <!-- Layers -->
    <section class="flex min-h-0 flex-1 flex-col">
      <header class="flex items-center gap-1 border-b px-3 py-2 hairline">
        <h2 class="text-[11px] font-semibold tracking-wide text-ink-500 uppercase">Layers</h2>
        <span class="ml-auto text-[10px] text-ink-700">
          {{ editor.view.value?.nodes.length ?? 0 }}
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

      <ul class="min-h-0 flex-1 overflow-y-auto pb-4" role="tree">
        <li
          v-for="node in visibleNodes()"
          :key="node.id"
          role="treeitem"
          :aria-selected="node.id === editor.selectedId.value"
        >
          <div
            class="flex items-center gap-1 pr-2"
            :style="{ paddingLeft: `${0.5 + depthOf(node) * 0.9}rem` }"
          >
            <button
              v-if="node.children.length > 0"
              type="button"
              class="rounded p-0.5 text-ink-500 hover:bg-shell-800"
              :aria-label="collapsed.has(node.id) ? 'Expand' : 'Collapse'"
              @click.stop="toggle(node.id)"
            >
              <component
                :is="collapsed.has(node.id) ? ChevronRight : ChevronDown"
                class="size-3"
                aria-hidden="true"
              />
            </button>
            <span v-else class="w-4" aria-hidden="true" />

            <button
              type="button"
              class="flex min-w-0 flex-1 items-center gap-2 rounded py-1 pl-1 text-left"
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

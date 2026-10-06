import { computed, inject, provide, ref, type ComputedRef, type Ref } from 'vue'

import {
  AppError,
  type DocumentView,
  type EditorBridge,
  type LayoutView,
  type NodeView,
  type Rgba,
} from '@/bridge/types'
import { editorKey, editorStateKey, type ToolId } from './context'

/** The canvas the sample document is drawn at, matching its committed artboard. */
export const ARTBOARD: [number, number] = [480, 320]

/** The preview options the shell uses. Zoom is applied by the canvas, not here. */
export const PREVIEW_OPTIONS = {
  pageSize: ARTBOARD,
  scale: 1,
  background: [255, 255, 255, 255] as [number, number, number, number],
}

export interface EditorFailure {
  code: string
  message: string
}

type EditCommands = Parameters<EditorBridge['apply']>[0]['commands']

/**
 * The editor session.
 *
 * ## What lives here and what does not
 *
 * Only **session state**: which node is selected, which tool is active, the zoom,
 * and the last failure to show. The document is read from the service and every
 * mutation goes back through it. Keeping a second, editable copy of the document
 * in the interface is the mistake this shape prevents — undo would then have two
 * histories.
 *
 * ## Revisions
 *
 * `revision` is whatever the service last reported. An edit sends it back, so a
 * stale interface is refused instead of overwriting work it never saw.
 */
export interface EditorSession {
  view: Ref<DocumentView | null>
  layout: Ref<LayoutView | null>
  nodesById: ComputedRef<Map<string, NodeView>>
  roots: ComputedRef<NodeView[]>
  selected: ComputedRef<NodeView | null>
  selectedId: Ref<string | null>
  selectedRect: ComputedRef<[number, number, number, number] | null>
  selectedRotation: ComputedRef<number>
  activeTool: Ref<ToolId>
  zoom: Ref<number>
  busy: Ref<boolean>
  status: Ref<string>
  failure: Ref<EditorFailure | null>
  revision: ComputedRef<number>
  dirty: ComputedRef<boolean>
  load: () => Promise<void>
  edit: (commands: EditCommands) => Promise<void>
  select: (id: string | null) => void
  selectAt: (pageX: number, pageY: number) => Promise<void>
  setFill: (fill: Rgba | null) => Promise<void>
  renameSelected: (name: string) => Promise<void>
  undo: () => Promise<void>
  redo: () => Promise<void>
  save: () => Promise<void>
  open: () => Promise<void>
  openSample: () => Promise<void>
  canOpen: ComputedRef<boolean>
  canOpenSample: ComputedRef<boolean>
  setZoom: (next: number) => void
  clearFailure: () => void
}

/** Builds a session over a service. */
export function createEditorSession(bridge: EditorBridge): EditorSession {
  const view = ref<DocumentView | null>(null)
  const layout = ref<LayoutView | null>(null)
  const selectedId = ref<string | null>(null)
  const activeTool = ref<ToolId>('select')
  const zoom = ref(0.21)
  const busy = ref(false)
  const failure = ref<EditorFailure | null>(null)
  const status = ref('Ready')

  const nodesById = computed(() => {
    const map = new Map<string, NodeView>()
    for (const node of view.value?.nodes ?? []) {
      map.set(node.id, node)
    }
    return map
  })

  const selected = computed<NodeView | null>(() => {
    const id = selectedId.value
    return id ? (nodesById.value.get(id) ?? null) : null
  })

  const roots = computed<NodeView[]>(() => {
    const page = view.value?.pages[0]
    if (!page) return []
    return page.roots
      .map((id) => nodesById.value.get(id))
      .filter((node): node is NodeView => node !== undefined)
  })

  const selectedRect = computed<[number, number, number, number] | null>(() => {
    const id = selectedId.value
    if (!id) return null
    return layout.value?.nodes.find((node) => node.id === id)?.rect ?? null
  })

  /**
   * The selected node's rotation in degrees, read from its resolved transform.
   *
   * The angle comes from the layout pass, not from the stored transform alone:
   * under a container that positions its children, the stored value is not where
   * the node ended up.
   */
  const selectedRotation = computed<number>(() => {
    const id = selectedId.value
    const node = id ? layout.value?.nodes.find((entry) => entry.id === id) : undefined
    if (!node) return 0
    const [a, b] = node.world
    const degrees = (Math.atan2(b, a) * 180) / Math.PI
    return Math.round(degrees * 10) / 10
  })

  const revision = computed(() => view.value?.revision ?? 0)
  const dirty = computed(() => view.value?.dirty ?? false)

  function report(cause: unknown, fallback: string): void {
    if (cause instanceof AppError) {
      failure.value = { code: cause.code, message: cause.message }
      status.value = cause.message
      return
    }
    // A service that failed before it could shape an error still has to surface
    // something a person can act on.
    const shape = cause as { code?: unknown; message?: unknown } | null
    const message = typeof shape?.message === 'string' ? shape.message : fallback
    const code = typeof shape?.code === 'string' ? shape.code : 'invalid-request'
    failure.value = { code, message }
    status.value = message
  }

  function clearFailure(): void {
    failure.value = null
  }

  async function refresh(): Promise<void> {
    const previousPage = view.value?.pages[0]?.id
    const next = await bridge.getView()
    view.value = next
    const page = next.pages[0]?.id ?? previousPage
    if (page) {
      layout.value = await bridge.layout(page, PREVIEW_OPTIONS)
    }
    // A node can disappear under an undo, so a selection is validated rather
    // than kept on faith.
    if (selectedId.value && !nodesById.value.has(selectedId.value)) {
      selectedId.value = null
    }
  }

  async function load(): Promise<void> {
    busy.value = true
    try {
      await refresh()
      status.value = `Opened ${view.value?.displayName ?? 'document'}`
    } catch (cause) {
      report(cause, 'the document could not be opened')
    } finally {
      busy.value = false
    }
  }

  async function edit(commands: EditCommands): Promise<void> {
    if (commands.length === 0) return
    busy.value = true
    try {
      const summary = await bridge.apply({ expectedRevision: revision.value, commands })
      await refresh()
      clearFailure()
      status.value = `Revision ${summary.revision}`
    } catch (cause) {
      report(cause, 'the edit was refused')
    } finally {
      busy.value = false
    }
  }

  async function selectAt(pageX: number, pageY: number): Promise<void> {
    const page = view.value?.pages[0]?.id
    if (!page) return
    try {
      const hit = await bridge.hitTest(page, pageX, pageY, PREVIEW_OPTIONS)
      selectedId.value = hit.node
      const name = hit.node ? nodesById.value.get(hit.node)?.name || hit.node : ''
      status.value = hit.node ? `Selected ${name}` : 'Nothing selected'
    } catch (cause) {
      report(cause, 'the hit test failed')
    }
  }

  function select(id: string | null): void {
    selectedId.value = id
  }

  async function setFill(fill: Rgba | null): Promise<void> {
    const id = selectedId.value
    if (!id) return
    await edit([{ kind: 'set-node-fill', node: id, fill }])
  }

  async function renameSelected(name: string): Promise<void> {
    const id = selectedId.value
    if (!id) return
    await edit([{ kind: 'rename-node', node: id, name: name === '' ? null : name }])
  }

  async function undo(): Promise<void> {
    busy.value = true
    try {
      await bridge.undo(revision.value)
      await refresh()
      clearFailure()
      status.value = 'Undo'
    } catch (cause) {
      report(cause, 'there is nothing to undo')
    } finally {
      busy.value = false
    }
  }

  async function redo(): Promise<void> {
    busy.value = true
    try {
      await bridge.redo(revision.value)
      await refresh()
      clearFailure()
      status.value = 'Redo'
    } catch (cause) {
      report(cause, 'there is nothing to redo')
    } finally {
      busy.value = false
    }
  }

  async function save(): Promise<void> {
    busy.value = true
    try {
      // A host that can show a dialog owns the write, so it decides the
      // destination and reports back. Without one, the bytes are exported and
      // recorded as saved, which is the most a browser build can honestly do.
      if (bridge.saveDocument) {
        const written = await bridge.saveDocument()
        if (written === null) {
          // Cancelling is an answer, not a failure: reporting it as an error
          // would train a person to ignore errors.
          status.value = 'Save cancelled'
          return
        }
        await refresh()
        clearFailure()
        status.value = `Saved ${written}`
        return
      }
      const bytes = await bridge.exportBytes()
      await bridge.noteSaved()
      await refresh()
      clearFailure()
      status.value = `Saved ${bytes.byteLength} bytes`
    } catch (cause) {
      report(cause, 'the document could not be saved')
    } finally {
      busy.value = false
    }
  }

  /** Whether this build can ask a host for a file. */
  const canOpen = computed(() => bridge.openDocument !== undefined)

  /** Whether this build can open the committed sample. */
  const canOpenSample = computed(() => bridge.openSample !== undefined)

  async function openSample(): Promise<void> {
    if (!bridge.openSample) {
      report(
        new AppError({
          code: 'document-read',
          message: 'This build has no sample document to open.',
        }),
        'this build has no sample document',
      )
      return
    }
    busy.value = true
    try {
      const opened = await bridge.openSample()
      selectedId.value = null
      await refresh()
      clearFailure()
      status.value = `Opened ${opened.displayName ?? 'the sample'}`
    } catch (cause) {
      report(cause, 'the sample could not be opened')
    } finally {
      busy.value = false
    }
  }

  async function open(): Promise<void> {
    if (!bridge.openDocument) {
      report(
        new AppError({
          code: 'document-read',
          message: 'This build cannot read files; the desktop host supplies a file picker.',
        }),
        'this build cannot read files',
      )
      return
    }
    busy.value = true
    try {
      const opened = await bridge.openDocument()
      if (opened === null) {
        status.value = 'Open cancelled'
        return
      }
      // A newly opened document has no selection, because the old one names a
      // node that the new document does not have.
      selectedId.value = null
      await refresh()
      clearFailure()
      status.value = `Opened ${opened.displayName ?? 'document'}`
    } catch (cause) {
      report(cause, 'the document could not be opened')
    } finally {
      busy.value = false
    }
  }

  function setZoom(next: number): void {
    zoom.value = Math.min(64, Math.max(0.02, next))
  }

  return {
    view,
    layout,
    nodesById,
    roots,
    selected,
    selectedId,
    selectedRect,
    selectedRotation,
    activeTool,
    zoom,
    busy,
    status,
    failure,
    revision,
    dirty,
    load,
    edit,
    select,
    selectAt,
    setFill,
    renameSelected,
    undo,
    redo,
    save,
    open,
    openSample,
    canOpen,
    canOpenSample,
    setZoom,
    clearFailure,
  }
}

/**
 * Creates the session for a subtree and makes it available to every component
 * below it. Called once, by the shell.
 */
export function provideEditorSession(bridge: EditorBridge): EditorSession {
  const session = createEditorSession(bridge)
  provide(editorStateKey, session)
  return session
}

/** Reads the session the shell created. */
export function useEditorSession(): EditorSession {
  const session = inject(editorStateKey)
  if (!session) {
    throw new Error('the editor session was not provided')
  }
  return session
}

/** Reads the service a host provided. */
export function useEditorBridge(): EditorBridge {
  const bridge = inject(editorKey)
  if (!bridge) {
    throw new Error('the editor bridge was not provided')
  }
  return bridge
}

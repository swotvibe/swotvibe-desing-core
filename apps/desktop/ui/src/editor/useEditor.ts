import { computed, inject, provide, ref, type ComputedRef, type Ref } from 'vue'

import {
  AppError,
  type Capabilities,
  type DocumentView,
  type EditorBridge,
  type LayoutView,
  type NodeView,
  type Rgba,
} from '@/bridge/types'
import { editorKey, editorStateKey, type ToolId } from './context'

/**
 * A new node identity.
 *
 * The v4 shape the document schema expects. Generated here rather than by the
 * service so the caller can name a node it is about to create — which is what
 * lets a batch refer to it and the selection adopt it.
 */
function newIdentity(): string {
  return crypto.randomUUID()
}

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

/**
 * A question the interface must answer before an action can proceed.
 *
 * Some actions destroy work — replacing an open document, closing the window —
 * and a design tool that discards edits without asking is worse than one that
 * lacks a feature. The question is queued here rather than blocking, so the
 * component that asked stays a plain function and the dialog stays a component.
 */
export interface PendingConfirmation {
  /** What is about to happen, in one sentence. */
  message: string
  /** The label of the button that proceeds. */
  confirmLabel: string
  /** Runs when the person agrees. */
  onConfirm: () => void | Promise<void>
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
  setPosition: (x: number, y: number) => Promise<void>
  setSize: (width: number, height: number) => Promise<void>
  setCornerRadius: (radius: number) => Promise<void>
  renameSelected: (name: string) => Promise<void>
  undo: () => Promise<void>
  redo: () => Promise<void>
  save: () => Promise<void>
  open: () => void
  openSample: () => void
  confirmation: Ref<PendingConfirmation | null>
  awaitingConfirmation: ComputedRef<boolean>
  resolveConfirmation: (accept: boolean) => void
  createNode: (kind: string, name?: string) => Promise<void>
  deleteSelected: () => Promise<void>
  capabilities: Ref<Capabilities | null>
  canCreate: (kind: string) => boolean
  loadCapabilities: () => Promise<void>
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
  const confirmation = ref<PendingConfirmation | null>(null)

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
      await loadCapabilities()
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

  /** Moves the selected node. Refused by the service when the parent lays it out. */
  async function setPosition(x: number, y: number): Promise<void> {
    const id = selectedId.value
    if (!id) return
    await edit([{ kind: 'set-node-position', node: id, position: [x, y] }])
  }

  /** Resizes the selected node, switching both axes to a fixed size. */
  async function setSize(width: number, height: number): Promise<void> {
    const id = selectedId.value
    if (!id) return
    await edit([{ kind: 'set-node-size', node: id, size: [width, height] }])
  }

  /** Sets the selected shape's corner radius. Refused on a node that has none. */
  async function setCornerRadius(radius: number): Promise<void> {
    const id = selectedId.value
    if (!id) return
    await edit([{ kind: 'set-node-corner-radius', node: id, radius }])
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

  /** What the running build can do. Empty until it answers, so nothing is offered early. */
  const capabilities = ref<Capabilities | null>(null)

  async function loadCapabilities(): Promise<void> {
    try {
      capabilities.value = await bridge.capabilities()
    } catch (cause) {
      // A build that cannot describe itself still edits; only the offered
      // controls fall back to the conservative defaults below.
      report(cause, 'the build did not report its capabilities')
    }
  }

  /**
   * The kinds a tool may create.
   *
   * Falls back to an empty list rather than to an assumption: offering to
   * create a kind the build cannot handle would fail on the first edit.
   */
  const creatableKinds = computed<string[]>(
    () => capabilities.value?.creatableKinds ?? [],
  )

  function canCreate(kind: string): boolean {
    return creatableKinds.value.includes(kind)
  }

  /**
   * The page a new node should join.
   *
   * A document always has at least one page after opening one, so this is the
   * first page. `null` when there is none, which is a document state rather than
   * an error.
   */
  function activePage(): string | null {
    return view.value?.pages[0]?.id ?? null
  }

  /**
   * Creates a node of `kind` on the active page and selects it.
   *
   * The identity is generated here and sent on the wire so the same batch can
   * refer to the node, and so the selection can name it immediately.
   */
  async function createNode(kind: string, name?: string): Promise<void> {
    const page = activePage()
    if (!page) {
      report(
        new AppError({
          code: 'unknown-page',
          message: 'There is no page to add to. Open or create a document first.',
        }),
        'there is no page to add to',
      )
      return
    }
    if (!canCreate(kind)) {
      report(
        new AppError({
          code: 'invalid-request',
          message: `This build cannot create a ${kind} node.`,
        }),
        'that kind cannot be created',
      )
      return
    }

    const id = newIdentity()
    await edit([
      {
        kind: 'create-node',
        node: id,
        nodeKind: kind,
        name: name ?? null,
        // Left unset: the service supplies the kind's default, including a
        // registered font for text, which is knowledge this layer does not have.
        fill: null,
        parent: { in: 'page-root', page },
      },
    ])
    // Selecting the new node is what makes the create visible: the inspector
    // fills in and the layer tree highlights it.
    if (nodesById.value.has(id)) {
      selectedId.value = id
    }
  }

  /** Deletes the selected node and its subtree. */
  async function deleteSelected(): Promise<void> {
    const id = selectedId.value
    if (!id) return
    await edit([{ kind: 'delete-node', node: id }])
  }

  /**
   * Runs an action that replaces the open document, asking first when there is
   * unsaved work.
   *
   * Replacing a document discards the undo timeline as well as the edits, so
   * there is no way back from it. The question is asked before anything is read
   * or written, and nothing happens if it is declined.
   */
  function guardUnsaved(message: string, confirmLabel: string, action: () => void | Promise<void>): void {
    if (!dirty.value) {
      void action()
      return
    }
    confirmation.value = {
      message,
      confirmLabel,
      onConfirm: async () => {
        confirmation.value = null
        await action()
      },
    }
  }

  /** Whether a question is waiting for an answer. */
  const awaitingConfirmation = computed(() => confirmation.value !== null)

  function resolveConfirmation(accept: boolean): void {
    const pending = confirmation.value
    confirmation.value = null
    if (accept && pending) {
      void pending.onConfirm()
    } else {
      status.value = 'Cancelled'
    }
  }

  function openSampleInternal(): void {
    // Bound to its receiver on purpose: a method taken off its object and called
    // bare loses `this`, and this one assigns to the service's own state.
    const openSampleFn = bridge.openSample?.bind(bridge)
    if (!openSampleFn) {
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
    void (async () => {
      try {
        const opened = await openSampleFn()
        selectedId.value = null
        await refresh()
        clearFailure()
        status.value = `Opened ${opened.displayName ?? 'the sample'}`
      } catch (cause) {
        report(cause, 'the sample could not be opened')
      } finally {
        busy.value = false
      }
    })()
  }

  function openSample(): void {
    guardUnsaved(
      'Opening the sample replaces the open document and discards its undo history.',
      'Discard and open',
      openSampleInternal,
    )
  }

  function openInternal(): void {
    // Bound for the same reason as `openSampleInternal`: a detached method loses
    // its receiver, and the service methods write to the service.
    const openDocumentFn = bridge.openDocument?.bind(bridge)
    if (!openDocumentFn) {
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
    void (async () => {
      try {
        const opened = await openDocumentFn()
        if (opened === null) {
          // Cancelling a picker is an answer, not a failure.
          status.value = 'Open cancelled'
          return
        }
        // A newly opened document has no selection: the old one names a node the
        // new document does not have.
        selectedId.value = null
        await refresh()
        clearFailure()
        status.value = `Opened ${opened.displayName ?? 'document'}`
      } catch (cause) {
        report(cause, 'the document could not be opened')
      } finally {
        busy.value = false
      }
    })()
  }

  function open(): void {
    guardUnsaved(
      'Opening a document replaces the current one and discards its undo history.',
      'Discard and open',
      openInternal,
    )
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
    setPosition,
    setSize,
    setCornerRadius,
    renameSelected,
    undo,
    redo,
    save,
    open,
    openSample,
    confirmation,
    awaitingConfirmation,
    resolveConfirmation,
    createNode,
    deleteSelected,
    capabilities,
    canCreate,
    canOpen,
    canOpenSample,
    loadCapabilities,
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

import { computed, inject, nextTick, provide, ref, type ComputedRef, type Ref } from 'vue'

import {
  AppError,
  type Capabilities,
  type DocumentView,
  type EditorBridge,
  type LayoutView,
  type NodeView,
  type PreviewOptions,
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

/** A sole, untransformed root frame is the document's artboard. */
function pageOptions(document: DocumentView | null, page: string): PreviewOptions {
  const roots = document?.pages.find((item) => item.id === page)?.roots ?? []
  const frame = roots.length === 1 ? document?.nodes.find((node) => node.id === roots[0]) : null
  if (frame?.kind === 'frame' && frame.props.widthSizing === 'fixed' && frame.props.heightSizing === 'fixed'
    && frame.props.transform.every((value, index) => value === [1, 0, 0, 1, 0, 0][index])
    && frame.props.size.every((value) => Number.isFinite(value) && value > 0)) {
    return { ...PREVIEW_OPTIONS, pageSize: [...frame.props.size] }
  }
  return PREVIEW_OPTIONS
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
  previewUrl: Ref<string | null>
  dispose: () => void
  whenIdle: () => Promise<void>
  nudge: (dx: number, dy: number) => Promise<void>
  view: Ref<DocumentView | null>
  layout: Ref<LayoutView | null>
  nodesById: ComputedRef<Map<string, NodeView>>
  roots: ComputedRef<NodeView[]>
  selected: ComputedRef<NodeView | null>
  selectedId: Ref<string | null>
  selectedRect: ComputedRef<[number, number, number, number] | null>
  selectedRotation: ComputedRef<number>
  activePageId: Ref<string | null>
  selectPage: (id: string) => Promise<void>
  canUndo: ComputedRef<boolean>
  canRedo: ComputedRef<boolean>
  setText: (content: string, fontSize: number) => Promise<void>
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
  selectAt: (pageX: number, pageY: number) => Promise<boolean>
  setFill: (fill: Rgba | null) => Promise<void>
  setPosition: (x: number, y: number) => Promise<void>
  setSize: (width: number, height: number) => Promise<void>
  setCornerRadius: (radius: number) => Promise<void>
  renameSelected: (name: string) => Promise<void>
  undo: () => Promise<void>
  redo: () => Promise<void>
  save: () => Promise<void>
  newDocument: () => void
  canNew: boolean
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
  const previewUrl = ref<string | null>(null)
  let disposed = false
  function clearPreview(): void {
    if (previewUrl.value) URL.revokeObjectURL(previewUrl.value)
    previewUrl.value = null
  }
  function dispose(): void { disposed = true; clearPreview() }
  const view = ref<DocumentView | null>(null)
  const layout = ref<LayoutView | null>(null)
  const selectedId = ref<string | null>(null)
  const activeTool = ref<ToolId>('select')
  const zoom = ref(1)
  const activePageId = ref<string | null>(null)
  let selectionRequest = 0
  let pendingOperations = 0
  let operationTail = Promise.resolve()
  let mutationSequence = 0
  let pendingSave: { sequence: number; promise: Promise<void> } | null = null
  let pendingReplacement = false
  const busy = ref(false)
  const failure = ref<EditorFailure | null>(null)
  const status = ref('Ready')
  const confirmation = ref<PendingConfirmation | null>(null)

  // Read the revision only when an operation reaches the front of the queue.
  // Rapid colour changes and keyboard repeats must never share a stale revision.
  function enqueue(action: () => Promise<void>): Promise<void> {
    if (disposed) return Promise.resolve()
    pendingOperations += 1
    busy.value = true
    const result = operationTail.then(() => disposed ? undefined : action()).finally(() => {
      pendingOperations -= 1
      busy.value = pendingOperations > 0
    })
    operationTail = result.catch(() => {})
    return result
  }

  async function whenIdle(): Promise<void> {
    let tail: Promise<void>
    do {
      tail = operationTail
      await tail
    } while (tail !== operationTail)
  }

  const canUndo = computed(() => view.value?.canUndo ?? false)
  const canRedo = computed(() => view.value?.canRedo ?? false)

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
    const page = view.value?.pages.find((page) => page.id === activePageId.value)
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
    if (failure.value && status.value === failure.value.message) status.value = 'Ready'
    failure.value = null
  }

  // Keep the last complete frame while the next is computed. Publish geometry,
  // properties and pixels together; a failed render must still expose the new view.
  async function refresh(reuseDrawing = false): Promise<void> {
    selectionRequest += 1
    const next = await bridge.getView()
    const page = next.pages.some((page) => page.id === activePageId.value)
      ? activePageId.value : next.pages[0]?.id ?? null
    if (reuseDrawing && next.revision === revision.value && page === layout.value?.page) {
      view.value = next
      return
    }
    let nextLayout: LayoutView | null = null
    let nextPreview: string | null = null
    try {
      if (page) {
        const options = pageOptions(next, page)
        nextLayout = await bridge.layout(page, options)
        nextPreview = await makePreview(page, next.revision, options)
      }
    } finally {
      const previousPreview = previewUrl.value
      view.value = next
      activePageId.value = page
      layout.value = nextLayout
      previewUrl.value = disposed ? null : nextPreview
      if (selectedId.value && !nodesById.value.has(selectedId.value)) selectedId.value = null
      await nextTick()
      if (previousPreview) URL.revokeObjectURL(previousPreview)
      if (disposed && nextPreview) URL.revokeObjectURL(nextPreview)
    }
  }

  async function makePreview(page: string, expectedRevision: number, options: PreviewOptions): Promise<string | null> {
    if (!bridge.rasterPreview || disposed) return null
    const preview = await bridge.preview(page, options)
    if (disposed || preview.revision !== expectedRevision) return null
    const url = URL.createObjectURL(new Blob([new Uint8Array(preview.png)], { type: 'image/png' }))
    try {
      const image = new Image()
      image.src = url
      // Decode before replacing the old image to avoid a blank frame between loads.
      if (image.decode) await image.decode()
      return url
    } catch (cause) {
      URL.revokeObjectURL(url)
      throw cause
    }
  }

  function selectPage(id: string): Promise<void> {
    if (confirmation.value) return Promise.resolve()
    return enqueue(async () => {
      if (id === activePageId.value || !view.value?.pages.some((page) => page.id === id)) return
      activePageId.value = id
      select(null)
      layout.value = null
      clearPreview()
      try {
        await refresh()
        clearFailure()
      } catch (cause) { report(cause, 'the page could not be laid out') }
    })
  }

  function load(): Promise<void> {
    if (confirmation.value) return Promise.resolve()
    return enqueue(() => loadNow())
  }

  async function loadNow(): Promise<void> {
    try {
      await loadCapabilities()
      await refresh()
      status.value = `Opened ${view.value?.displayName ?? 'document'}`
    } catch (cause) {
      report(cause, 'the document could not be opened')
    }
  }

  function edit(commands: EditCommands): Promise<void> {
    if (confirmation.value) return Promise.resolve()
    mutationSequence += 1
    return enqueue(() => editNow(commands))
  }

  async function editNow(commands: EditCommands): Promise<void> {
    if (commands.length === 0) return
    try {
      selectionRequest += 1
      await bridge.apply({ expectedRevision: revision.value, commands })
      await refresh()
      clearFailure()
      const action = commands.length === 1 ? commands[0]?.kind : null
      status.value = action === 'set-node-position' ? 'Layer moved' : action === 'set-node-size' ? 'Layer resized' : action === 'rename-node' ? 'Layer renamed' : action === 'set-node-fill' ? 'Fill updated' : action === 'set-node-text' ? 'Text updated' : 'Changes applied'
    } catch (cause) {
      await reportMutationFailure(cause, 'the edit was refused')
    }
  }

  async function reportMutationFailure(cause: unknown, fallback: string): Promise<void> {
    if ((cause as { code?: unknown } | null)?.code === 'revision-conflict') {
      try {
        // Refresh for the next action, without replaying an edit against a document
        // the person has not seen. Retrying a deletion here could destroy new work.
        await refresh()
        report({ code: 'revision-conflict', message: 'The document changed elsewhere. Review the latest version and try again.' }, fallback)
        return
      } catch { /* Keep the original refusal if the service is still unavailable. */ }
    }
    report(cause, fallback)
  }

  async function selectAt(pageX: number, pageY: number): Promise<boolean> {
    const page = activePageId.value
    if (!page || busy.value || confirmation.value || disposed) return false
    const request = ++selectionRequest
    const expectedRevision = revision.value
    try {
      const hit = await bridge.hitTest(page, pageX, pageY, pageOptions(view.value, page))
      if (disposed || request !== selectionRequest || hit.revision !== expectedRevision || activePageId.value !== page) return false
      selectedId.value = hit.node
      const name = hit.node ? nodesById.value.get(hit.node)?.name || hit.node : ''
      status.value = hit.node ? `Selected ${name}` : 'Nothing selected'
      return true
    } catch (cause) {
      if (!disposed && request === selectionRequest && activePageId.value === page && revision.value === expectedRevision) {
        report(cause, 'the hit test failed')
      }
      return false
    }
  }

  function select(id: string | null): void {
    selectionRequest += 1
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

  function undo(): Promise<void> {
    if (confirmation.value) return Promise.resolve()
    mutationSequence += 1
    return enqueue(() => undoNow())
  }

  async function undoNow(): Promise<void> {
    if (view.value?.canUndo === false) return
    try {
      await bridge.undo(revision.value)
      await refresh()
      clearFailure()
      status.value = 'Undo'
    } catch (cause) {
      await reportMutationFailure(cause, 'there is nothing to undo')
    }
  }

  function redo(): Promise<void> {
    if (confirmation.value) return Promise.resolve()
    mutationSequence += 1
    return enqueue(() => redoNow())
  }

  async function redoNow(): Promise<void> {
    if (view.value?.canRedo === false) return
    try {
      await bridge.redo(revision.value)
      await refresh()
      clearFailure()
      status.value = 'Redo'
    } catch (cause) {
      await reportMutationFailure(cause, 'there is nothing to redo')
    }
  }

  function save(): Promise<void> {
    if (!activePageId.value) return Promise.resolve()
    if (confirmation.value) return Promise.resolve()
    if (pendingSave?.sequence === mutationSequence) return pendingSave.promise
    const promise = enqueue(() => saveNow())
    const entry = { sequence: mutationSequence, promise }
    pendingSave = entry
    void promise.finally(() => { if (pendingSave === entry) pendingSave = null })
    return promise
  }

  async function saveNow(): Promise<void> {
    try {
      // A host that can show a dialog owns the write, so it decides the
      // destination and reports back. In a browser, download the full JSON
      // before recording the saved state.
      if (bridge.saveDocument) {
        const written = await bridge.saveDocument()
        if (written === null) {
          // Cancelling is an answer, not a failure: reporting it as an error
          // would train a person to ignore errors.
          status.value = 'Save cancelled'
          return
        }
        await refresh(true)
        clearFailure()
        status.value = `Saved ${written}`
        return
      }
      const bytes = await bridge.exportBytes()
      const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'application/json' }))
      try {
        const link = document.createElement('a')
        link.href = url
        link.download = view.value?.displayName?.endsWith('.json') ? view.value.displayName : 'design.json'
        document.body.append(link)
        link.click()
        link.remove()
      } finally { window.setTimeout(() => URL.revokeObjectURL(url), 1000) }
      await bridge.noteSaved()
      await refresh(true)
      clearFailure()
      status.value = `Downloaded ${bytes.byteLength} bytes`
    } catch (cause) {
      report(cause, 'the document could not be saved')
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
    return activePageId.value
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
    const offset = 24 + ((view.value?.pages.find((entry) => entry.id === page)?.roots.length ?? 0) % 8) * 20
    await edit([
      {
        kind: 'create-node',
        node: id,
        nodeKind: kind,
        name: name ?? `${kind[0]?.toUpperCase()}${kind.slice(1)}`,
        // Left unset: the service supplies the kind's default, including a
        // registered font for text, which is knowledge this layer does not have.
        fill: null,
        parent: { in: 'page-root', page },
      },
      { kind: 'set-node-position', node: id, position: [offset, offset] },
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
    if (disposed || confirmation.value || pendingReplacement) return
    if (busy.value) {
      pendingReplacement = true
      void whenIdle().then(() => {
        pendingReplacement = false
        guardUnsaved(message, confirmLabel, action)
      })
      return
    }
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
    void enqueue(async () => {
      try {
        const opened = await openSampleFn()
        selectedId.value = null
        await refresh()
        clearFailure()
        status.value = `Opened ${opened.displayName ?? 'the sample'}`
      } catch (cause) {
        report(cause, 'the sample could not be opened')
      }
    })
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
    void enqueue(async () => {
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
      }
    })
  }

  function open(): void {
    guardUnsaved(
      'Opening a document replaces the current one and discards its undo history.',
      'Discard and open',
      openInternal,
    )
  }

  function newDocument(): void {
    if (!bridge.newDocument) return
    guardUnsaved('Creating a document replaces the current document and discards its undo history.', 'Discard and create', () => enqueue(async () => {
      try {
        await bridge.newDocument!()
        select(null)
        activePageId.value = null
        await refresh()
        clearFailure()
        status.value = 'New document — add a frame, shape or text'
      } catch (cause) { report(cause, 'the document could not be created') }
    }))
  }

  function setZoom(next: number): void {
    if (!Number.isFinite(next)) return
    zoom.value = Math.min(64, Math.max(0.02, next))
  }

  function nudge(dx: number, dy: number): Promise<void> {
    const id = selectedId.value
    if (!id || confirmation.value) return Promise.resolve()
    mutationSequence += 1
    return enqueue(async () => {
      const node = nodesById.value.get(id)
      if (!node || node.props.positionIsLayoutDecided) return
      const parent = node.parent ? layout.value?.nodes.find((entry) => entry.id === node.parent) : null
      let localX = dx
      let localY = dy
      if (parent) {
        const [a, b, c, d] = parent.world
        const determinant = a * d - b * c
        if (Math.abs(determinant) < 1e-10) {
          status.value = 'This layer cannot move independently. Try moving its parent.'
          return
        }
        localX = (d * dx - c * dy) / determinant
        localY = (-b * dx + a * dy) / determinant
      }
      await editNow([{ kind: 'set-node-position', node: id,
        position: [node.props.transform[4] + localX, node.props.transform[5] + localY] }])
    })
  }

  async function setText(content: string, fontSize: number): Promise<void> {
    const node = selectedId.value
    if (node) await edit([{ kind: 'set-node-text', node, content, fontSize }])
  }

  return {
    previewUrl, dispose, whenIdle, nudge,
    activePageId, selectPage, canUndo, canRedo, setText, newDocument, canNew: bridge.newDocument !== undefined,
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

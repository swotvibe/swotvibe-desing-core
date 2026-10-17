/** Browser preview adapter. Desktop document layout and rendering live in Rust.
 * The preview keeps transactional edits and history, and exports schema-v2 JSON.
 * Browser text measurement remains approximate; it is not a replacement for the host.
 */

import type {
  Capabilities,
  CommitSummary,
  DocumentView,
  EditRequest,
  LayoutNodeView,
  LayoutView,
  NodeView,
  Preview,
  PreviewOptions,
  PropsView,
  Rgba,
} from './types'

const PAGE_ID = '018f0000-0000-7000-8000-000000000022'

/**
 * What a newly created node of each kind looks like.
 *
 * These mirror the real service's tool defaults, and they matter: the stand-in
 * previously invented its own (80x80, always filled), which is more generous
 * than the service, so a browser test passed while the desktop build created
 * invisible zero-sized nodes. A stand-in that is kinder than what it stands in
 * for hides the defect it exists to catch.
 */
const CREATION_DEFAULTS: Record<
  string,
  { size: [number, number]; fill: Rgba | null; text?: string }
> = {
  frame: { size: [120, 120], fill: { r: 217, g: 217, b: 217, a: 255 } },
  group: { size: [120, 120], fill: { r: 217, g: 217, b: 217, a: 255 } },
  shape: { size: [120, 120], fill: { r: 217, g: 217, b: 217, a: 255 } },
  text: { size: [0, 0], fill: { r: 26, g: 26, b: 26, a: 255 }, text: 'Text' },
  image: { size: [120, 120], fill: null },
}

/** The page the sample service will answer for. */
export const SAMPLE_PAGE_ID = PAGE_ID

function props(overrides: Partial<PropsView>): PropsView {
  return {
    size: [0, 0],
    transform: [1, 0, 0, 1, 0, 0],
    widthSizing: 'fixed',
    heightSizing: 'fixed',
    positionIsLayoutDecided: false,
    fill: null,
    stroke: null,
    shapeGeometry: null,
    cornerRadius: null,
    textContent: null,
    fontFamily: null,
    fontSize: null,
    fontWeight: null,
    textAlign: null,
    textDirection: null,
    imageAsset: null,
    frameLayout: null,
    ...overrides,
  }
}

/**
 * The sample's nodes, in paint order, with their committed geometry.
 *
 * These mirror `tests/fixtures/m0-sample-v2.json`, including the detail that the
 * Card is a **flex** frame: its children's positions come from layout, so they
 * are marked `positionIsLayoutDecided` and a position edit on them is refused —
 * exactly as the real service refuses it. A stand-in that is more permissive than
 * the service hides the defects it exists to catch.
 */
interface SampleNode {
  id: string
  kind: string
  name: string
  parent: string | null
  children: string[]
  props: PropsView
  rect: [number, number, number, number]
}

function sampleNodes(): SampleNode[] {
  const card = '018f0000-0000-7000-8000-000000020001'
  return [
    {
      id: card,
      kind: 'frame',
      name: 'Card',
      parent: null,
      children: [
        '018f0000-0000-7000-8000-000000020002',
        '018f0000-0000-7000-8000-000000020003',
        '018f0000-0000-7000-8000-000000020004',
        '018f0000-0000-7000-8000-000000020005',
      ],
      props: props({
        size: [400, 240],
        transform: [1, 0, 0, 1, 40, 40],
        fill: { r: 255, g: 255, b: 255, a: 255 },
        // Mirrors the fixture: the Card is a flex column, so its children are
        // placed by layout.
        frameLayout: 'flex',
      }),
      rect: [40, 40, 400, 240],
    },
    {
      id: '018f0000-0000-7000-8000-000000020002',
      kind: 'shape',
      name: 'Header',
      parent: card,
      children: [],
      props: props({
        size: [368, 48],
        fill: { r: 76, g: 110, b: 245, a: 255 },
        shapeGeometry: 'rect',
        cornerRadius: 8,
        positionIsLayoutDecided: true,
      }),
      rect: [56, 56, 368, 48],
    },
    {
      id: '018f0000-0000-7000-8000-000000020003',
      kind: 'text',
      name: 'Latin',
      parent: card,
      children: [],
      props: props({
        size: [186.08203125, 29.0390625],
        fill: { r: 26, g: 26, b: 26, a: 255 },
        textContent: 'Swotvibe Design',
        fontFamily: 'Inter',
        fontSize: 24,
        fontWeight: 400,
        textAlign: 'start',
        textDirection: 'auto',
        positionIsLayoutDecided: true,
        widthSizing: 'hug',
        heightSizing: 'hug',
      }),
      rect: [56, 116, 186.08203125, 29.0390625],
    },
    {
      id: '018f0000-0000-7000-8000-000000020004',
      kind: 'text',
      name: 'Arabic',
      parent: card,
      children: [],
      props: props({
        size: [96.359992980957, 42.2399978637695],
        fill: { r: 26, g: 26, b: 26, a: 255 },
        textContent: 'محرر تصميم',
        fontFamily: 'Noto Sans Arabic',
        fontSize: 24,
        fontWeight: 400,
        textAlign: 'start',
        textDirection: 'auto',
        positionIsLayoutDecided: true,
        widthSizing: 'hug',
        heightSizing: 'hug',
      }),
      rect: [56, 157.0390625, 96.359992980957, 42.2399978637695],
    },
    {
      id: '018f0000-0000-7000-8000-000000020005',
      kind: 'shape',
      name: 'Dot',
      parent: card,
      children: [],
      props: props({
        size: [32, 32],
        fill: { r: 255, g: 107, b: 107, a: 255 },
        shapeGeometry: 'ellipse',
        cornerRadius: 0,
        positionIsLayoutDecided: true,
      }),
      rect: [56, 211.279052734375, 32, 32],
    },
  ]
}

/**
 * A sample service.
 *
 * Revisions advance on every committed batch and are refused when the caller's
 * expectation does not match, which is the behaviour the interface must handle
 * rather than assume away.
 */
export class SampleEditorBridge {
  private nodes = sampleNodes()
  private hasPage = true
  private past: string[] = []
  private future: string[] = []
  private revision = 1
  private pristine = JSON.stringify(this.nodes)
  private dirty = false

  async capabilities(): Promise<Capabilities> {
    return {
      editableCommands: ['create-node', 'delete-node', 'move-node', 'rename-node', 'set-node-fill',
        'set-node-position', 'set-node-size', 'set-node-corner-radius', 'set-node-text',
        'set-node-text-style', 'set-node-image-asset'],
      textDirections: ['auto'],
      textAlignments: ['start', 'center', 'end'],
      fontWeightRange: [1, 1000],
      creatableKinds: ['frame', 'group', 'shape', 'text', 'image'],
      fontFamilies: ['Inter', 'Noto Sans Arabic'],
      renderer: 'sample service (the browser build draws no pixels)',
      layoutEngine: 'sample service (geometry is the committed M0 reference)',
    }
  }

  async getView(): Promise<DocumentView> {
    const nodes: NodeView[] = this.nodes.map((node) => ({
      id: node.id,
      kind: node.kind,
      name: node.name,
      page: PAGE_ID,
      parent: node.parent,
      children: [...node.children],
      props: structuredClone(node.props),
    }))
    // An open page survives deletion of its last layer.
    const roots = this.nodes
      .filter((node) => node.parent === null)
      .map((node) => node.id)
    return {
      revision: this.revision,
      dirty: this.dirty,
      displayName: this.hasPage ? 'Preview' : null,
      canUndo: this.past.length > 0,
      canRedo: this.future.length > 0,
      pages: this.hasPage ? [{ id: PAGE_ID, name: 'Preview', roots }] : [],
      nodes,
    }
  }

  async apply(request: EditRequest): Promise<CommitSummary> {
    if (request.expectedRevision !== this.revision) {
      const error = new Error(
        `the request expected revision ${request.expectedRevision}, but the document is at ${this.revision}`,
      )
      Object.assign(error, {
        code: 'revision-conflict',
        revision: this.revision,
      })
      throw error
    }

    const before = JSON.stringify(this.nodes)
    try {
      return await this.applyBatch(request, before)
    } catch (cause) {
      this.nodes = JSON.parse(before) as SampleNode[]
      throw cause
    }
  }

  private async applyBatch(request: EditRequest, before: string): Promise<CommitSummary> {
    const added: string[] = []
    const removed: string[] = []
    const changed: string[] = []

    for (const command of request.commands) {
      if (command.kind === 'create-node') {
        if (this.nodes.some((candidate) => candidate.id === command.node)) {
          const error = new Error('the identity is already taken')
          Object.assign(error, { code: 'command-rejected', node: command.node })
          throw error
        }
        const parentId = command.parent.in === 'child' ? command.parent.parent : null
        if (parentId && !this.nodes.some((candidate) => candidate.id === parentId)) {
          const error = new Error('the parent node is not in the document')
          Object.assign(error, { code: 'unknown-node', node: parentId })
          throw error
        }
        const defaults = CREATION_DEFAULTS[command.nodeKind] ?? {
          size: [120, 120] as [number, number],
          fill: { r: 217, g: 217, b: 217, a: 255 } as Rgba,
        }
        this.nodes.push({
          id: command.node,
          kind: command.nodeKind,
          name: command.name ?? '',
          parent: parentId,
          children: [],
          props: props({
            size: [...defaults.size],
            widthSizing: command.nodeKind === 'text' ? 'hug' : 'fixed',
            heightSizing: command.nodeKind === 'text' ? 'hug' : 'fixed',
            fill: command.fill ?? defaults.fill,
            shapeGeometry: command.nodeKind === 'shape' ? 'rect' : null,
            cornerRadius: command.nodeKind === 'shape' ? 0 : null,
            textContent: defaults.text ?? null,
            fontFamily: command.nodeKind === 'text' ? 'Inter' : null,
            fontSize: command.nodeKind === 'text' ? 16 : null,
            fontWeight: command.nodeKind === 'text' ? 400 : null,
            textAlign: command.nodeKind === 'text' ? 'start' : null,
            textDirection: command.nodeKind === 'text' ? 'auto' : null,
          }),
          rect: [0, 0, defaults.size[0], defaults.size[1]],
        })
        if (parentId) {
          const parent = this.nodes.find((candidate) => candidate.id === parentId)
          parent?.children.push(command.node)
        }
        added.push(command.node)
        continue
      }

      const node = this.nodes.find((candidate) => candidate.id === command.node)
      if (!node) {
        const error = new Error('the node is not in the document')
        Object.assign(error, { code: 'unknown-node', node: command.node })
        throw error
      }

      switch (command.kind) {
        case 'delete-node': {
          // A subtree delete, like the kernel's: the node and everything under
          // it, and the parent stops naming it.
          const doomed = this.collectSubtree(command.node)
          if (node.parent) {
            const parent = this.nodes.find((candidate) => candidate.id === node.parent)
            if (parent) {
              parent.children = parent.children.filter((id) => id !== command.node)
            }
          }
          this.nodes = this.nodes.filter((candidate) => !doomed.has(candidate.id))
          removed.push(...doomed)
          break
        }
        case 'move-node': {
          const parentId = command.parent.in === 'child' ? command.parent.parent : null
          if (node.parent) {
            const previous = this.nodes.find((candidate) => candidate.id === node.parent)
            if (previous) {
              previous.children = previous.children.filter((id) => id !== command.node)
            }
          }
          node.parent = parentId
          if (parentId) {
            const parent = this.nodes.find((candidate) => candidate.id === parentId)
            parent?.children.push(command.node)
          }
          changed.push(node.id)
          break
        }
        case 'rename-node':
          node.name = command.name ?? ''
          changed.push(node.id)
          break
        case 'set-node-fill':
          node.props = { ...node.props, fill: command.fill }
          changed.push(node.id)
          break
        case 'set-node-position': {
          // The real service refuses this when the parent lays the node out;
          // the stand-in does the same, so a browser test sees the real rule.
          if (node.props.positionIsLayoutDecided) {
            const error = new Error(
              "the parent lays out its children, so this node's position is not its own to set",
            )
            Object.assign(error, { code: 'command-rejected', node: node.id })
            throw error
          }
          const [x, y] = command.position
          if (!Number.isFinite(x) || !Number.isFinite(y)) throw new Error('Position must be finite')
          const dx = x - node.props.transform[4]
          const dy = y - node.props.transform[5]
          for (const child of this.nodes) {
            if (child.id !== node.id && this.collectSubtree(node.id).has(child.id)) {
              child.rect = [child.rect[0] + dx, child.rect[1] + dy, child.rect[2], child.rect[3]]
            }
          }
          node.props = {
            ...node.props,
            transform: [
              node.props.transform[0] ?? 1,
              node.props.transform[1] ?? 0,
              node.props.transform[2] ?? 0,
              node.props.transform[3] ?? 1,
              x,
              y,
            ],
          }
          node.rect = [x, y, node.rect[2], node.rect[3]]
          changed.push(node.id)
          break
        }
        case 'set-node-size': {
          const [width, height] = command.size
          if (!Number.isFinite(width) || !Number.isFinite(height) || width < 0 || height < 0) {
            const error = new Error(`the size [${width}, ${height}] is not usable`)
            Object.assign(error, { code: 'invalid-request', node: node.id })
            throw error
          }
          node.props = { ...node.props, size: [width, height], widthSizing: 'fixed', heightSizing: 'fixed' }
          node.rect = [node.rect[0], node.rect[1], width, height]
          changed.push(node.id)
          break
        }
        case 'set-node-text': {
          if (node.kind !== 'text' || !Number.isFinite(command.fontSize) || command.fontSize <= 0) {
            throw new Error('Text requires a text node and a positive font size')
          }
          node.props = { ...node.props, textContent: command.content, fontSize: command.fontSize }
          changed.push(node.id)
          break
        }
        case 'set-node-text-style': {
          const refuse = (code: 'invalid-request' | 'command-rejected', message: string): never => {
            throw Object.assign(new Error(message), { code, node: node.id })
          }
          if (node.kind !== 'text') refuse('command-rejected', 'only a text node has a text style')
          if (!Number.isInteger(command.fontWeight) || command.fontWeight < 1 || command.fontWeight > 1000) {
            refuse('invalid-request', 'font weight must be in 1..=1000')
          }
          if (!['Inter', 'Noto Sans Arabic'].includes(command.fontFamily)) refuse('invalid-request', 'the font family is not registered')
          if (!['start', 'center', 'end'].includes(command.align)) refuse('invalid-request', 'text alignment must be start, center or end')
          if (!['auto', 'ltr', 'rtl'].includes(command.direction)) refuse('invalid-request', 'text direction must be auto, ltr or rtl')
          if (command.direction !== 'auto') refuse('command-rejected', 'forced paragraph direction is unsupported')
          node.props = { ...node.props, fontFamily: command.fontFamily, fontWeight: command.fontWeight,
            textAlign: command.align, textDirection: command.direction }
          changed.push(node.id)
          break
        }
        case 'set-node-image-asset': {
          if (node.kind !== 'image') throw Object.assign(new Error('only an image node has an image asset'), { code: 'command-rejected', node: node.id })
          if (!Object.hasOwn(command, 'asset') || (command.asset !== null && typeof command.asset !== 'string')) {
            throw Object.assign(new Error('asset is required; use null to clear it'), { code: 'invalid-request', node: node.id })
          }
          if (command.asset !== null) {
            const validId = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(command.asset)
            throw Object.assign(new Error(validId ? 'the asset is not registered in this document' : 'invalid asset identity'),
              { code: validId ? 'command-rejected' : 'invalid-request', node: node.id })
          }
          // The browser sample has no asset registry or payloads. Native binding
          // is tested in Rust; this adapter can clear bindings and rejects missing assets.
          node.props = { ...node.props, imageAsset: null }
          changed.push(node.id)
          break
        }
        case 'set-node-corner-radius': {
          if (!Number.isFinite(command.radius) || command.radius < 0) throw new Error('Radius must be non-negative')
          if (node.props.shapeGeometry === null) {
            const error = new Error('only a shape node has a corner radius')
            Object.assign(error, { code: 'command-rejected', node: node.id })
            throw error
          }
          node.props = { ...node.props, cornerRadius: command.radius }
          changed.push(node.id)
          break
        }
      }
    }

    this.past.push(before)
    this.future = []
    this.measureText()
    const previous = this.revision
    this.revision += 1
    this.dirty = JSON.stringify(this.nodes) !== this.pristine
    return {
      previousRevision: previous,
      revision: this.revision,
      dirty: this.dirty,
      canUndo: this.past.length > 0,
      canRedo: false,
      addedNodes: added,
      removedNodes: removed,
      changedNodes: changed,
      affectedPages: [PAGE_ID],
    }
  }

  /** A node and every node beneath it. */
  private collectSubtree(root: string): Set<string> {
    const found = new Set<string>([root])
    let grew = true
    while (grew) {
      grew = false
      for (const node of this.nodes) {
        if (node.parent && found.has(node.parent) && !found.has(node.id)) {
          found.add(node.id)
          grew = true
        }
      }
    }
    return found
  }

  private restore(expectedRevision: number, from: string[], to: string[]): CommitSummary {
    if (expectedRevision !== this.revision) throw Object.assign(new Error('Stale revision'), { code: 'revision-conflict' })
    const snapshot = from.pop()
    if (snapshot === undefined) throw Object.assign(new Error('There is no history step available'), { code: 'history' })
    to.push(JSON.stringify(this.nodes))
    this.nodes = JSON.parse(snapshot) as SampleNode[]
    const previousRevision = this.revision++
    this.dirty = JSON.stringify(this.nodes) !== this.pristine
    return { previousRevision, revision: this.revision, dirty: this.dirty,
      canUndo: this.past.length > 0, canRedo: this.future.length > 0,
      addedNodes: [], removedNodes: [], changedNodes: this.nodes.map((node) => node.id), affectedPages: [PAGE_ID] }
  }

  async undo(expectedRevision: number): Promise<CommitSummary> {
    return this.restore(expectedRevision, this.past, this.future)
  }

  async redo(expectedRevision: number): Promise<CommitSummary> {
    return this.restore(expectedRevision, this.future, this.past)
  }

  // Browser preview only: desktop typography is measured by the Rust engine.
  private measureText(): void {
    for (const node of this.nodes) {
      if (node.kind !== 'text') continue
      const size = node.props.fontSize ?? 16
      const lines = (node.props.textContent ?? '').split('\n')
      if (node.props.widthSizing === 'hug') node.rect[2] = Math.max(...lines.map((line) => line.length)) * size * 0.6
      if (node.props.heightSizing === 'hug') node.rect[3] = lines.length * size * 1.4
    }
  }

  async layout(page: string, options: PreviewOptions): Promise<LayoutView> {
    if (page !== PAGE_ID) {
      const error = new Error('the page is not in the document')
      Object.assign(error, { code: 'unknown-page', page })
      throw error
    }
    const nodes: LayoutNodeView[] = this.nodes.map((node) => ({
      id: node.id,
      rect: node.rect,
      size: [node.rect[2], node.rect[3]],
      world: [1, 0, 0, 1, node.rect[0], node.rect[1]],
    }))
    return {
      revision: this.revision,
      page,
      pageSize: this.pageSize(options),
      nodes,
      diagnostics: [],
    }
  }

  /**
   * The page the service would answer with.
   *
   * `max(requested, contentBounds)` componentwise, which is what the service
   * does: the request is a floor, and content outside it grows the page rather
   * than falling off it. Returning the requested size unconditionally — as this
   * stand-in used to — agreed with a canvas that drew the same constant, so the
   * two hid the difference from each other instead of exposing it.
   */
  private pageSize(options: PreviewOptions): [number, number] {
    const [requestedWidth, requestedHeight] = options.pageSize ?? [480, 320]
    let width = requestedWidth
    let height = requestedHeight
    for (const node of this.nodes) {
      width = Math.max(width, node.rect[0] + node.rect[2])
      height = Math.max(height, node.rect[1] + node.rect[3])
    }
    return [width, height]
  }

  async preview(page: string, options: PreviewOptions): Promise<Preview> {
    const layout = await this.layout(page, options)
    return {
      revision: this.revision,
      page,
      pageSize: layout.pageSize,
      width: Math.round(layout.pageSize[0] * options.scale),
      height: Math.round(layout.pageSize[1] * options.scale),
      scale: options.scale,
      // The sample service renders nothing: the canvas draws the nodes as DOM,
      // so there are no pixels to hand back yet.
      png: new Uint8Array(),
      layoutDiagnostics: [],
      renderDiagnostics: [],
    }
  }

  async hitTest(page: string, x: number, y: number, options: PreviewOptions) {
    const layout = await this.layout(page, options)
    const hit = [...layout.nodes]
      .reverse()
      .find((node) => {
        const [left, top, width, height] = node.rect
        return x >= left && x <= left + width && y >= top && y <= top + height
      })
    return { node: hit?.id ?? null, revision: this.revision }
  }

  async nodeProps(node: string): Promise<PropsView> {
    const found = this.nodes.find((candidate) => candidate.id === node)
    if (!found) {
      const error = new Error('the node is not in the document')
      Object.assign(error, { code: 'unknown-node', node })
      throw error
    }
    return found.props
  }

  async exportBytes(): Promise<Uint8Array> {
    const rgba = (color: Rgba | null) => color ? [color.r, color.g, color.b, color.a] : null
    const payload = {
      schema_version: 2, id: '018f0000-0000-7000-8000-000000000021',
      pages: this.hasPage ? [{ id: PAGE_ID, name: 'Page 1', roots: this.nodes.filter((n) => n.parent === null).map((n) => n.id) }] : [],
      assets: [],
      nodes: this.nodes.map((node) => {
        const p = node.props
        return {
          id: node.id, kind: node.kind, name: node.name, children: node.children,
          props: {
            transform: p.transform, size: p.size, width: p.widthSizing, height: p.heightSizing,
            fill: rgba(p.fill),
            ...(p.stroke ? { stroke: { color: rgba(p.stroke.color), width: p.stroke.width } } : {}),
            ...(node.kind === 'frame' ? { frame: p.frameLayout === 'flex'
              ? { layout: 'flex', direction: 'column', gap: 12, padding: { top: 16, right: 16, bottom: 16, left: 16 }, main_align: 'start', cross_align: 'start' }
              : { layout: 'none' } } : {}),
            ...(node.kind === 'shape' ? { shape: { geometry: p.shapeGeometry ?? 'rect', corner_radius: p.cornerRadius ?? 0 } } : {}),
            ...(node.kind === 'text' ? { text: { content: p.textContent ?? '', font_family: p.fontFamily ?? 'Inter', font_size: p.fontSize ?? 16, font_weight: p.fontWeight ?? 400, direction: p.textDirection ?? 'auto', align: p.textAlign ?? 'start' } } : {}),
            ...(node.kind === 'image' ? { image: { asset: p.imageAsset } } : {}),
          },
        }
      }),
    }
    return new TextEncoder().encode(JSON.stringify(payload, null, 2))
  }

  async noteSaved(): Promise<void> {
    this.pristine = JSON.stringify(this.nodes)
    this.dirty = false
  }

  /**
   * Reloads the sample this service was seeded with.
   *
   * The browser build has no file to open, so this is the honest equivalent: it
   * restores the committed sample rather than pretending to read a document.
   */
  async newDocument(): Promise<DocumentView> {
    this.empty()
    this.hasPage = true
    return this.getView()
  }

  async openSample(): Promise<DocumentView> {
    this.hasPage = true
    this.past = []
    this.future = []
    this.nodes = sampleNodes()
    this.revision += 1
    this.dirty = false
    this.pristine = JSON.stringify(this.nodes)
    return this.getView()
  }

  async openBytes(): Promise<DocumentView> {
    const error = new Error('this build has no file reader; the desktop host supplies one')
    Object.assign(error, { code: 'document-read' })
    throw error
  }

  /** The page the sample service will answer for. */
  get pageId(): string {
    return PAGE_ID
  }

  /**
   * Empties the document, so the shell's first-run path can be tested.
   *
   * The desktop host starts with no document at all; the browser build starts
   * with the sample so the interface can be reviewed. This is how a test reaches
   * the state a real first run is in.
   */
  empty(): void {
    this.hasPage = false
    this.past = []
    this.future = []
    this.nodes = []
    this.revision += 1
    this.pristine = JSON.stringify(this.nodes)
    this.dirty = false
  }
}

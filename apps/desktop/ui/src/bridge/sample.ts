/**
 * A deterministic sample document for the interface.
 *
 * This is a stand-in for the application service, not a second implementation of
 * it. It exists so the editor shell can be built, run, and reviewed in a browser
 * before the desktop host lands, and so the components have something real to
 * render.
 *
 * What it deliberately does **not** do: it never decides what an edit means. It
 * stores properties, refuses a stale revision, and answers a hit test — the same
 * three behaviours a host relies on — and nothing else.
 *
 * The numbers come from `tests/fixtures/m0-sample-v2.json` and
 * `tests/golden/m0-sample.layout.json`, so the shell is laid out against the
 * committed M0 sample rather than invented geometry.
 */

import type {
  CommitSummary,
  DocumentView,
  LayoutNodeView,
  LayoutView,
  NodeView,
  Preview,
  PreviewOptions,
  PropsView,
  Rgba,
} from './types'

const PAGE_ID = '018f0000-0000-7000-8000-000000000022'
const PAGE_SIZE: [number, number] = [480, 320]

function props(overrides: Partial<PropsView>): PropsView {
  return {
    size: [0, 0],
    transform: [1, 0, 0, 1, 0, 0],
    widthSizing: 'fixed',
    heightSizing: 'fixed',
    fill: null,
    stroke: null,
    shapeGeometry: null,
    cornerRadius: null,
    textContent: null,
    fontFamily: null,
    fontSize: null,
    textDirection: null,
    imageAsset: null,
    frameLayout: null,
    ...overrides,
  }
}

/** The sample's nodes, in paint order, with their committed geometry. */
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
        frameLayout: 'none',
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
        textDirection: 'auto',
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
        textDirection: 'rtl',
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
  private revision = 1
  private pristine = JSON.stringify(this.nodes)
  private dirty = false

  async getView(): Promise<DocumentView> {
    const nodes: NodeView[] = this.nodes.map((node) => ({
      id: node.id,
      kind: node.kind,
      name: node.name,
      page: PAGE_ID,
      parent: node.parent,
      children: node.children,
      props: node.props,
    }))
    // A document with no roots has no page to show, which is what an empty
    // document is. Reporting a page with no children would be a second meaning
    // for "empty".
    const roots = this.nodes
      .filter((node) => node.parent === null)
      .map((node) => node.id)
    return {
      revision: this.revision,
      dirty: this.dirty,
      displayName: roots.length > 0 ? 'Preview' : null,
      pages: roots.length > 0 ? [{ id: PAGE_ID, name: 'Preview', roots }] : [],
      nodes,
    }
  }

  async apply(request: {
    expectedRevision: number
    commands: ReadonlyArray<
      | { kind: 'rename-node'; node: string; name: string | null }
      | { kind: 'set-node-fill'; node: string; fill: Rgba | null }
    >
  }): Promise<CommitSummary> {
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

    const changed: string[] = []
    for (const command of request.commands) {
      const node = this.nodes.find((candidate) => candidate.id === command.node)
      if (!node) {
        const error = new Error('the node is not in the document')
        Object.assign(error, { code: 'unknown-node', node: command.node })
        throw error
      }
      if (command.kind === 'rename-node') {
        node.name = command.name ?? ''
      } else {
        node.props = { ...node.props, fill: command.fill }
      }
      changed.push(node.id)
    }

    const previous = this.revision
    this.revision += 1
    this.dirty = JSON.stringify(this.nodes) !== this.pristine
    return {
      previousRevision: previous,
      revision: this.revision,
      dirty: this.dirty,
      canUndo: this.dirty,
      canRedo: false,
      addedNodes: [],
      removedNodes: [],
      changedNodes: changed,
      affectedPages: [PAGE_ID],
    }
  }

  async undo(): Promise<CommitSummary> {
    const error = new Error('there is nothing to undo')
    Object.assign(error, { code: 'history' })
    throw error
  }

  async redo(): Promise<CommitSummary> {
    const error = new Error('there is nothing to redo')
    Object.assign(error, { code: 'history' })
    throw error
  }

  async layout(page: string, _options: PreviewOptions): Promise<LayoutView> {
    if (page !== PAGE_ID) {
      const error = new Error('the page is not in the document')
      Object.assign(error, { code: 'unknown-page', page })
      throw error
    }
    const nodes: LayoutNodeView[] = this.nodes.map((node) => ({
      id: node.id,
      rect: node.rect,
      size: node.props.size,
      world: [1, 0, 0, 1, node.rect[0], node.rect[1]],
    }))
    return {
      revision: this.revision,
      page,
      pageSize: PAGE_SIZE,
      nodes,
      diagnostics: [],
    }
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
    const payload = JSON.stringify({ schema_version: 2, nodes: this.nodes.length })
    return new TextEncoder().encode(payload)
  }

  async noteSaved(): Promise<void> {
    this.pristine = JSON.stringify(this.nodes)
    this.dirty = false
  }

  /**
   * Saves to memory.
   *
   * A browser has no file system, so this stands in for a host's write: it
   * records the current state as saved and reports a name. `openDocument` is
   * deliberately absent — there is nothing to open, and pretending otherwise
   * would put a dialog that cannot appear behind a button that looks live.
   */
  async saveDocument(): Promise<string | null> {
    await this.noteSaved()
    return 'design.json'
  }

  /**
   * Reloads the sample this service was seeded with.
   *
   * The browser build has no file to open, so this is the honest equivalent: it
   * restores the committed sample rather than pretending to read a document.
   */
  async openSample(): Promise<DocumentView> {
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
    this.nodes = []
    this.revision += 1
    this.pristine = JSON.stringify(this.nodes)
    this.dirty = false
  }
}

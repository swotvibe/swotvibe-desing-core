/**
 * The interface's view of the application service.
 *
 * These types mirror the Rust DTOs in `crates/app/src/dto.rs`. They are written
 * once here and are the only shapes the interface may depend on: the kernel's
 * types and the persisted schema are different things, and neither should reach
 * a component.
 *
 * Two rules matter more than the exact fields:
 *
 * - **Identities are opaque strings.** Nothing in the interface may parse one,
 *   order by one, or assume it is a UUID.
 * - **`revision` is read from the service, never assumed.** Every mutation sends
 *   back the revision the interface last saw, so a stale view is refused instead
 *   of overwriting a change it never saw.
 */

export interface Rgba {
  r: number
  g: number
  b: number
  a: number
}

export interface StrokeView {
  color: Rgba
  width: number
}

export interface PropsView {
  size: [number, number]
  transform: [number, number, number, number, number, number]
  widthSizing: string
  heightSizing: string
  fill: Rgba | null
  stroke: StrokeView | null
  shapeGeometry: string | null
  cornerRadius: number | null
  textContent: string | null
  fontFamily: string | null
  fontSize: number | null
  textDirection: string | null
  imageAsset: string | null
  frameLayout: string | null
}

export interface NodeView {
  id: string
  kind: string
  name: string
  page: string
  parent: string | null
  children: string[]
  props: PropsView
}

export interface PageView {
  id: string
  name: string
  roots: string[]
}

export interface DocumentView {
  revision: number
  dirty: boolean
  displayName: string | null
  pages: PageView[]
  nodes: NodeView[]
}

export interface PreviewOptions {
  pageSize: [number, number] | null
  scale: number
  background: [number, number, number, number]
}

export interface Preview {
  revision: number
  page: string
  pageSize: [number, number]
  width: number
  height: number
  scale: number
  /** PNG bytes. Never a data URL for a real document. */
  png: Uint8Array
  layoutDiagnostics: string[]
  renderDiagnostics: string[]
}

export interface HitTestResult {
  node: string | null
  revision: number
}

/**
 * One node's resolved geometry, in page units.
 *
 * Derived, never stored: valid only for the revision it came with.
 */
export interface LayoutNodeView {
  id: string
  /** `[x, y, width, height]` in page units. */
  rect: [number, number, number, number]
  size: [number, number]
  /** The composed page-to-node transform, `[a, b, c, d, e, f]`. */
  world: [number, number, number, number, number, number]
}

export interface LayoutView {
  revision: number
  page: string
  pageSize: [number, number]
  /** In paint order, back to front. */
  nodes: LayoutNodeView[]
  diagnostics: string[]
}

export interface CommitSummary {
  previousRevision: number
  revision: number
  dirty: boolean
  canUndo: boolean
  canRedo: boolean
  addedNodes: string[]
  removedNodes: string[]
  changedNodes: string[]
  affectedPages: string[]
}

export type EditCommand =
  | { kind: 'rename-node'; node: string; name: string | null }
  | { kind: 'set-node-fill'; node: string; fill: Rgba | null }

export interface EditRequest {
  expectedRevision: number
  commands: EditCommand[]
}

/** The stable failure categories, matching `AppErrorCode` in Rust. */
export type AppErrorCode =
  | 'invalid-request'
  | 'unknown-node'
  | 'unknown-page'
  | 'schema'
  | 'validation'
  | 'command-rejected'
  | 'revision-conflict'
  | 'history'
  | 'layout'
  | 'render'
  | 'resource-limit'
  | 'document-read'
  | 'document-write'

export interface AppErrorShape {
  code: AppErrorCode
  message: string
  node?: string
  page?: string
  revision?: number
}

/**
 * A failure the interface can branch on.
 *
 * A plain `Error` would force every caller to match on a message, which is how
 * error handling turns into string comparison.
 */
export class AppError extends Error {
  readonly code: AppErrorCode
  readonly node: string | null
  readonly page: string | null
  readonly revision: number | null

  constructor(shape: AppErrorShape) {
    super(shape.message)
    this.name = 'AppError'
    this.code = shape.code
    this.node = shape.node ?? null
    this.page = shape.page ?? null
    this.revision = shape.revision ?? null
  }
}

/**
 * The operations the editor shell needs.
 *
 * The interface is written against this interface, not against Tauri, so the
 * browser build and the desktop build run the same components. A Tauri
 * implementation is added when the host lands; nothing in a component changes
 * when it does.
 */
export interface EditorBridge {
  /** The open document. */
  getView(): Promise<DocumentView>
  /** Applies one atomic batch at the revision the caller last saw. */
  apply(request: EditRequest): Promise<CommitSummary>
  undo(expectedRevision: number): Promise<CommitSummary>
  redo(expectedRevision: number): Promise<CommitSummary>
  /** Renders one page to PNG bytes. */
  preview(page: string, options: PreviewOptions): Promise<Preview>
  /** The resolved geometry of one page, for a selection overlay. */
  layout(page: string, options: PreviewOptions): Promise<LayoutView>
  /** The topmost node under a point in page units. */
  hitTest(page: string, x: number, y: number, options: PreviewOptions): Promise<HitTestResult>
  /** The node's properties on their own, for an inspector that needs them fresh. */
  nodeProps(node: string): Promise<PropsView>
  /** The bytes a save would write. */
  exportBytes(): Promise<Uint8Array>
  /** Records that the bytes were written successfully. */
  noteSaved(): Promise<void>
  /** Opens a document from bytes. A host supplies these after a file dialog. */
  openBytes(bytes: Uint8Array, displayName: string | null): Promise<DocumentView>

  /**
   * Opens a document through the host's own file picker.
   *
   * Optional, and absent in a browser build, which has no picker and no file
   * system access. The interface works without it rather than assuming it.
   * Resolves to `null` when the person cancels, which is an answer and not a
   * failure.
   */
  openDocument?(): Promise<DocumentView | null>

  /**
   * Saves through the host's own picker, returning the file name it wrote.
   *
   * Optional for the same reason as [`openDocument`]. Resolves to `null` when the
   * person cancels.
   */
  saveDocument?(): Promise<string | null>
}

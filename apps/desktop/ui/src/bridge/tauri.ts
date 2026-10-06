/**
 * The desktop bridge: the interface's side of the Tauri IPC surface.
 *
 * ## What this file is allowed to know
 *
 * The command names and the shape of the arguments. Nothing else. Every rule —
 * what an edit means, when a revision is stale, what a failure is called —
 * lives in the Rust application service, and this file must not restate any of
 * it. When this adapter disagrees with the service, the service is right.
 *
 * ## Why the results are not trusted blindly
 *
 * Rust serializes these types through `serde`; TypeScript cannot check that at
 * runtime. The types here are a promise the host makes, and the host's own tests
 * pin the serialized shape. Re-validating every field in JavaScript would
 * duplicate the service's rules in the place least able to enforce them.
 */

import { invoke } from '@tauri-apps/api/core'

import {
  AppError,
  type AppErrorShape,
  type CommitSummary,
  type DocumentView,
  type EditorBridge,
  type EditRequest,
  type HitTestResult,
  type LayoutView,
  type Preview,
  type PreviewOptions,
  type PropsView,
} from './types'

/** The command names the host registers. Kept in one place so they cannot drift. */
export const COMMANDS = {
  getView: 'get_view',
  openBytes: 'open_bytes',
  openDocument: 'open_document',
  saveDocument: 'save_document',
  apply: 'apply',
  undo: 'undo',
  redo: 'redo',
  nodeProps: 'node_props',
  layout: 'layout',
  hitTest: 'hit_test',
  preview: 'preview',
  exportBytes: 'export_bytes',
  noteSaved: 'note_saved',
} as const

/**
 * Calls one host command and turns a rejection into an [`AppError`].
 *
 * Tauri rejects with whatever the command returned as its error, which for this
 * host is the `AppError` shape. Anything else — a missing command, a transport
 * failure — is reported as an invalid request rather than thrown as a bare
 * string, so a caller always has a code to branch on.
 */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (cause) {
    if (isErrorShape(cause)) {
      throw new AppError(cause)
    }
    const message = typeof cause === 'string' ? cause : `the command ${command} failed`
    throw new AppError({ code: 'invalid-request', message })
  }
}

function isErrorShape(value: unknown): value is AppErrorShape {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as { code?: unknown; message?: unknown }
  return typeof candidate.code === 'string' && typeof candidate.message === 'string'
}

/**
 * The interface's implementation over the desktop host.
 *
 * Only a build inside the desktop shell constructs this. The browser build uses
 * the sample service instead, which is why no component imports this file.
 */
export class TauriEditorBridge implements EditorBridge {
  getView(): Promise<DocumentView> {
    return call<DocumentView>(COMMANDS.getView)
  }

  openBytes(bytes: Uint8Array, displayName: string | null): Promise<DocumentView> {
    // Tauri serializes a `Vec<u8>` as a number array. Converting here keeps the
    // interface's own type honest — it works with bytes, not with a JSON list.
    return call<DocumentView>(COMMANDS.openBytes, {
      bytes: Array.from(bytes),
      displayName,
    })
  }

  apply(request: EditRequest): Promise<CommitSummary> {
    return call<CommitSummary>(COMMANDS.apply, { request })
  }

  undo(expectedRevision: number): Promise<CommitSummary> {
    return call<CommitSummary>(COMMANDS.undo, { expectedRevision })
  }

  redo(expectedRevision: number): Promise<CommitSummary> {
    return call<CommitSummary>(COMMANDS.redo, { expectedRevision })
  }

  nodeProps(node: string): Promise<PropsView> {
    return call<PropsView>(COMMANDS.nodeProps, { node })
  }

  layout(page: string, options: PreviewOptions): Promise<LayoutView> {
    return call<LayoutView>(COMMANDS.layout, { page, options })
  }

  hitTest(
    page: string,
    x: number,
    y: number,
    options: PreviewOptions,
  ): Promise<HitTestResult> {
    return call<HitTestResult>(COMMANDS.hitTest, { page, x, y, options })
  }

  async preview(page: string, options: PreviewOptions): Promise<Preview> {
    const preview = await call<Preview>(COMMANDS.preview, { page, options })
    // A large payload should cross as a binary response rather than as a JSON
    // number array. Until the host uses `tauri::ipc::Response`, the bytes arrive
    // as a list and are converted once, here.
    return { ...preview, png: toBytes(preview.png) }
  }

  exportBytes(): Promise<Uint8Array> {
    return call<number[]>(COMMANDS.exportBytes).then(toBytes)
  }

  noteSaved(): Promise<void> {
    return call<void>(COMMANDS.noteSaved)
  }

  /**
   * Opens a document through the host's dialog.
   *
   * The host picks the file and reads it in Rust; the WebView never learns the
   * path. That is why this returns a document rather than a location.
   */
  openDocument(): Promise<DocumentView | null> {
    return call<DocumentView | null>(COMMANDS.openDocument)
  }

  /** Saves through the host's dialog and returns the file name it wrote. */
  saveDocument(): Promise<string | null> {
    return call<string | null>(COMMANDS.saveDocument)
  }
}

function toBytes(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) return value
  if (Array.isArray(value)) return Uint8Array.from(value as number[])
  return new Uint8Array()
}

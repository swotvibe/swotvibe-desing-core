import type { InjectionKey } from 'vue'

import type { EditorBridge } from '@/bridge/types'
import type { EditorSession } from './useEditor'

/**
 * The injection keys the shell uses.
 *
 * Two keys rather than one, because they answer different questions:
 *
 * - [`editorKey`] is the **service**: what can be done to the document.
 * - [`editorStateKey`] is the **session**: what is currently selected, which tool
 *   is active, the zoom, and the last failure.
 *
 * Keeping them apart matters. When each component built its own session, the
 * layer panel and the canvas disagreed about what was selected — a bug that looks
 * like a rendering problem and is actually a state-ownership problem. The session
 * is created once, at the top of the tree, and injected below.
 */
export const editorKey: InjectionKey<EditorBridge> = Symbol('swotvibe.editor-bridge')

/** The session state, created once by the shell and injected by its children. */
export const editorStateKey: InjectionKey<EditorSession> = Symbol('swotvibe.editor-state')

/** The active tool. Only `select` changes anything today. */
export type ToolId = 'move' | 'select' | 'frame' | 'shape' | 'pen' | 'text' | 'comment' | 'hand'

export interface ToolDefinition {
  id: ToolId
  /** The accessible name, also used as the tooltip. */
  label: string
  /** Whether the tool is wired to the document service yet. */
  available: boolean
}

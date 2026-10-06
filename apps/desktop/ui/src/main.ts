import { createApp } from 'vue'

import App from './App.vue'
import './assets/main.css'
import { SampleEditorBridge } from './bridge/sample'
import { TauriEditorBridge } from './bridge/tauri'
import type { EditorBridge } from './bridge/types'
import { editorKey } from './editor/context'

/**
 * Chooses the service the shell talks to.
 *
 * The desktop host injects `__TAURI_INTERNALS__` into the page it serves, so one
 * build runs inside the shell and in a plain browser without a second build
 * configuration. In a browser there is no host and no file system, so the sample
 * service is used: the shell is the same, and the document it opens is the
 * committed M0 sample.
 *
 * This is the only place a service is chosen. A component never imports one.
 */
function selectBridge(): EditorBridge {
  return '__TAURI_INTERNALS__' in window ? new TauriEditorBridge() : new SampleEditorBridge()
}

createApp(App).provide(editorKey, selectBridge()).mount('#app')
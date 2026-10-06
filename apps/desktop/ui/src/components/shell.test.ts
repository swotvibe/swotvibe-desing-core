import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import App from '@/App.vue'
import { SampleEditorBridge } from '@/bridge/sample'
import type { DocumentView, EditorBridge, LayoutView, PropsView } from '@/bridge/types'
import { editorKey } from '@/editor/context'
import { createEditorSession } from '@/editor/useEditor'

/**
 * Shell tests.
 *
 * The whole application is mounted, not one component at a time, because the
 * properties worth checking are about *shared* state: a selection made in the
 * layer panel has to be the selection the inspector shows. Testing components in
 * isolation would hide exactly the bug this design exists to prevent.
 *
 * What is not covered here — geometry, hit-testing, revisions, byte-stable save —
 * is covered in Rust, where those rules live.
 */

const CARD = '018f0000-0000-7000-8000-000000020001'
const HEADER = '018f0000-0000-7000-8000-000000020002'

/**
 * A service whose second view no longer contains the node that was selected.
 *
 * Used to test one rule — a selection is validated against the view it came
 * from — without giving the sample service a method that exists only for tests.
 */
class VanishingBridge implements EditorBridge {
  private calls = 0

  private view(): DocumentView {
    this.calls += 1
    // Present only in the first view, so the second one simulates a node that
    // was removed between two reads.
    const present = this.calls <= 1
    return {
      revision: this.calls,
      dirty: false,
      displayName: 'Vanishing',
      pages: [{ id: 'page', name: 'Page', roots: [CARD] }],
      nodes: present
        ? [
            {
              id: CARD,
              kind: 'frame',
              name: 'Card',
              page: 'page',
              parent: null,
              children: [HEADER],
              props: props(),
            },
            {
              id: HEADER,
              kind: 'shape',
              name: 'Header',
              page: 'page',
              parent: CARD,
              children: [],
              props: props(),
            },
          ]
        : [
            {
              id: CARD,
              kind: 'frame',
              name: 'Card',
              page: 'page',
              parent: null,
              children: [],
              props: props(),
            },
          ],
    }
  }

  getView(): Promise<DocumentView> {
    return Promise.resolve(this.view())
  }

  layout(page: string): Promise<LayoutView> {
    return Promise.resolve({ revision: 1, page, pageSize: [100, 100], nodes: [], diagnostics: [] })
  }

  apply(): never {
    throw new Error('not used')
  }
  undo(): never {
    throw new Error('not used')
  }
  redo(): never {
    throw new Error('not used')
  }
  preview(): never {
    throw new Error('not used')
  }
  hitTest(): never {
    throw new Error('not used')
  }
  nodeProps(): never {
    throw new Error('not used')
  }
  exportBytes(): never {
    throw new Error('not used')
  }
  noteSaved(): never {
    throw new Error('not used')
  }
  openBytes(): never {
    throw new Error('not used')
  }
}

function props(): PropsView {
  return {
    size: [10, 10],
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
  }
}

async function mountShell(): Promise<{ shell: VueWrapper; bridge: SampleEditorBridge }> {
  const bridge = new SampleEditorBridge()
  const shell = mount(App as never, {
    global: { provide: { [editorKey as symbol]: bridge } },
    attachTo: document.body,
  })
  // The shell loads the document on mount; waiting here keeps every assertion
  // below about a loaded state rather than a race.
  await flushPromises()
  await flushPromises()
  return { shell, bridge }
}

function layerRows(shell: VueWrapper) {
  return shell.findAll('[role="treeitem"]')
}

function selectLayer(shell: VueWrapper, name: string): Promise<void> {
  const row = layerRows(shell).find((item) => item.text().includes(name))
  if (!row) throw new Error(`no layer row named ${name}`)
  return row.find('button:not([aria-label])').trigger('click')
}

describe('the shell', () => {
  it('renders the workspace from the service, not from a local copy', async () => {
    const { shell } = await mountShell()

    expect(shell.text()).toContain('Preview')
    expect(shell.text()).toContain('Pages')
    expect(shell.text()).toContain('Layers')
    expect(layerRows(shell)).toHaveLength(5)
    expect(layerRows(shell)[0]?.text()).toContain('Card')
  })

  it('filters layers without losing the parent of a match', async () => {
    const { shell } = await mountShell()

    await shell.find('#layer-search').setValue('Dot')

    const rows = layerRows(shell)
    expect(rows).toHaveLength(2)
    expect(rows[0]?.text()).toContain('Card')
    expect(rows[1]?.text()).toContain('Dot')
  })
})

describe('the selection', () => {
  it('is shared between the layer panel and the inspector', async () => {
    const { shell } = await mountShell()

    expect(shell.text()).toContain('Select a layer or click the canvas')

    await selectLayer(shell, 'Header')
    await flushPromises()

    // The inspector shows the same node the layer panel marked.
    expect(shell.findAll('[role="treeitem"][aria-selected="true"]')).toHaveLength(1)
    expect(shell.find('input[aria-label="Fill hex value"]').exists()).toBe(true)
    expect((shell.find('input[aria-label="Fill hex value"]').element as HTMLInputElement).value).toBe(
      '#4c6ef5',
    )
  })

  it('is cleared when the selected node is no longer in the document', async () => {
    // A node can vanish between two views — an undo does exactly that — so the
    // rule is tested directly against the session with a stub service, rather
    // than by adding a test-only method to the sample one.
    const session = createEditorSession(new VanishingBridge())
    await session.load()
    session.select(HEADER)
    expect(session.selected.value?.id).toBe(HEADER)

    await session.load()
    expect(session.selectedId.value).toBeNull()
    expect(session.selected.value).toBeNull()
  })
})

describe('editing a property', () => {
  it('writes the fill through the service and marks the document modified', async () => {
    const { shell, bridge } = await mountShell()

    await selectLayer(shell, 'Header')
    await flushPromises()

    const hex = shell.find('input[aria-label="Fill hex value"]')
    // `setValue` on a text input fires `input` and `change`, and the field
    // commits on `change`; triggering it again would be a second edit.
    await hex.setValue('#ff0000')
    await flushPromises()

    // Asserted against the document, not the component: the service is the
    // source of truth and the interface must not be a second one.
    await expect(bridge.nodeProps(HEADER)).resolves.toMatchObject({
      fill: { r: 255, g: 0, b: 0, a: 255 },
    })
    await expect(bridge.getView()).resolves.toMatchObject({ revision: 2, dirty: true })
  })

  it('refuses a half-typed colour instead of storing a guess', async () => {
    const { shell, bridge } = await mountShell()

    await selectLayer(shell, 'Header')
    await flushPromises()

    const hex = shell.find('input[aria-label="Fill hex value"]')
    // `setValue` on a text input fires `input` and `change`, and the field
    // commits on `change`; triggering it again would be a second edit.
    await hex.setValue('#12')
    await flushPromises()

    await expect(bridge.nodeProps(HEADER)).resolves.toMatchObject({
      fill: { r: 76, g: 110, b: 245, a: 255 },
    })
    await expect(bridge.getView()).resolves.toMatchObject({ revision: 1 })
  })

  it('marks the document saved only after a write succeeds', async () => {
    const { shell, bridge } = await mountShell()

    await selectLayer(shell, 'Header')
    await flushPromises()

    const hex = shell.find('input[aria-label="Fill hex value"]')
    // `setValue` on a text input fires `input` and `change`, and the field
    // commits on `change`; triggering it again would be a second edit.
    await hex.setValue('#00ff00')
    await flushPromises()
    await expect(bridge.getView()).resolves.toMatchObject({ dirty: true })

    await shell.find('button[aria-label="Save"]').trigger('click')
    await flushPromises()
    await flushPromises()

    await expect(bridge.getView()).resolves.toMatchObject({ dirty: false })
  })
})

describe('undo with an empty timeline', () => {
  it('surfaces a typed failure instead of doing nothing quietly', async () => {
    const { shell } = await mountShell()

    await shell.find('button[aria-label="Undo"]').trigger('click')
    await flushPromises()

    expect(shell.text()).toContain('history')
    expect(shell.text()).toContain('nothing to undo')
  })
})

describe('opening and saving', () => {
  it('disables Open in a build with no file reader', async () => {
    const { shell } = await mountShell()

    // The sample service has no file picker, so the control is off rather than
    // present and broken.
    expect(shell.find('button[aria-label="Open"]').attributes('disabled')).toBeDefined()
  })

  it('offers Open when the host can supply a file', async () => {
    const bridge = new SampleEditorBridge()
    const opened = { calls: 0 }
    const hostBridge = Object.assign(bridge, {
      openDocument: async () => {
        opened.calls += 1
        return bridge.getView()
      },
    })

    const shell = mount(App as never, {
      global: { provide: { [editorKey as symbol]: hostBridge } },
      attachTo: document.body,
    })
    await flushPromises()

    expect(shell.find('button[aria-label="Open"]').attributes('disabled')).toBeUndefined()
    await shell.find('button[aria-label="Open"]').trigger('click')
    await flushPromises()
    expect(opened.calls).toBe(1)
  })

  it('treats a cancelled save as an answer rather than a failure', async () => {
    const session = createEditorSession(
      Object.assign(new SampleEditorBridge(), {
        saveDocument: async () => null,
      }),
    )
    await session.load()

    await session.save()

    expect(session.failure.value).toBeNull()
    expect(session.status.value).toBe('Save cancelled')
  })

  it('records the file name a successful host write reported', async () => {
    const session = createEditorSession(
      Object.assign(new SampleEditorBridge(), {
        saveDocument: async () => 'card.json',
      }),
    )
    await session.load()

    await session.save()

    expect(session.failure.value).toBeNull()
    expect(session.status.value).toContain('card.json')
  })
})

describe('the tool palette', () => {
  it('enables only the tools that are wired to the service', async () => {
    const { shell } = await mountShell()

    const select = shell.find('button[aria-label="Select"]')
    expect(select.attributes('disabled')).toBeUndefined()

    expect(shell.find('button[aria-label="Frame"]').attributes('disabled')).toBeDefined()
    expect(shell.find('button[aria-label="Pen"]').attributes('disabled')).toBeDefined()
  })

  it('keeps the canvas usable while a creation tool is unavailable', async () => {
    const { shell } = await mountShell()

    await shell.find('button[aria-label="Frame"]').trigger('click')
    await flushPromises()

    // A disabled tool must not change what a click on a layer does.
    await selectLayer(shell, 'Card')
    await flushPromises()
    expect(shell.findAll('[role="treeitem"][aria-selected="true"]')).toHaveLength(1)
  })
})

describe('the canvas', () => {
  it('converts a click into page units before asking the service', async () => {
    const { shell } = await mountShell()

    const stage = shell.find('[data-test="stage"]')
    expect(stage.exists()).toBe(true)

    // The artboard is drawn at the current zoom (21%), so the stage element is
    // the only place that knows the screen-to-page mapping. A click at 11px
    // lands near page unit 52, which is inside the frame but outside the header.
    const element = stage.element as HTMLElement
    element.getBoundingClientRect = () =>
      ({ left: 0, top: 0, width: 100, height: 100, right: 100, bottom: 100, x: 0, y: 0 }) as DOMRect

    await stage.trigger('click', { clientX: 11, clientY: 11 })
    await flushPromises()

    const selected = shell.findAll('[role="treeitem"][aria-selected="true"]')
    expect(selected).toHaveLength(1)
    expect(selected[0]?.text()).toContain('Card')
  })

  it('selects nothing when the click lands outside every node', async () => {
    const { shell } = await mountShell()

    const stage = shell.find('[data-test="stage"]')
    const element = stage.element as HTMLElement
    element.getBoundingClientRect = () =>
      ({ left: 0, top: 0, width: 100, height: 100, right: 100, bottom: 100, x: 0, y: 0 }) as DOMRect

    // Page unit (0, 0) is outside the frame, which starts at (40, 40).
    await stage.trigger('click', { clientX: 0, clientY: 0 })
    await flushPromises()

    expect(shell.findAll('[role="treeitem"][aria-selected="true"]')).toHaveLength(0)
    expect(shell.text()).toContain('Nothing selected')
  })
})

describe('the layer count', () => {
  it('reflects the document instead of a cached number', async () => {
    const { shell } = await mountShell()

    expect(shell.text()).toContain('5')
  })
})

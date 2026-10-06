import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import App from '@/App.vue'
import { SampleEditorBridge } from '@/bridge/sample'
import type {
  Capabilities,
  DocumentView,
  EditorBridge,
  LayoutView,
  PropsView,
} from '@/bridge/types'
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

  capabilities(): Promise<Capabilities> {
    return Promise.resolve({
      creatableKinds: ['shape'],
      fontFamilies: ['Inter'],
      renderer: 'test',
      layoutEngine: 'test',
    })
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
    positionIsLayoutDecided: false,
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

/** Commits a fill through the inspector field, as the panel does. */
async function editorEditFill(shell: VueWrapper, hex: string): Promise<void> {
  const field = shell.find('input[aria-label="Fill hex value"]')
  await field.setValue(hex)
  await flushPromises()
  await flushPromises()
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

/** Drains pending microtasks and timers until the shell has settled. */
async function settle(): Promise<void> {
  for (let i = 0; i < 4; i += 1) {
    await flushPromises()
  }
}

describe('a document with no pages', () => {
  it('offers a way forward instead of a blank canvas', async () => {
    const bridge = new SampleEditorBridge()
    bridge.empty()

    const shell = mount(App as never, {
      global: { provide: { [editorKey as symbol]: bridge } },
      attachTo: document.body,
    })
    await flushPromises()

    expect(shell.find('[data-test="empty-state"]').exists()).toBe(true)
    expect(shell.find('[data-test="stage"]').exists()).toBe(false)
    expect(shell.text()).toContain('No document is open')

    // The sample control is the one a first run can use without a file.
    const sample = shell
      .findAll('button')
      .find((button) => button.text().includes('Open the sample'))
    expect(sample).toBeDefined()

    await sample!.trigger('click')
    // Opening runs an async action behind a synchronous guard, so the click
    // returns before the document has been replaced.
    await settle()

    expect(shell.find('[data-test="stage"]').exists()).toBe(true)
    expect(shell.findAll('[role="treeitem"]')).toHaveLength(5)
  })

  it('does not offer a file it cannot read', async () => {
    const bridge = new SampleEditorBridge()
    bridge.empty()

    const shell = mount(App as never, {
      global: { provide: { [editorKey as symbol]: bridge } },
      attachTo: document.body,
    })
    await flushPromises()

    const open = shell
      .findAll('button')
      .find((button) => button.text().includes('Open a file'))
    expect(open?.attributes('disabled')).toBeDefined()
  })
})

describe('creating and deleting', () => {
  it('creates a node of the kind the tool names and selects it', async () => {
    const { shell, bridge } = await mountShell()
    const before = (await bridge.getView()).nodes.length

    await shell.find('button[aria-label="Shape"]').trigger('click')
    await flushPromises()
    await flushPromises()

    const view = await bridge.getView()
    expect(view.nodes).toHaveLength(before + 1)
    expect(view.revision).toBeGreaterThan(1)

    // The new node is selected, which is what makes the create visible: the
    // inspector fills in and the layer row highlights.
    const selected = shell.findAll('[role="treeitem"][aria-selected="true"]')
    expect(selected).toHaveLength(1)
    const created = view.nodes.at(-1)
    expect(created).toBeDefined()
    expect(selected[0]?.text()).toContain(created!.name || created!.id.slice(-6))
  })

  it('creates a node that is visible rather than zero-sized', async () => {
    const { shell, bridge } = await mountShell()

    await shell.find('button[aria-label="Shape"]').trigger('click')
    await flushPromises()
    await flushPromises()

    const view = await bridge.getView()
    const created = view.nodes.at(-1)
    // The document model's default is deliberately empty, so a zero-sized,
    // unfilled node would be invisible and the create would look like nothing
    // happened. The tool supplies defaults, and the stand-in mirrors them.
    expect(created?.props.size[0]).toBeGreaterThan(0)
    expect(created?.props.size[1]).toBeGreaterThan(0)
    expect(created?.props.fill).not.toBeNull()
  })

  it('creates a text node with a registered family, so it can be measured', async () => {
    const { shell, bridge } = await mountShell()

    await shell.find('button[aria-label="Text"]').trigger('click')
    await flushPromises()
    await flushPromises()

    const view = await bridge.getView()
    const created = view.nodes.at(-1)
    expect(created?.kind).toBe('text')
    // A family that is not registered fails layout, and layout runs on every
    // refresh, so this is the assertion that the create is actually usable.
    expect(created?.props.fontFamily).toBe('Inter')
  })

  it('hands control back to Select after a creation tool acts', async () => {
    const { shell } = await mountShell()

    await shell.find('button[aria-label="Frame"]').trigger('click')
    await flushPromises()
    await flushPromises()

    const select = shell.find('button[aria-label="Select"]')
    expect(select.attributes('aria-pressed')).toBe('true')
  })

  it('deletes the selected node through the palette', async () => {
    const { shell, bridge } = await mountShell()
    const before = (await bridge.getView()).nodes.length

    await selectLayer(shell, 'Dot')
    await flushPromises()

    await shell.find('button[aria-label="Delete"]').trigger('click')
    await flushPromises()
    await flushPromises()

    const view = await bridge.getView()
    expect(view.nodes).toHaveLength(before - 1)
    expect(view.nodes.some((node) => node.name === 'Dot')).toBe(false)
  })

  it('refuses to delete with nothing selected rather than guessing', async () => {
    const { shell, bridge } = await mountShell()
    const before = (await bridge.getView()).nodes.length

    // Nothing is selected on a fresh shell.
    expect(shell.findAll('[role="treeitem"][aria-selected="true"]')).toHaveLength(0)
    expect(shell.find('button[aria-label="Delete"]').attributes('disabled')).toBeDefined()

    expect((await bridge.getView()).nodes).toHaveLength(before)
  })

  it('offers a creation tool only for a kind the build reports', async () => {
    // A build that can create nothing: the palette must not offer creation.
    const session = createEditorSession(new VanishingBridge())
    await session.loadCapabilities()

    expect(session.capabilities.value?.creatableKinds).toEqual(['shape'])
    expect(session.canCreate('shape')).toBe(true)
    expect(session.canCreate('frame')).toBe(false)
    expect(session.canCreate('text')).toBe(false)
  })

  it('offers nothing before the build has answered', async () => {
    // The capabilities are asked for during load. Before it answers, an empty
    // list is the honest state: offering a kind and then failing is worse than
    // offering nothing for a moment.
    const session = createEditorSession(new VanishingBridge())
    expect(session.capabilities.value).toBeNull()
    expect(session.canCreate('shape')).toBe(false)
  })
})

describe('editing geometry', () => {
  it('renames the selected node from the inspector field', async () => {
    const { shell, bridge } = await mountShell()

    await selectLayer(shell, 'Header')
    await flushPromises()

    const name = shell.find('input[aria-label="Node name"]')
    expect((name.element as HTMLInputElement).value).toBe('Header')

    await name.setValue('Banner')
    await flushPromises()
    await flushPromises()

    const view = await bridge.getView()
    expect(view.nodes.find((node) => node.id === HEADER)?.name).toBe('Banner')
  })

  it('moves a node whose position the layout does not decide', async () => {
    const { shell, bridge } = await mountShell()

    // The Card is a page root: a page has no layout rule, so the transform
    // places it and the position fields act.
    await selectLayer(shell, 'Card')
    await flushPromises()

    const fields = shell.findAll('input[aria-label="X"], input[aria-label="Y"]')
    expect(fields.length).toBe(2)
    expect(fields[0]?.attributes('disabled')).toBeUndefined()

    await fields[0]!.setValue('120')
    await flushPromises()
    await flushPromises()

    const props = await bridge.nodeProps(CARD)
    expect(props.transform[4]).toBe(120)
  })

  it('disables the position fields when the parent lays the node out', async () => {
    const { shell } = await mountShell()

    // The Card is a flex column in both the fixture and the stand-in, so a
    // child's position comes from layout. A field that accepted a value which
    // never appears would look like a broken editor; the reason is on screen.
    await selectLayer(shell, 'Header')
    await flushPromises()

    const x = shell.find('input[aria-label="X"]')
    expect(x.exists()).toBe(true)
    expect(x.attributes('disabled')).toBeDefined()
    expect(shell.text()).toContain("position comes from layout")
  })

  it('resizes a node from the inspector fields', async () => {
    const { shell, bridge } = await mountShell()

    await selectLayer(shell, 'Card')
    await flushPromises()

    const width = shell.find('input[aria-label="W"]')
    await width.setValue('250')
    await flushPromises()
    await flushPromises()

    const props = await bridge.nodeProps(CARD)
    expect(props.size[0]).toBe(250)
    // Stating a size switches the axis to fixed, or `hug` would ignore it.
    expect(props.widthSizing).toBe('fixed')
  })

  it('sets a corner radius on a shape and refuses it elsewhere', async () => {
    const { shell, bridge } = await mountShell()

    await selectLayer(shell, 'Header')
    await flushPromises()
    const radius = shell.find('input[aria-label="R"]')
    expect(radius.attributes('disabled')).toBeUndefined()

    await radius.setValue('20')
    await flushPromises()
    await flushPromises()
    expect((await bridge.nodeProps(HEADER)).cornerRadius).toBe(20)

    // A text node has no corner: the field is disabled with the reason shown,
    // matching the service's refusal.
    await selectLayer(shell, 'Latin')
    await flushPromises()
    expect(shell.find('input[aria-label="R"]').attributes('disabled')).toBeDefined()
    expect(shell.text()).toContain('Only a shape has a corner radius')
  })

  it('drops a half-typed number instead of storing it', async () => {
    const { shell, bridge } = await mountShell()

    await selectLayer(shell, 'Card')
    await flushPromises()
    const before = await bridge.nodeProps(CARD)

    const width = shell.find('input[aria-label="W"]')
    ;(width.element as HTMLInputElement).value = 'abc'
    await width.trigger('change')
    await flushPromises()

    expect((await bridge.nodeProps(CARD)).size).toEqual(before.size)
  })

  it('does not call a constant opacity a property', async () => {
    const { shell } = await mountShell()

    await selectLayer(shell, 'Header')
    await flushPromises()

    // The schema has no opacity, so the panel says so rather than showing a
    // value that nothing can change.
    expect(shell.text()).toContain('Not in the schema yet')
  })
})

describe('losing unsaved work', () => {
  /**
   * Replaces the document through the shell's own session.
   *
   * Driven directly rather than through a button, because the control that
   * replaces a document differs by build — a file picker on the desktop, the
   * sample in the browser — while the guard behind it is the same code and is
   * what this suite is about. What the *buttons* do about dirty state is checked
   * separately, above.
   */
  function shellEditor(shell: VueWrapper) {
    return (shell.vm as unknown as { editor: ReturnType<typeof createEditorSession> }).editor
  }

  it('asks before replacing a document with unsaved edits', async () => {
    const { shell, bridge } = await mountShell()

    // Edit something, so the document differs from what it would write.
    await selectLayer(shell, 'Header')
    await flushPromises()
    await editorEditFill(shell, '#ff0000')
    expect((await bridge.getView()).dirty).toBe(true)

    // Replacing the document would discard the edit and the undo history, so the
    // question comes first and nothing has happened yet.
    shellEditor(shell).openSample()
    await flushPromises()

    const dialog = shell.find('[data-test="confirmation"]')
    expect(dialog.exists()).toBe(true)
    expect(dialog.text()).toContain('Unsaved changes')
    expect(dialog.text()).toContain('undo history')
  })

  it('keeps the document when the question is declined', async () => {
    const { shell, bridge } = await mountShell()

    await selectLayer(shell, 'Header')
    await flushPromises()
    await editorEditFill(shell, '#00ff00')
    const before = await bridge.getView()

    shellEditor(shell).openSample()
    await flushPromises()
    await shell.find('[data-test="confirm-cancel"]').trigger('click')
    await flushPromises()
    await flushPromises()

    expect(shell.find('[data-test="confirmation"]').exists()).toBe(false)
    const after = await bridge.getView()
    expect(after).toEqual(before)
    expect(after.dirty).toBe(true)
    expect(after.nodes.find((node) => node.id === HEADER)?.props.fill).toEqual({
      r: 0,
      g: 255,
      b: 0,
      a: 255,
    })
  })


  it('proceeds when the question is accepted', async () => {
    const { shell, bridge } = await mountShell()

    await selectLayer(shell, 'Header')
    await flushPromises()
    await editorEditFill(shell, '#0000ff')
    expect((await bridge.getView()).dirty).toBe(true)

    shellEditor(shell).openSample()
    await flushPromises()
    await shell.find('[data-test="confirm-accept"]').trigger('click')
    await settle()

    expect(shell.find('[data-test="confirmation"]').exists()).toBe(false)

    // Accepting replaced the document with the pristine sample, so the edit is
    // gone and the document is clean again.
    const after = await bridge.getView()
    expect(after.dirty).toBe(false)
    expect(after.nodes.find((node) => node.id === HEADER)?.props.fill).toEqual({
      r: 76,
      g: 110,
      b: 245,
      a: 255,
    })
  })

  it('does not ask when there is nothing to lose', async () => {
    const { shell } = await mountShell()

    // A freshly opened document matches what it would write, so replacing it
    // costs nothing and a question would only be noise.
    expect(shell.find('[data-test="confirmation"]').exists()).toBe(false)
    shellEditor(shell).openSample()
    await flushPromises()

    expect(shell.find('[data-test="confirmation"]').exists()).toBe(false)
  })

  it('takes Escape as the safe answer', async () => {
    const { shell, bridge } = await mountShell()

    await selectLayer(shell, 'Dot')
    await flushPromises()
    await editorEditFill(shell, '#123456')
    expect((await bridge.getView()).dirty).toBe(true)

    shellEditor(shell).openSample()
    await flushPromises()
    expect(shell.find('[data-test="confirmation"]').exists()).toBe(true)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(shell.find('[data-test="confirmation"]').exists()).toBe(false)
    // Escape declines, so the edit survives.
    expect((await bridge.getView()).dirty).toBe(true)
  })
})

describe('keyboard shortcuts', () => {
  it('deletes the selection with Delete', async () => {
    const { shell, bridge } = await mountShell()
    const before = (await bridge.getView()).nodes.length

    await selectLayer(shell, 'Dot')
    await flushPromises()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete' }))
    await flushPromises()
    await flushPromises()

    expect((await bridge.getView()).nodes).toHaveLength(before - 1)
  })

  it('clears the selection with Escape', async () => {
    const { shell } = await mountShell()

    await selectLayer(shell, 'Header')
    await flushPromises()
    expect(shell.findAll('[role="treeitem"][aria-selected="true"]')).toHaveLength(1)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(shell.findAll('[role="treeitem"][aria-selected="true"]')).toHaveLength(0)
  })

  it('leaves Delete alone while a text field has focus', async () => {
    const { shell, bridge } = await mountShell()
    const before = (await bridge.getView()).nodes.length

    await selectLayer(shell, 'Dot')
    await flushPromises()

    // Typing in a field must delete a character, not the selected node.
    const search = shell.find('#layer-search')
    ;(search.element as HTMLInputElement).focus()
    search.element.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Delete', bubbles: true }),
    )
    await flushPromises()

    expect((await bridge.getView()).nodes).toHaveLength(before)
  })
})

describe('the tool palette', () => {
  it('enables the creation tools the build reports and leaves the rest', async () => {
    const { shell } = await mountShell()

    // The sample service can create these, so they act.
    for (const label of ['Frame', 'Shape', 'Text']) {
      expect(shell.find(`button[aria-label="${label}"]`).attributes('disabled')).toBeUndefined()
    }

    // Nothing is behind these two yet, and a control that looks live and does
    // nothing is worse than one that is visibly unavailable.
    expect(shell.find('button[aria-label="Pen"]').attributes('disabled')).toBeDefined()
    expect(shell.find('button[aria-label="Comment"]').attributes('disabled')).toBeDefined()

    // Select stays pressed until a creation tool acts.
    expect(shell.find('button[aria-label="Select"]').attributes('aria-pressed')).toBe('true')
  })

  it('names why an unavailable tool is unavailable', async () => {
    const { shell } = await mountShell()

    expect(shell.find('button[aria-label="Pen"]').attributes('title')).toContain(
      'not available yet',
    )
    expect(shell.find('button[aria-label="Shape"]').attributes('title')).toContain('shape')
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

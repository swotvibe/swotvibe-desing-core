import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import App from '@/App.vue'
import { SampleEditorBridge } from '@/bridge/sample'
import { editorKey } from './context'
import { createEditorSession, PREVIEW_OPTIONS } from './useEditor'

enableAutoUnmount(afterEach)
afterEach(() => { vi.restoreAllMocks(); document.body.innerHTML = '' })
const CARD = '018f0000-0000-7000-8000-000000020001'
const HEADER = '018f0000-0000-7000-8000-000000020002'
const LATIN = '018f0000-0000-7000-8000-000000020003'

async function setup() {
  const bridge = new SampleEditorBridge()
  const shell = mount(App, { global: { provide: { [editorKey as symbol]: bridge } }, attachTo: document.body })
  await flushPromises()
  const editor = (shell.vm as unknown as { editor: ReturnType<typeof createEditorSession> }).editor
  return { shell, editor, bridge }
}

function pointer(element: Element, type: string, x: number, y: number) {
  const event = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 })
  Object.defineProperty(event, 'pointerId', { value: 1 })
  element.dispatchEvent(event)
}

describe('editor interaction regressions', () => {
  it('opens a portrait artboard at its full size and uses it for rendering and picking', async () => {
    const bridge = new SampleEditorBridge()
    const view = await bridge.getView()
    const frame = view.nodes.find((node) => node.id === CARD)!
    frame.props.size = [1080, 1350]
    frame.props.transform = [1, 0, 0, 1, 0, 0]
    frame.props.widthSizing = 'fixed'
    frame.props.heightSizing = 'fixed'
    vi.spyOn(bridge, 'getView').mockResolvedValue(view)
    const layout = vi.spyOn(bridge, 'layout')
    const hit = vi.spyOn(bridge, 'hitTest')
    Object.assign(bridge, { rasterPreview: true })
    const preview = vi.spyOn(bridge, 'preview')
    const editor = createEditorSession(bridge)
    await editor.load()
    expect(editor.failure.value).toBeNull()
    expect(editor.layout.value?.pageSize).toEqual([1080, 1350])
    expect(layout).toHaveBeenLastCalledWith(bridge.pageId, expect.objectContaining({ pageSize: [1080, 1350] }))
    expect(preview).toHaveBeenLastCalledWith(bridge.pageId, expect.objectContaining({ pageSize: [1080, 1350] }))
    await editor.selectAt(800, 1200)
    expect(hit).toHaveBeenLastCalledWith(bridge.pageId, 800, 1200, expect.objectContaining({ pageSize: [1080, 1350] }))
    editor.dispose()
  })

  it('honors Save clicked while a property update is still pending', async () => {
    const { shell, editor, bridge } = await setup()
    const apply = bridge.apply.bind(bridge)
    let release!: () => void
    const gate = new Promise<void>((resolve) => { release = resolve })
    vi.spyOn(bridge, 'apply').mockImplementation(async (request) => { await gate; return apply(request) })
    const saved: string[] = []
    Object.assign(bridge, { saveDocument: async () => {
      saved.push((await bridge.getView()).nodes.find((node) => node.id === HEADER)!.name)
      await bridge.noteSaved()
      return 'design.json'
    } })
    editor.select(HEADER)
    const edit = editor.renameSelected('Save this draft')
    await flushPromises()
    const disabled = shell.get('button[aria-label="Save"]').attributes('disabled')
    await shell.get('button[aria-label="Save"]').trigger('click')
    release()
    await edit
    await editor.whenIdle()
    expect(disabled).toBeUndefined()
    expect(saved).toEqual(['Save this draft'])
    expect(editor.dirty.value).toBe(false)
  })

  it('waits for pending edits before asking to replace their document', async () => {
    const { editor } = await setup()
    editor.select(HEADER)
    const edit = editor.renameSelected('Keep this draft')
    editor.newDocument()
    await edit
    await editor.whenIdle()
    expect(editor.awaitingConfirmation.value).toBe(true)
    editor.resolveConfirmation(false)
    expect(editor.selected.value?.name).toBe('Keep this draft')
  })

  it('refreshes a conflicted revision without retrying the refused edit', async () => {
    const { editor, bridge } = await setup()
    editor.select(HEADER)
    await bridge.apply({ expectedRevision: editor.revision.value, commands: [
      { kind: 'rename-node', node: HEADER, name: 'External edit' },
    ] })
    await editor.renameSelected('Refused edit')
    expect(editor.revision.value).toBe((await bridge.getView()).revision)
    expect(editor.selected.value?.name).toBe('External edit')
    expect(editor.failure.value?.code).toBe('revision-conflict')
    await editor.renameSelected('A fresh edit')
    expect(editor.selected.value?.name).toBe('A fresh edit')
    expect(editor.failure.value).toBeNull()
  })

  it('ignores a late hit-test error after a newer selection', async () => {
    const { editor, bridge } = await setup()
    let reject!: (cause: Error) => void
    vi.spyOn(bridge, 'hitTest').mockImplementation(() => new Promise((_, refuse) => { reject = refuse }))
    const hit = editor.selectAt(60, 60)
    editor.select(LATIN)
    reject(new Error('Obsolete hit test'))
    await hit
    expect(editor.selectedId.value).toBe(LATIN)
    expect(editor.failure.value).toBeNull()
  })

  it('does not apply editing shortcuts during input composition', async () => {
    const { editor } = await setup()
    editor.select(CARD)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', isComposing: true, cancelable: true }))
    await editor.whenIdle()
    expect(editor.nodesById.value.has(CARD)).toBe(true)
    expect(editor.dirty.value).toBe(false)
  })

  it('waits for composition to finish before committing a field with Enter', async () => {
    const { shell, editor, bridge } = await setup()
    editor.select(HEADER)
    await flushPromises()
    const name = shell.get('input[aria-label="Node name"]')
    ;(name.element as HTMLInputElement).value = 'Still composing'
    await name.trigger('keydown', { key: 'Enter', isComposing: true })
    await editor.whenIdle()
    expect((await bridge.getView()).nodes.find((node) => node.id === HEADER)?.name).toBe('Header')
  })

  it('collapses and reopens the existing text properties section', async () => {
    const { shell, editor } = await setup()
    editor.select(LATIN)
    await flushPromises()
    const toggle = shell.findAll('button').find((button) => button.text() === 'Text')!
    await toggle.trigger('click')
    expect(shell.find('textarea[aria-label="Text content"]').exists()).toBe(false)
    await toggle.trigger('click')
    expect(shell.find('textarea[aria-label="Text content"]').exists()).toBe(true)
  })

  it('shows when a layer filter has no matches', async () => {
    const { shell } = await setup()
    await shell.get('#layer-search').setValue('No matching layer')
    expect(shell.findAll('[role="treeitem"]')).toHaveLength(0)
    expect(shell.get('[data-test="no-layer-results"]').text()).toContain('No layers match')
  })

  it('reveals a canvas selection inside a collapsed layer group', async () => {
    const { shell, editor } = await setup()
    await shell.get('button[aria-label="Collapse"]').trigger('click')
    editor.select(HEADER)
    await flushPromises()
    expect(shell.get('[role="treeitem"][aria-selected="true"]').text()).toContain('Header')
  })

  it('nudges in page directions under a rotated and scaled parent', async () => {
    const { editor, bridge } = await setup()
    await editor.createNode('shape')
    const id = editor.selectedId.value!
    const before = (await bridge.nodeProps(id)).transform.slice(4)
    const getView = bridge.getView.bind(bridge)
    const layout = bridge.layout.bind(bridge)
    vi.spyOn(bridge, 'getView').mockImplementation(async () => {
      const view = await getView()
      view.nodes.find((node) => node.id === id)!.parent = CARD
      view.nodes.find((node) => node.id === CARD)!.children.push(id)
      view.pages[0]!.roots = view.pages[0]!.roots.filter((root) => root !== id)
      return view
    })
    vi.spyOn(bridge, 'layout').mockImplementation(async (page, options) => {
      const drawing = await layout(page, options)
      drawing.nodes.find((node) => node.id === CARD)!.world = [0, 2, -2, 0, 40, 40]
      return drawing
    })
    await editor.load()
    await editor.nudge(2, 0)
    expect((await bridge.nodeProps(id)).transform.slice(4)).toEqual([before[0], before[1]! - 1])
  })

  it('does not reuse a wheel anchor when zoom was already at its limit', async () => {
    const { shell, editor } = await setup()
    editor.setZoom(64)
    await flushPromises()
    const viewport = shell.get('[data-test="viewport"]').element as HTMLElement
    Object.defineProperty(viewport, 'clientWidth', { value: 800 })
    Object.defineProperty(viewport, 'clientHeight', { value: 600 })
    const stage = shell.get('[data-test="stage"]').element as HTMLElement
    stage.getBoundingClientRect = () => {
      const offset = stage.style.transform.match(/translate\(([-.\d]+)px, ([-.\d]+)px\)/)
      const width = parseFloat(stage.style.width)
      return { left: Math.max(0, (800 - width) / 2) + Number(offset?.[1] ?? 0),
        top: 48 + Number(offset?.[2] ?? 0), width, height: 320 * editor.zoom.value } as DOMRect
    }
    const before = stage.getBoundingClientRect()
    const point = [(400 - before.left) / 64, (300 - before.top) / 64]
    viewport.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, ctrlKey: true, clientX: 100, clientY: 200, deltaY: -100 }))
    await flushPromises()
    await shell.get('button[aria-label="Actual size"]').trigger('click')
    await flushPromises()
    const after = stage.getBoundingClientRect()
    expect(after.left + point[0]!).toBeCloseTo(400)
    expect(after.top + point[1]!).toBeCloseTo(300)
  })

  it('serializes rapid edits using the latest revision and supports undo/redo', async () => {
    const { editor, bridge } = await setup()
    editor.select(HEADER)
    await Promise.all([editor.renameSelected('One'), editor.renameSelected('Two'), editor.setCornerRadius(18)])
    expect(editor.failure.value).toBeNull()
    expect(editor.selected.value?.name).toBe('Two')
    expect(editor.selected.value?.props.cornerRadius).toBe(18)
    expect(editor.busy.value).toBe(false)
    await editor.undo()
    expect((await bridge.nodeProps(HEADER)).cornerRadius).toBe(8)
    await editor.redo()
    expect((await bridge.nodeProps(HEADER)).cornerRadius).toBe(18)
  })

  it('rolls back an invalid batch, including earlier commands', async () => {
    const bridge = new SampleEditorBridge()
    const before = await bridge.getView()
    await expect(bridge.apply({ expectedRevision: before.revision, commands: [
      { kind: 'rename-node', node: HEADER, name: 'Should not remain' },
      { kind: 'set-node-size', node: HEADER, size: [-1, 20] },
    ] })).rejects.toThrow()
    expect(await bridge.getView()).toEqual(before)
  })

  it('keeps an empty page editable after its last node is deleted', async () => {
    const { shell, editor } = await setup()
    editor.select(CARD)
    await editor.deleteSelected()
    expect(shell.find('[data-test="empty-state"]').exists()).toBe(false)
    expect(shell.find('[data-test="stage"]').exists()).toBe(true)
    await editor.createNode('shape')
    expect(editor.view.value?.nodes).toHaveLength(1)
    expect(editor.selected.value?.name).toBe('Shape')
  })

  it('creates a blank document and protects unsaved changes', async () => {
    const { editor } = await setup()
    editor.select(HEADER)
    await editor.renameSelected('Unsaved')
    editor.newDocument()
    expect(editor.awaitingConfirmation.value).toBe(true)
    editor.resolveConfirmation(false)
    expect(editor.selected.value?.name).toBe('Unsaved')
    editor.newDocument()
    editor.resolveConfirmation(true)
    await flushPromises()
    expect(editor.view.value?.pages).toHaveLength(1)
    expect(editor.view.value?.nodes).toHaveLength(0)
    expect(editor.canUndo.value).toBe(false)
  })

  it('edits text and type size through the inspector, then undoes it', async () => {
    const { shell, editor, bridge } = await setup()
    editor.select(LATIN)
    await flushPromises()
    await shell.get('textarea[aria-label="Text content"]').setValue('Hello\nمرحبا')
    await flushPromises()
    expect((await bridge.nodeProps(LATIN)).textContent).toBe('Hello\nمرحبا')
    await shell.get('input[aria-label="Font size"]').setValue('30')
    await flushPromises()
    expect((await bridge.nodeProps(LATIN)).fontSize).toBe(30)
    await editor.undo()
    expect((await bridge.nodeProps(LATIN)).fontSize).toBe(24)
  })

  it('rejects negative dimensions and restores the input', async () => {
    const { shell, editor, bridge } = await setup()
    editor.select(CARD)
    await flushPromises()
    const width = shell.get('input[aria-label="W"]')
    await width.setValue('-20')
    expect((width.element as HTMLInputElement).value).toBe('400')
    expect((await bridge.nodeProps(CARD)).size[0]).toBe(400)
  })

  it('allows removing and re-adding fill', async () => {
    const { shell, editor } = await setup()
    editor.select(HEADER)
    await flushPromises()
    await shell.findAll('button').find((button) => button.text() === 'Remove fill')!.trigger('click')
    await flushPromises()
    expect(editor.selected.value?.props.fill).toBeNull()
    await shell.findAll('button').find((button) => button.text() === 'Add fill')!.trigger('click')
    await flushPromises()
    expect(editor.selected.value?.props.fill).not.toBeNull()
  })

  it('drags a node at the current zoom and moves its children with it', async () => {
    const { shell, editor, bridge } = await setup()
    editor.setZoom(2)
    await flushPromises()
    const stage = shell.get('[data-test="stage"]').element
    pointer(stage, 'pointerdown', 90, 90)
    await flushPromises()
    pointer(stage, 'pointermove', 130, 150)
    pointer(stage, 'pointerup', 130, 150)
    await flushPromises()
    expect((await bridge.nodeProps(CARD)).transform.slice(4)).toEqual([60, 70])
    expect(editor.layout.value?.nodes.find((node) => node.id === HEADER)?.rect.slice(0, 2)).toEqual([76, 86])
    await editor.undo()
    expect((await bridge.nodeProps(CARD)).transform.slice(4)).toEqual([40, 40])
  })

  it('pans with Hand without changing the document', async () => {
    const { shell, editor } = await setup()
    const viewport = shell.get('[data-test="viewport"]').element as HTMLElement
    editor.activeTool.value = 'hand'
    viewport.scrollLeft = 200
    viewport.scrollTop = 150
    pointer(viewport, 'pointerdown', 100, 100)
    pointer(viewport, 'pointermove', 125, 135)
    pointer(viewport, 'pointerup', 125, 135)
    await flushPromises()
    expect((shell.get('[data-test="stage"]').element as HTMLElement).style.transform).toBe('translate(25px, 35px)')
    expect(editor.dirty.value).toBe(false)
  })

  it('resizes a selected layer with its canvas handle and can undo it', async () => {
    const { shell, editor, bridge } = await setup()
    editor.select(CARD)
    await flushPromises()
    const handle = shell.get('[data-resize]').element
    pointer(handle, 'pointerdown', 440, 280)
    await flushPromises()
    pointer(handle, 'pointermove', 500, 300)
    pointer(handle, 'pointerup', 500, 300)
    await flushPromises()
    expect((await bridge.nodeProps(CARD)).size).toEqual([460, 260])
    await editor.undo()
    expect((await bridge.nodeProps(CARD)).size).toEqual([400, 240])
  })

  it('does not delete background layers while a confirmation is open', async () => {
    const { shell, editor } = await setup()
    editor.select(HEADER)
    await editor.renameSelected('Keep')
    editor.openSample()
    await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete' }))
    await flushPromises()
    expect(editor.view.value?.nodes).toHaveLength(5)
    const cancel = shell.get('[data-test="confirm-cancel"]').element
    const accept = shell.get('[data-test="confirm-accept"]').element
    expect(document.activeElement).toBe(cancel)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', cancelable: true }))
    expect(document.activeElement).toBe(accept)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', cancelable: true }))
    expect(document.activeElement).toBe(cancel)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(editor.awaitingConfirmation.value).toBe(false)
    expect(editor.selected.value?.name).toBe('Keep')
  })

  it('downloads full document bytes before marking the document saved', async () => {
    const { editor, bridge } = await setup()
    editor.select(HEADER)
    await editor.renameSelected('Exported')
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
      expect(editor.dirty.value).toBe(true)
      expect(this.download).toBe('design.json')
      expect(this.href).toMatch(/^blob:/)
    })
    await editor.save()
    expect(click).toHaveBeenCalledOnce()
    expect(editor.dirty.value).toBe(false)
    const saved = JSON.parse(new TextDecoder().decode(await bridge.exportBytes()))
    expect(saved.schema_version).toBe(2)
    expect(saved.nodes.find((node: { id: string }) => node.id === HEADER).name).toBe('Exported')
    expect(saved.pages[0].roots).toEqual([CARD])
  })

  it('keeps dirty state when a download fails', async () => {
    const { editor } = await setup()
    editor.select(HEADER)
    await editor.renameSelected('Keep me')
    vi.spyOn(URL, 'createObjectURL').mockImplementation(() => { throw new Error('Download failed') })
    await editor.save()
    expect(editor.dirty.value).toBe(true)
    expect(editor.failure.value?.message).toBe('Download failed')
  })

  it('ignores a delayed hit test after a newer layer selection', async () => {
    const { editor, bridge } = await setup()
    let answer!: (value: { node: string; revision: number }) => void
    vi.spyOn(bridge, 'hitTest').mockImplementation(() => new Promise((resolve) => { answer = resolve }))
    const hit = editor.selectAt(60, 60)
    editor.select(LATIN)
    answer({ node: HEADER, revision: editor.revision.value })
    await hit
    expect(editor.selectedId.value).toBe(LATIN)
  })

  it('clears stale geometry when a refreshed layout fails', async () => {
    const { editor, bridge } = await setup()
    vi.spyOn(bridge, 'layout').mockRejectedValue(new Error('Layout failed'))
    editor.select(HEADER)
    await editor.renameSelected('Updated')
    expect(editor.layout.value).toBeNull()
    expect(editor.selected.value?.name).toBe('Updated')
    expect(editor.failure.value?.message).toBe('Layout failed')
  })

  it('finds matches under collapsed layers', async () => {
    const { shell } = await setup()
    await shell.get('button[aria-label="Collapse"]').trigger('click')
    expect(shell.findAll('[role="treeitem"]')).toHaveLength(1)
    await shell.get('#layer-search').setValue('Dot')
    expect(shell.findAll('[role="treeitem"]')).toHaveLength(2)
  })

  it('supports nullable page size and ignores non-finite zoom', async () => {
    const { editor, bridge } = await setup()
    await expect(bridge.layout(bridge.pageId, { ...PREVIEW_OPTIONS, pageSize: null })).resolves.toMatchObject({ pageSize: [480, 320] })
    editor.setZoom(NaN)
    expect(editor.zoom.value).toBe(1)
  })
  it('finishes a quick drag even when the hit test answers after pointer release', async () => {
    const { shell, editor, bridge } = await setup()
    let answer!: (value: { node: string; revision: number }) => void
    vi.spyOn(bridge, 'hitTest').mockImplementation(() => new Promise((resolve) => { answer = resolve }))
    const stage = shell.get('[data-test="stage"]').element
    pointer(stage, 'pointerdown', 45, 45)
    await flushPromises()
    pointer(stage, 'pointermove', 85, 75)
    pointer(stage, 'pointerup', 85, 75)
    answer({ node: CARD, revision: editor.revision.value })
    await flushPromises()
    expect((await bridge.nodeProps(CARD)).transform.slice(4)).toEqual([80, 70])
    expect(editor.canUndo.value).toBe(true)
  })

  it('cancels a pending drag with Escape without changing the document', async () => {
    const { shell, editor, bridge } = await setup()
    editor.select(CARD)
    let answer!: (value: { node: string; revision: number }) => void
    vi.spyOn(bridge, 'hitTest').mockImplementation(() => new Promise((resolve) => { answer = resolve }))
    const stage = shell.get('[data-test="stage"]').element
    pointer(stage, 'pointerdown', 45, 45)
    await flushPromises()
    pointer(stage, 'pointermove', 100, 100)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }))
    answer({ node: CARD, revision: editor.revision.value })
    pointer(stage, 'pointerup', 100, 100)
    await flushPromises()
    expect((await bridge.nodeProps(CARD)).transform.slice(4)).toEqual([40, 40])
    expect(editor.selectedId.value).toBe(CARD)
    expect(editor.dirty.value).toBe(false)
  })

  it('keeps every repeated arrow nudge while previous updates are pending', async () => {
    const { editor, bridge } = await setup()
    editor.select(CARD)
    for (let i = 0; i < 4; i++) window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', cancelable: true }))
    await editor.whenIdle()
    expect((await bridge.nodeProps(CARD)).transform[4]).toBe(44)
  })

  it('commits numeric fields with Enter once and cancels with Escape', async () => {
    const { shell, editor, bridge } = await setup()
    editor.select(CARD)
    await flushPromises()
    const width = shell.get('input[aria-label="W"]')
    ;(width.element as HTMLInputElement).value = '420'
    await width.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    expect((await bridge.nodeProps(CARD)).size[0]).toBe(420)
    const revision = editor.revision.value
    await width.trigger('change')
    await flushPromises()
    expect(editor.revision.value).toBe(revision)
    ;(width.element as HTMLInputElement).value = '900'
    await width.trigger('keydown', { key: 'Escape' })
    await width.trigger('change')
    await flushPromises()
    expect((width.element as HTMLInputElement).value).toBe('420')
    expect((await bridge.nodeProps(CARD)).size[0]).toBe(420)
    expect(editor.selectedId.value).toBe(CARD)
  })

  it('restores a cancelled name draft and preserves selection', async () => {
    const { shell, editor } = await setup()
    editor.select(CARD)
    await flushPromises()
    const name = shell.get('input[aria-label="Node name"]')
    ;(name.element as HTMLInputElement).value = 'Discard this'
    await name.trigger('keydown', { key: 'Escape' })
    await name.trigger('change')
    await flushPromises()
    expect((name.element as HTMLInputElement).value).toBe('Card')
    expect(editor.selectedId.value).toBe(CARD)
    expect(editor.dirty.value).toBe(false)
  })

  it('retains the complete previous frame until new layout is ready', async () => {
    const { editor, bridge } = await setup()
    const previous = editor.layout.value
    const layout = bridge.layout.bind(bridge)
    let complete!: () => Promise<void>
    vi.spyOn(bridge, 'layout').mockImplementation((page, options) => new Promise((resolve) => {
      complete = async () => resolve(await layout(page, options))
    }))
    editor.select(HEADER)
    const edit = editor.renameSelected('Ready together')
    await flushPromises()
    expect(editor.layout.value).toBe(previous)
    expect(editor.selected.value?.name).toBe('Header')
    await complete()
    await edit
    expect(editor.layout.value?.revision).toBe(editor.revision.value)
    expect(editor.selected.value?.name).toBe('Ready together')
  })

  it('coalesces repeated Save requests without redrawing the document', async () => {
    const { editor, bridge } = await setup()
    let finish!: (value: string) => void
    const saveDocument = vi.fn(() => new Promise<string>((resolve) => { finish = resolve }))
    Object.assign(bridge, { saveDocument })
    const layout = vi.spyOn(bridge, 'layout')
    const first = editor.save()
    const second = editor.save()
    await flushPromises()
    expect(saveDocument).toHaveBeenCalledOnce()
    finish('design.json')
    await Promise.all([first, second])
    expect(layout).not.toHaveBeenCalled()
  })

  it('zooms around the page point under the cursor', async () => {
    const { shell, editor } = await setup()
    const viewport = shell.get('[data-test="viewport"]').element as HTMLElement
    Object.defineProperty(viewport, 'clientWidth', { value: 800 })
    Object.defineProperty(viewport, 'clientHeight', { value: 600 })
    const stage = shell.get('[data-test="stage"]').element as HTMLElement
    stage.getBoundingClientRect = () => {
      const offset = stage.style.transform.match(/translate\(([-.\d]+)px, ([-.\d]+)px\)/)
      const width = parseFloat(stage.style.width)
      const left = Math.max(0, (800 - width) / 2) + Number(offset?.[1] ?? 0)
      const top = 48 + Number(offset?.[2] ?? 0)
      return { left, top, width, height: 320 * editor.zoom.value } as DOMRect
    }
    const before = stage.getBoundingClientRect()
    const pagePoint = [(300 - before.left) / editor.zoom.value, (200 - before.top) / editor.zoom.value]
    viewport.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, ctrlKey: true, clientX: 300, clientY: 200, deltaY: -Math.log(2) / 0.002 }))
    await flushPromises()
    const after = stage.getBoundingClientRect()
    expect(after.left + pagePoint[0]! * editor.zoom.value).toBeCloseTo(300)
    expect(after.top + pagePoint[1]! * editor.zoom.value).toBeCloseTo(200)
  })

})

import { afterEach, describe, expect, it, vi } from 'vitest'
import { SampleEditorBridge } from './sample'
import type { EditCommand, EditRequest } from './types'
import { TauriEditorBridge } from './tauri'
import { invoke } from '@tauri-apps/api/core'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
afterEach(() => { vi.clearAllMocks() })
const NODE = '018f0000-0000-7000-8000-000000000003'
const ASSET = '018f0000-0000-7000-8000-000000000004'
const style: EditCommand = { kind: 'set-node-text-style', node: NODE, fontFamily: 'Noto Sans Arabic', fontWeight: 900, align: 'center', direction: 'auto' }

async function blank() {
  const bridge = new SampleEditorBridge()
  await bridge.newDocument()
  const create = (nodeKind: string): EditCommand => ({ kind: 'create-node', node: NODE, nodeKind,
    name: 'Independent layer', fill: null, parent: { in: 'page-root', page: bridge.pageId } })
  const apply = async (commands: EditCommand[]) => bridge.apply({ expectedRevision: (await bridge.getView()).revision, commands })
  return { bridge, create, apply }
}

describe('native authoring contract', () => {
  it('creates and styles Arabic text in a batch and preserves style on export and undo/redo', async () => {
    const { bridge, create, apply } = await blank()
    await apply([create('text'), style, { kind: 'set-node-text', node: NODE, content: 'عرض — SALE', fontSize: 32 }])
    expect(await bridge.nodeProps(NODE)).toMatchObject({ fontFamily: 'Noto Sans Arabic', fontWeight: 900,
      textAlign: 'center', textDirection: 'auto', textContent: 'عرض — SALE', fontSize: 32 })
    const saved = JSON.parse(new TextDecoder().decode(await bridge.exportBytes()))
    expect(saved.nodes[0].props.text).toMatchObject({ font_weight: 900, align: 'center', direction: 'auto' })
    await bridge.undo((await bridge.getView()).revision)
    expect((await bridge.getView()).nodes).toEqual([])
    expect((await bridge.getView()).dirty).toBe(false)
    await bridge.redo((await bridge.getView()).revision)
    expect((await bridge.nodeProps(NODE)).fontWeight).toBe(900)
  })

  it.each([
    { fontWeight: 0 }, { fontWeight: 1001 }, { fontWeight: 400.5 }, { fontFamily: 'Unknown' },
    { align: 'justify' }, { direction: 'rtl' }, { direction: 'ltr' }, { direction: 'sideways' },
  ])('rejects unsupported text style %j without changing state or history', async (change) => {
    const { bridge, create, apply } = await blank()
    const before = await bridge.getView()
    await expect(apply([create('text'), { ...style, ...change } as EditCommand])).rejects.toMatchObject({
      code: ['rtl', 'ltr'].includes(String(change.direction)) ? 'command-rejected' : 'invalid-request', node: NODE,
    })
    expect(await bridge.getView()).toEqual(before)
  })

  it('rejects a wrong node type and an unavailable image atomically, and accepts explicit clearing', async () => {
    const { bridge, create, apply } = await blank()
    const before = await bridge.getView()
    await expect(apply([create('image'), style])).rejects.toMatchObject({ code: 'command-rejected' })
    await expect(apply([create('image'), { kind: 'set-node-image-asset', node: NODE, asset: ASSET }])).rejects.toMatchObject({ code: 'command-rejected' })
    expect(await bridge.getView()).toEqual(before)
    await apply([create('image'), { kind: 'set-node-image-asset', node: NODE, asset: null }])
    expect((await bridge.nodeProps(NODE)).imageAsset).toBeNull()
    const saved = JSON.parse(new TextDecoder().decode(await bridge.exportBytes()))
    expect(saved.nodes[0].props.image).toEqual({ asset: null })
  })

  it('reports the same commands and supported text limits as the native contract', async () => {
    const { bridge } = await blank()
    expect(await bridge.capabilities()).toMatchObject({ editableCommands: expect.arrayContaining(['set-node-text-style', 'set-node-image-asset']),
      textDirections: ['auto'], textAlignments: ['start', 'center', 'end'], fontWeightRange: [1, 1000] })
  })

  it('forwards both authoring commands through existing Tauri apply and retains structured refusals', async () => {
    const bridge = new TauriEditorBridge()
    const request: EditRequest = { expectedRevision: 7, commands: [style, { kind: 'set-node-image-asset', node: NODE, asset: ASSET }] }
    vi.mocked(invoke).mockResolvedValueOnce({ revision: 8 })
    await expect(bridge.apply(request)).resolves.toEqual({ revision: 8 })
    expect(invoke).toHaveBeenCalledWith('apply', { request })
    vi.mocked(invoke).mockRejectedValueOnce({ code: 'command-rejected', message: 'the asset is not registered', node: NODE })
    await expect(bridge.apply(request)).rejects.toMatchObject({ code: 'command-rejected', node: NODE })
  })
})

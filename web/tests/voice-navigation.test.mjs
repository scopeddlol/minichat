import { build } from 'esbuild'
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'

// Bundle the actual application modules, with only browser hardware mocked.
// This keeps the tests runnable on Node 22 without a test framework or browser.
globalThis.localStorage = {
  getItem: () => null,
  setItem() {},
  removeItem() {},
}
globalThis.document = { visibilityState: 'visible' }
const result = await build({
  stdin: {
    contents: `export { useStore } from './src/lib/store'; export { useInbox } from './src/lib/direct'; export { useVoice } from './src/lib/voice'; export { moveChannel } from './src/lib/channelOrder'`,
    resolveDir: fileURLToPath(new URL('..', import.meta.url)),
    loader: 'ts',
  },
  bundle: true,
  write: false,
  platform: 'node',
  format: 'esm',
  logLevel: 'silent',
})
const { useStore, useInbox, useVoice, moveChannel } = await import(
  `data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`
)

test('messages and mentions in the previous channel remain unread while in DMs', () => {
  useStore.setState({
    activeChannelId: 'general',
    me: { id: 'me' },
    messages: { general: [] },
    unread: {},
    mentionCounts: {},
  })
  useInbox.setState({ open: true })
  useStore.getState().applyEvent({
    t: 'MESSAGE_CREATE',
    d: { id: 'm1', channel_id: 'general', author: { id: 'other' } },
  })
  useStore.getState().applyEvent({ t: 'MENTION_ADD', d: { channel_id: 'general' } })
  useStore.getState().markRead('general')
  assert.equal(useStore.getState().unread.general, 1)
  assert.equal(useStore.getState().mentionCounts.general, 1)
  useStore.getState().setActiveChannel('general')
  assert.equal(useInbox.getState().open, false)
  assert.equal(useStore.getState().unread.general, 0)
})

test('dragging preserves channels and their relative order across categories', () => {
  const original = [
    { id: 'a', category_id: 'one', position: 0 },
    { id: 'b', category_id: 'one', position: 1 },
    { id: 'c', category_id: 'two', position: 2 },
  ]
  assert.deepEqual(
    moveChannel(original, 'a', 'two', 'c').map((c) => c.id),
    ['b', 'a', 'c'],
  )
  const moved = moveChannel(original, 'b', null, null)
  assert.deepEqual(
    moved.map((c) => c.id),
    ['a', 'c', 'b'],
  )
  assert.equal(moved[2].category_id, null)
  assert.equal(original[1].category_id, 'one')
  assert.deepEqual(
    moved.map((c) => c.position),
    [0, 1, 2],
  )
  assert.equal(moveChannel(original, 'a', 'one', 'a'), original)
})

test('camera toggle opens a picker without publishing; selection uses the exact device', async () => {
  const requests = []
  useVoice.setState({
    room: {
      localParticipant: {
        setCameraEnabled: async (...args) => requests.push(args),
      },
    },
    channelId: null,
    canVideo: true,
    cameraOn: false,
    cameraPickerOpen: false,
  })
  await useVoice.getState().toggleCamera()
  assert.equal(useVoice.getState().cameraPickerOpen, true)
  assert.equal(requests.length, 0)
  await useVoice.getState().startCamera('usb-camera')
  assert.deepEqual(requests[0], [true, { deviceId: { exact: 'usb-camera' } }])
  assert.equal(useVoice.getState().selectedDevices.videoinput, 'usb-camera')
  assert.equal(useVoice.getState().cameraOn, true)
})

test('failed device changes preserve the previous selection', async () => {
  useVoice.setState({
    room: { switchActiveDevice: async () => false },
    selectedDevices: {
      audioinput: 'old',
      audiooutput: 'default',
      videoinput: 'default',
    },
  })
  await assert.rejects(useVoice.getState().selectDevice('audioinput', 'missing'))
  assert.equal(useVoice.getState().selectedDevices.audioinput, 'old')
})

test('deafening silences other participants and restores the prior mic state', async () => {
  const volumes = [],
    mic = []
  useVoice.setState({
    room: {
      remoteParticipants: new Map([
        ['peer', { identity: 'peer', setVolume: (v) => volumes.push(v) }],
      ]),
      localParticipant: { setMicrophoneEnabled: async (v) => mic.push(v) },
    },
    canSpeak: true,
    channelId: null,
    muted: false,
    deafened: false,
    volumes: { peer: 0.7 },
  })
  await useVoice.getState().toggleDeafen()
  assert.equal(useVoice.getState().muted, true)
  await useVoice.getState().toggleDeafen()
  assert.equal(useVoice.getState().muted, false)
  assert.deepEqual(volumes, [0, 0.7])
  assert.deepEqual(mic, [false, true])
})

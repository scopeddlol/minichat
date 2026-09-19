import { test, expect } from '@playwright/test'

test.beforeEach(async ({ page }) => {
  await page.route('**/api/**', route => route.fulfill({ json: [] }))
  await page.goto('/tests/browser/harness.html')
})

test('channel and category menus open, stay open and edit', async ({ page }) => {
  await page.locator('[data-channel-id="general"]').click({ button: 'right' })
  await expect(page.getByRole('menu')).toBeVisible()
  await page.getByRole('menuitem', { name: 'Edit channel' }).click()
  await expect(page.getByRole('dialog')).toBeVisible()
  await page.keyboard.press('Escape')
  await page.getByRole('button', { name: 'Games', exact: true }).click({ button: 'right' })
  await page.getByRole('menuitem', { name: 'Edit category' }).click()
  await expect(page.getByRole('dialog')).toBeVisible()
})

test('message menu actions and member moderation are reachable', async ({ page }) => {
  await page.getByText('Hello world', { exact: true }).click({ button: 'right' })
  await expect(page.getByRole('menuitem', { name: 'Reply', exact: true })).toBeVisible()
  await page.getByRole('menuitem', { name: 'Edit', exact: true }).click()
  await expect(page.locator('textarea').filter({ hasText: 'Hello world' })).toBeVisible()
  await page.keyboard.press('Escape')
  await page.locator('.member-panel [data-user-id="peer"]:visible').click({ button: 'right' })
  await expect(page.getByRole('menuitem', { name: 'Roles & nickname' })).toBeVisible()
  await expect(page.getByRole('menuitem', { name: 'Kick member' })).toBeVisible()
  await expect(page.getByRole('menuitem', { name: 'Ban member' })).toBeVisible()
})

test('dragging a channel into an empty category persists the move', async ({ page }) => {
  let moved
  await page.route('**/api/channels/reorder', async route => { moved = route.request().postDataJSON(); await route.fulfill({ json: { ok: true } }) })
  await page.locator('[data-channel-id="general"]').dragTo(page.getByRole('button', { name: 'Games', exact: true }))
  await expect.poll(() => moved?.channels.find(c => c.id === 'general')?.category_id).toBe('category')
})

test('remote audio survives navigation and detaches on leave', async ({ page }) => {
  await page.evaluate(() => {
    window.attaches = 0; window.detaches = 0
    const track = { kind: 'audio', isLocal: false, attach(el) { window.attaches++; el.dataset.testAudio = 'remote' }, detach() { window.detaches++ } }
    const local = { kind: 'audio', isLocal: true, attach() { throw Error('Local mic must never play back') } }
    window.fixtureVoice.setState({ connected: true, channelId: 'voice', tracks: { 'peer:microphone': track, 'me:microphone': local } })
  })
  await expect(page.locator('audio[data-test-audio="remote"]')).toHaveCount(1)
  await page.locator('[data-channel-id="other"]').click()
  await expect(page.locator('audio[data-test-audio="remote"]')).toHaveCount(1)
  await page.evaluate(() => window.fixtureVoice.setState({ connected: false, channelId: null, tracks: {} }))
  await expect(page.locator('audio')).toHaveCount(0)
  await expect.poll(() => page.evaluate(() => window.attaches - window.detaches)).toBe(0)
})

test('voice tones fall back from an unplugged output', async ({ page }) => {
  await page.evaluate(() => {
    window.notes = 0; window.sinks = []
    window.AudioContext = class {
      currentTime = 0; destination = {}
      resume() { return Promise.resolve() }
      setSinkId(id) { window.sinks.push(id); return id ? Promise.reject(Error('Unplugged')) : Promise.resolve() }
      createOscillator() { return { frequency: {}, connect(gain) { return gain }, start() { window.notes++ }, stop() {}, disconnect() {} } }
      createGain() { return { gain: { setValueAtTime() {}, linearRampToValueAtTime() {}, exponentialRampToValueAtTime() {} }, connect() {}, disconnect() {} } }
    }
    for (const cue of ['join', 'leave', 'mute', 'unmute', 'deafen', 'undeafen']) window.playVoiceSound(cue, 'missing-headset')
  })
  await expect.poll(() => page.evaluate(() => window.notes)).toBe(14)
  expect(await page.evaluate(() => window.sinks.filter(id => id === '').length)).toBe(6)
})


test('keyboard menu opens on a channel and restores focus', async ({ page }) => {
  const row = page.locator('[data-channel-id="general"]')
  await row.focus()
  await page.keyboard.press('Shift+F10')
  await expect(page.getByRole('menu')).toBeVisible()
  await expect(page.getByRole('menuitem', { name: 'Copy link' })).toBeFocused()
  await page.keyboard.press('Escape')
  await expect(row).toBeFocused()
})

test('ordinary members cannot see channel or member administration', async ({ page }) => {
  await page.evaluate(async () => {
    const { useStore } = await import('/src/lib/store.ts')
    useStore.setState({ permissions: 0n, me: { ...useStore.getState().me, is_operator: false } })
  })
  await page.locator('[data-channel-id="general"]').click({ button: 'right' })
  await expect(page.getByRole('menuitem', { name: 'Copy link' })).toBeVisible()
  await expect(page.getByRole('menuitem', { name: 'Edit channel' })).toHaveCount(0)
  await page.keyboard.press('Escape')
  await page.locator('.member-panel [data-user-id="peer"]:visible').click({ button: 'right' })
  await expect(page.getByRole('menuitem', { name: 'View profile' })).toBeVisible()
  await expect(page.getByRole('menuitem', { name: 'Ban member' })).toHaveCount(0)
})

test('blocked autoplay offers a user gesture to resume call audio', async ({ page }) => {
  await page.evaluate(() => {
    const room = { canPlaybackAudio: false, on() {}, off() {}, async startAudio() { this.canPlaybackAudio = true } }
    window.fixtureVoice.setState({ room })
  })
  await page.getByRole('button', { name: 'Enable call audio' }).click()
  await expect(page.getByRole('button', { name: 'Enable call audio' })).toHaveCount(0)
})

test('member kick requires confirmation before sending the request', async ({ page }) => {
  let kicked = false
  await page.route('**/api/admin/members/peer', async route => {
    kicked = route.request().method() === 'DELETE'
    await route.fulfill({ json: { ok: true } })
  })
  await page.locator('.member-panel [data-user-id="peer"]:visible').click({ button: 'right' })
  await page.getByRole('menuitem', { name: 'Kick member' }).click()
  await expect(page.getByRole('dialog')).toBeVisible()
  expect(kicked).toBe(false)
  await page.getByRole('dialog').getByRole('button', { name: 'Kick', exact: true }).click()
  await expect.poll(() => kicked).toBe(true)
})

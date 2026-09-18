const invoke = window.__TAURI__.core.invoke
const $ = (id) => document.getElementById(id)
let sources = [],
  selected = null,
  kind = 'screen',
  version = 0
const urls = []
async function stop() {
  await invoke('stop_capture').catch(() => window.close())
}
$('close').onclick = stop
$('stop').onclick = stop
function render() {
  const current = ++version
  urls.splice(0).forEach(URL.revokeObjectURL)
  $('sources').replaceChildren()
  selected = null
  $('share').disabled = true
  for (const source of sources.filter((s) => s.kind === kind)) {
    const button = document.createElement('button')
    button.className = 'source'
    button.role = 'option'
    button.setAttribute('aria-selected', 'false')
    const image = document.createElement('img')
    image.alt = ''
    const name = document.createElement('span')
    name.textContent = source.name
    button.append(image, name)
    button.onclick = () => {
      selected = source
      document
        .querySelectorAll('.source')
        .forEach((b) => b.setAttribute('aria-selected', String(b === button)))
      $('share').disabled = false
    }
    $('sources').append(button)
    // Previews never leave this bundled desktop window.
    invoke('capture_thumbnail', { id: source.id })
      .then((bytes) => {
        if (current !== version) return
        const url = URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }))
        urls.push(url)
        image.src = url
      })
      .catch(() => {
        image.alt = 'Preview unavailable'
      })
  }
  if (!$('sources').children.length)
    $('sources').textContent = 'No available sources. Restore a window and refresh.'
}
document.querySelectorAll('[data-kind]').forEach(
  (button) =>
    (button.onclick = () => {
      kind = button.dataset.kind
      document
        .querySelectorAll('[data-kind]')
        .forEach((b) => b.setAttribute('aria-pressed', String(b === button)))
      render()
    }),
)
async function refresh() {
  $('refresh').disabled = true
  $('error').textContent = ''
  selected = null
  $('share').disabled = true
  version++
  $('sources').replaceChildren()
  try {
    sources = await invoke('capture_sources')
    render()
  } catch (e) {
    $('error').textContent = String(e)
  } finally {
    $('refresh').disabled = false
  }
}
$('refresh').onclick = refresh
$('share').onclick = async () => {
  if (!selected) return
  $('share').disabled = true
  $('tabs').inert = true
  $('sources').inert = true
  $('audio').disabled = true
  try {
    await invoke('capture_select', {
      id: selected.id,
      audio: $('audio').checked,
    })
    version++
    urls.splice(0).forEach(URL.revokeObjectURL)
    $('sources').replaceChildren()
    $('tabs').hidden = true
    $('tabs').style.display = 'none'
    $('hint').hidden = true
    $('audio-choice').hidden = true
    $('share').hidden = true
    $('stop').hidden = false
    document.body.classList.add('sharing')
    $('sharing').style.display = 'block'
    $('shared-name').textContent = selected.name
    $('detail').textContent = 'Screen capture is active'
  } catch (e) {
    $('error').textContent = String(e)
    $('share').disabled = false
    $('tabs').inert = false
    $('sources').inert = false
    $('audio').disabled = false
  }
}
void refresh()

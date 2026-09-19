import { useEffect, useRef, useState } from 'react'
import { Video } from 'lucide-react'
import { useVoice } from '../lib/voice'
import { Modal } from './ui'
import Select from './Select'

export default function CameraPicker() {
  const open = useVoice((s) => s.cameraPickerOpen)
  const devices = useVoice((s) => s.devices.videoinput)
  const saved = useVoice((s) => s.selectedDevices.videoinput)
  const [selected, setSelected] = useState(saved)
  const [error, setError] = useState('')
  const [ready, setReady] = useState(false)
  const [busy, setBusy] = useState(false)
  const video = useRef<HTMLVideoElement>(null)
  const stream = useRef<MediaStream | null>(null)
  const stop = () => {
    stream.current?.getTracks().forEach((t) => t.stop())
    stream.current = null
  }
  const close = () => {
    stop()
    useVoice.setState({ cameraPickerOpen: false })
  }
  useEffect(() => {
    if (open) {
      setSelected(saved)
      setError('')
    }
  }, [open, saved])
  useEffect(() => {
    if (!open) return
    let disposed = false
    setReady(false)
    setError('')
    stop()
    void navigator.mediaDevices
      .getUserMedia({
        audio: false,
        video: selected === 'default' ? true : { deviceId: { exact: selected } },
      })
      .then(async (media) => {
        if (disposed) {
          media.getTracks().forEach((t) => t.stop())
          return
        }
        stream.current = media
        if (video.current) {
          video.current.srcObject = media
          await video.current.play()
        }
        if (!disposed) setReady(true)
        await useVoice.getState().refreshDevices()
      })
      .catch((e) => {
        if (!disposed) setError(e instanceof Error ? e.message : 'Camera unavailable')
      })
    return () => {
      disposed = true
      stop()
    }
  }, [open, selected])
  return (
    <Modal
      open={open}
      onClose={close}
      title="Choose your camera"
      description="Preview your camera before anyone else sees it."
      width="sm"
      footer={
        <>
          <button className="btn btn-ghost" onClick={close}>
            Cancel
          </button>
          <button
            className="btn btn-primary"
            disabled={!ready || busy}
            onClick={async () => {
              const id = stream.current?.getVideoTracks()[0]?.getSettings().deviceId
              if (!id) return
              setBusy(true)
              stop()
              try {
                await useVoice.getState().startCamera(id)
              } catch (e) {
                setError(e instanceof Error ? e.message : 'Camera unavailable')
                setReady(false)
              } finally {
                setBusy(false)
              }
            }}
          >
            <Video size={15} />
            Start camera
          </button>
        </>
      }
    >
      <div className="p-5 space-y-4">
        <video
          ref={video}
          muted
          autoPlay
          playsInline
          className="w-full aspect-video rounded-xl object-contain bg-black"
        />
        <Select
          ariaLabel="Camera"
          value={selected}
          onChange={setSelected}
          options={[
            { value: 'default', label: 'System default' },
            ...devices
              .filter((d) => d.deviceId !== 'default')
              .map((d) => ({ value: d.deviceId, label: d.label })),
          ]}
        />
        {error && (
          <p role="alert" className="text-sm" style={{ color: 'var(--danger)' }}>
            {error}
          </p>
        )}
      </div>
    </Modal>
  )
}

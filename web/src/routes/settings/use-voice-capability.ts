import { useEffect, useState } from "react"
import { fetchTalkCapability, type TalkCapability } from "@/lib/talk-capability"

export function useVoiceCapability() {
  const [voiceCapability, setVoiceCapability] = useState<TalkCapability | null>(null)

  function loadVoiceCapability() {
    fetchTalkCapability().then(setVoiceCapability).catch(() => {})
  }

  useEffect(() => {
    loadVoiceCapability()
  }, [])

  return { voiceCapability, reloadVoiceCapability: loadVoiceCapability }
}

import { RemoteAccessPanel } from "@/components/auth/remote-access-panel"
import { useAuth } from "@/routes/auth-provider"
import { Section } from "./shared"

/** Remote access for this gateway: pairing codes and the devices already paired. */
export function PairingSection() {
  const auth = useAuth()

  return (
    <Section title="Pairing">
      <RemoteAccessPanel
        authState={auth.authState}
        devices={auth.devices}
        onCreatePairingCode={auth.createPairingCode}
        onLogout={auth.logout}
        onUnpairDevice={auth.unpairDevice}
      />
    </Section>
  )
}

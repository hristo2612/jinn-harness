import { useState, useSyncExternalStore } from "react"
import type { WorkspaceInfo } from "@/lib/api"
import { nativeGatewayProfiles } from "@/lib/native-gateway-bootstrap"
import { NativePairingDialog } from "@/components/auth/native-pairing-screen"

type LauncherProps = {
  className?: string
  workspaces: WorkspaceInfo[]
  onAdd: () => void
  onStart: (workspace: WorkspaceInfo) => void
  onOpen?: (workspace: WorkspaceInfo) => void
  onRemove?: (workspace: WorkspaceInfo) => void
  startingId?: string
  startError?: { id: string; message: string } | null
}

type Profiles = NonNullable<ReturnType<typeof nativeGatewayProfiles>>
type Snapshot = ReturnType<Profiles["snapshot"]>

function workspacesFrom(snapshot: Snapshot): WorkspaceInfo[] {
  return snapshot.profiles.map((profile) => ({
    id: profile.id,
    name: profile.name,
    displayName: profile.name,
    port: Number(new URL(profile.origin).port),
    running: true,
    current: profile.id === snapshot.activeId,
    switchUrl: profile.origin,
    warning: snapshot.failedProfileId === profile.id ? snapshot.error ?? "Unreachable" : undefined,
  }))
}

export function NativeWorkspaceSwitcher({
  className,
  Launcher,
}: {
  className?: string
  Launcher: React.ComponentType<LauncherProps>
}) {
  const profiles = nativeGatewayProfiles()!
  const snapshot = useSyncExternalStore(profiles.subscribe, profiles.snapshot, profiles.snapshot)
  const [pairing, setPairing] = useState(false)
  const [error, setError] = useState<{ id: string; message: string } | null>(null)
  const workspaces = workspacesFrom(snapshot)

  async function select(workspace: WorkspaceInfo) {
    setError(null)
    try {
      await profiles.select(workspace.id)
    } catch (reason) {
      setError({ id: workspace.id, message: reason instanceof Error ? reason.message : "Gateway is unreachable" })
    }
  }

  async function remove(workspace: WorkspaceInfo) {
    setError(null)
    try {
      await profiles.remove(workspace.id)
    } catch (reason) {
      setError({ id: workspace.id, message: reason instanceof Error ? reason.message : "Gateway could not be removed" })
    }
  }

  return (
    <>
      <Launcher
        className={className}
        workspaces={workspaces}
        onAdd={() => setPairing(true)}
        onStart={(workspace) => void select(workspace)}
        onOpen={(workspace) => void select(workspace)}
        onRemove={(workspace) => void remove(workspace)}
        startingId={snapshot.switchingProfileId}
        startError={error}
      />
      <NativePairingDialog open={pairing} onOpenChange={setPairing} />
    </>
  )
}

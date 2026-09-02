
import { type ReactNode } from "react"
import { QueryClientProvider } from '@tanstack/react-query'
import { queryClient } from '@/lib/query-client'
import { ThemeProvider } from "@/routes/providers"
import { SettingsProvider, DocumentTitle } from "@/routes/settings-provider"
import { useQueryInvalidation } from '@/hooks/use-query-invalidation'
import { EmojiFavicon } from '@/components/emoji-favicon'
import { GatewayProvider } from '@/hooks/use-gateway'
import { AuthGate, AuthProvider } from "@/routes/auth-provider"
import { TodoPrefixContext } from "@/components/chat/todo-prefix-context"
import { useTodoPrefixes } from "@/hooks/use-todo-prefixes"
import { PluginHostBridge } from "@/plugins/sdk/plugin-host-bridge"
import { PluginNotices } from "@/plugins/sdk/plugin-notices"
import { DiskPluginsBridge } from "@/plugins/disk-plugins-bridge"

function QueryInvalidationBridge() {
  useQueryInvalidation()
  return null
}

/** Which 3-letter prefixes name a live board, app-wide: a Todo id reads as a
 *  mention in a chat message, a Todo body, and a comment alike, so the answer
 *  cannot belong to one route. */
function TodoMentionPrefixes({ children }: { children: ReactNode }) {
  const prefixes = useTodoPrefixes()
  return <TodoPrefixContext.Provider value={prefixes}>{children}</TodoPrefixContext.Provider>
}

export function ClientProviders({ children }: { children: ReactNode }) {
  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <AuthProvider>
          <AuthGate>
            <SettingsProvider>
              <GatewayProvider>
                <TodoMentionPrefixes>{children}</TodoMentionPrefixes>
                <DocumentTitle />
                <EmojiFavicon />
                <QueryInvalidationBridge />
                {/* Before the host bridge, so the sink is registered by the
                    time a frame can route into it. */}
                <PluginNotices />
                <PluginHostBridge />
                {/* After the host bridge: a plugin's module body may read
                    host state the moment it evaluates. */}
                <DiskPluginsBridge />
              </GatewayProvider>
            </SettingsProvider>
          </AuthGate>
        </AuthProvider>
      </ThemeProvider>
    </QueryClientProvider>
  )
}

import { useQuery } from "@tanstack/react-query"
import { AREAS } from "@/contrib/types"
import { useContributions } from "@/contrib/use-contributions"
import { api } from "@/lib/api"
import { providedNavigationFor } from "./nav-provided"
import { foldNavigation, navigationDifference, navigationPayload, NAVIGATION_QUERY_KEY } from "./navigation-extension"

/** Both shell surfaces share one daemon fold. Reading the query's signal
 * makes cancellation discard responses started before an administration. */
export function useProvidedNavigation(notesEnabled: boolean) {
  useContributions(AREAS.sidebarNav)
  const base = providedNavigationFor(notesEnabled)
  const payload = navigationPayload(base)
  const query = useQuery({
    queryKey: [...NAVIGATION_QUERY_KEY, payload],
    queryFn: async ({ signal }) => {
      const result = await api.moment("ui", "after-build-navigation", payload)
      signal.throwIfAborted()
      return foldNavigation(base, result)
    },
    staleTime: 0,
    retry: false,
    refetchOnWindowFocus: "always",
    refetchOnMount: "always",
  })
  const navigation = query.isError ? base : query.data ?? base
  return {
    ...navigation,
    notice: query.isError ? `Customization unavailable: ${query.error.message}. Showing standard navigation.` : undefined,
    difference: query.isPending ? "Reading navigation…" : navigationDifference(base, navigation),
    refresh: query.refetch,
  }
}

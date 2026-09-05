import { AREAS } from "@/contrib/types"
import { useContributions } from "@/contrib/use-contributions"
import { providedNavigationFor, type ProvidedNavigation } from "./nav-provided"

/** `providedNavigationFor`, subscribed to the `sidebar.nav` area for the
 *  reason `useNavigation` gives: the derivation reads the registry itself. */
export function useProvidedNavigation(notesEnabled: boolean): ProvidedNavigation {
  useContributions(AREAS.sidebarNav)
  return providedNavigationFor(notesEnabled)
}

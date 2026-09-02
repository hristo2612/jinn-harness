import { AREAS } from "@/contrib/types"
import { useContributions } from "@/contrib/use-contributions"
import { navigationFor } from "./nav"

/**
 * `navigationFor`, subscribed to the `sidebar.nav` area.
 *
 * The subscription is the whole point and the returned snapshot is unused:
 * `navigationFor` reads the registry itself, so without a subscription a plugin
 * enabled after the rail mounted would sit in the registry unrendered until
 * something unrelated re-rendered it.
 */
export function useNavigation(notesEnabled: boolean): ReturnType<typeof navigationFor> {
  useContributions(AREAS.sidebarNav)
  return navigationFor(notesEnabled)
}

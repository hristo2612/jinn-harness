import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, it } from "vitest"
import { NavigationInspection } from "../navigation-extension"
afterEach(cleanup)
it.each(["ext/jinn-ext-js-boa", "custom/another-provider"])("qualifies preset access when the installed provider is %s", packageName => {
  render(<NavigationInspection document={{id:"ext-navigation", package:packageName, hash:"sha256:observed", config:{data:{topics:["jinn:ui/before-send"]}}}} runtime={{id:"ext-navigation",lifecycle:{state:"active"},grants:{source:"profile-document",values:[{contract:"jinn:ui/before-send"}],qualifier:"observed"}}} />)
  expect(screen.getByText(`Configured provider: ${packageName}`)).not.toBeNull()
  expect(screen.getByText(/Installed access may differ/)).not.toBeNull()
  expect(screen.queryByText(/It receives no profile contents, credentials or sessions/)).toBeNull()
})

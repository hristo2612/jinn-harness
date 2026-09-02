import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, type RenderResult } from "@testing-library/react"
import { MemoryRouter, useLocation } from "react-router-dom"
import { GlobalSearch, type GlobalSearchProps } from "@/components/global-search"

/** Where a row sent the operator, readable from the DOM. */
function LocationProbe() {
  return <div data-testid="location">{useLocation().pathname}</div>
}

export function renderOverlay(props: GlobalSearchProps = {}): RenderResult {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/"]}>
        <LocationProbe />
        <GlobalSearch initialOpen {...props} />
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

export function searchField(): HTMLInputElement {
  const field = document.querySelector<HTMLInputElement>("input[aria-label^='Search']")
  if (!field) throw new Error("the overlay is not open: no search field in the document")
  return field
}

import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouteLoading } from '../route-loading'

// Routes are code-split and every page mounts its own MobileTabBar, so this
// fallback is the committed frame BETWEEN two pages on a cold tab switch. It
// must carry the tab bar or the mobile nav visibly drops out mid-navigation
// (the bar's view-transition snapshot plays an exit fade over the old page).

function renderFallback() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <RouteLoading />
      </MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('RouteLoading', () => {
  it('announces the wait', () => {
    renderFallback()
    expect(screen.getByRole('status', { name: 'Loading page' })).toBeTruthy()
  })

  it('keeps the mobile tab bar mounted through the route load', () => {
    renderFallback()
    expect(screen.getByRole('navigation', { name: 'Primary' })).toBeTruthy()
  })
})

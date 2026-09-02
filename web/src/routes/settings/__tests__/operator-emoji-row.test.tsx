import { useSyncExternalStore } from 'react'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

/**
 * The operator emoji lives in gateway config, not localStorage, so the row is
 * only honest if a rejected write takes the optimistic local pick back with it
 * and says why — and if a write the operator has already replaced stays quiet.
 */

const apiMocks = vi.hoisted(() => ({ completeOnboarding: vi.fn() }))

/** Stands in for the settings provider's store, so the swatch reacts to the
 *  setter the way it does in the app instead of being frozen at render time. */
const store = vi.hoisted(() => {
  let emoji: string | null = null
  const listeners = new Set<() => void>()
  return {
    read: () => emoji,
    write: (next: string | null) => {
      emoji = next
      for (const listener of listeners) listener()
    },
    subscribe: (listener: () => void) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
  }
})
const setOperatorEmoji = vi.hoisted(() => vi.fn())

vi.mock('@/lib/api', () => ({ api: apiMocks }))
vi.mock('@/routes/settings-provider', () => ({
  useSettings: () => ({
    settings: { operatorEmoji: useSyncExternalStore(store.subscribe, store.read) },
    setOperatorEmoji,
  }),
}))
vi.mock('@/components/ui/emoji-picker', () => ({
  EmojiPicker: ({ onSelect }: { onSelect: (emoji: string) => void }) => (
    <>
      <button type="button" onClick={() => onSelect('🐼')}>pick panda</button>
      <button type="button" onClick={() => onSelect('🦊')}>pick fox</button>
    </>
  ),
}))

import { OperatorEmojiRow } from '../emoji-rows'

function pickPanda() {
  render(<OperatorEmojiRow />)
  fireEvent.click(screen.getByLabelText('Choose operator emoji'))
  fireEvent.click(screen.getByText('pick panda'))
}

/** A save the test settles by hand, so two of them can be in flight at once. */
function pendingSave() {
  let settle: { resolve: (value: unknown) => void; reject: (err: Error) => void }
  const promise = new Promise((resolve, reject) => {
    settle = { resolve, reject }
  })
  return { promise, ...settle! }
}

describe('OperatorEmojiRow', () => {
  beforeEach(() => {
    apiMocks.completeOnboarding.mockReset()
    setOperatorEmoji.mockReset()
    store.write(null)
    setOperatorEmoji.mockImplementation((emoji: string | null) => store.write(emoji))
  })

  it('rolls the pick back and names the failure when the gateway rejects it', async () => {
    apiMocks.completeOnboarding.mockRejectedValue(new Error('gateway offline'))

    pickPanda()

    await waitFor(() => expect(screen.getByRole('alert')).toBeTruthy())
    expect(screen.getByRole('alert').textContent).toContain('gateway offline')
    expect(setOperatorEmoji.mock.calls).toEqual([['🐼'], [null]])
  })

  it('keeps the pick and stays quiet when the gateway accepts it', async () => {
    apiMocks.completeOnboarding.mockResolvedValue({ status: 'ok', portal: {} })

    pickPanda()

    await waitFor(() => expect(apiMocks.completeOnboarding).toHaveBeenCalledWith({ operatorEmoji: '🐼' }))
    expect(setOperatorEmoji.mock.calls).toEqual([['🐼']])
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('leaves the newest pick alone when an older save fails after it', async () => {
    const fox = pendingSave()
    const panda = pendingSave()
    apiMocks.completeOnboarding.mockReturnValueOnce(fox.promise).mockReturnValueOnce(panda.promise)

    render(<OperatorEmojiRow />)
    const swatch = () => screen.getByLabelText('Choose operator emoji')

    fireEvent.click(swatch())
    fireEvent.click(screen.getByText('pick fox'))
    fireEvent.click(swatch())
    fireEvent.click(screen.getByText('pick panda'))

    await act(async () => {
      panda.resolve({ status: 'ok', portal: {} })
      await panda.promise
    })
    expect(swatch().textContent).toBe('🐼')

    await act(async () => {
      fox.reject(new Error('gateway offline'))
      await fox.promise.catch(() => {})
    })

    expect(swatch().textContent).toBe('🐼')
    expect(setOperatorEmoji.mock.calls).toEqual([['🦊'], ['🐼']])
    expect(screen.queryByRole('alert')).toBeNull()
  })
})

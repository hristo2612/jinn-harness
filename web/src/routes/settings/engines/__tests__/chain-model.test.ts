import { describe, expect, it } from 'vitest'
import {
  addOptionsFor,
  chainFor,
  classifyEngineHealth,
  legacyFallbackEngine,
  legacyMigrationMutations,
  moveInChain,
  removeFromChain,
} from '../chain-model'

describe('classifyEngineHealth', () => {
  const NOW = Date.parse('2026-08-19T10:00:00.000Z')
  const isoIn = (minutes: number) => new Date(NOW + minutes * 60_000).toISOString()

  it('reads an absent record as healthy', () => {
    expect(classifyEngineHealth(undefined)).toEqual({ tone: 'healthy', label: 'Healthy' })
  })

  it('reads an ok record as healthy', () => {
    expect(classifyEngineHealth({ state: 'ok' })).toEqual({ tone: 'healthy', label: 'Healthy' })
  })

  it('reads a reopening days out as the wait, never as a bare time of day', () => {
    expect(classifyEngineHealth({ state: 'exhausted', until: isoIn(6 * 24 * 60) }, NOW)).toEqual({
      tone: 'exhausted',
      label: 'Out of allowance, back in 6d',
    })
  })

  it('names the window that binds when the record carries one', () => {
    expect(classifyEngineHealth({ state: 'exhausted', until: isoIn(6 * 24 * 60), window: '7d' }, NOW).label)
      .toBe('Weekly limit, back in 6d')
    expect(classifyEngineHealth({ state: 'exhausted', until: isoIn(90), window: '5h' }, NOW).label)
      .toBe('5-hour limit, back in 2h')
  })

  it('reads a reopening within the hour in minutes, and one past a week as a date', () => {
    expect(classifyEngineHealth({ state: 'exhausted', until: isoIn(45) }, NOW).label)
      .toBe('Out of allowance, back in 45m')
    expect(classifyEngineHealth({ state: 'exhausted', until: isoIn(30 * 24 * 60) }, NOW).label)
      .toBe(`Out of allowance, back on ${new Date(NOW + 30 * 24 * 60 * 60_000).toLocaleDateString()}`)
  })

  it('reads exhausted without a reopening as exhausted with no time', () => {
    expect(classifyEngineHealth({ state: 'exhausted' })).toEqual({
      tone: 'exhausted',
      label: 'Out of allowance',
    })
  })

  it('drops a reopening that is not a date, or one already passed, rather than rendering it', () => {
    expect(classifyEngineHealth({ state: 'exhausted', until: 'soon' }).label).toBe('Out of allowance')
    expect(classifyEngineHealth({ state: 'exhausted', until: isoIn(-10) }, NOW).label).toBe('Out of allowance')
  })

  it('reads degraded as degraded', () => {
    expect(classifyEngineHealth({ state: 'degraded' })).toEqual({ tone: 'degraded', label: 'Degraded' })
  })
})

describe('addOptionsFor', () => {
  it('offers neither the card own engine nor one the chain already holds', () => {
    expect(addOptionsFor(['claude', 'codex', 'grok', 'pi'], 'claude', ['codex'])).toEqual(['grok', 'pi'])
  })

  it('offers nothing once every other engine is in the chain', () => {
    expect(addOptionsFor(['claude', 'codex'], 'claude', ['codex'])).toEqual([])
  })
})

describe('chainFor', () => {
  it('reads the chain an engine block carries', () => {
    expect(chainFor({ claude: { fallback: ['codex', 'grok'] } }, 'claude')).toEqual(['codex', 'grok'])
  })

  it('reads an engine with no block, and the `default` string key, as an empty chain', () => {
    expect(chainFor({ default: 'claude' }, 'claude')).toEqual([])
    expect(chainFor(undefined, 'claude')).toEqual([])
  })
})

describe('removeFromChain', () => {
  it('drops the entry at the index and keeps the rest in order', () => {
    expect(removeFromChain(['codex', 'grok', 'pi'], 1)).toEqual(['codex', 'pi'])
  })

  it('empties a one-entry chain to an array rather than to undefined', () => {
    expect(removeFromChain(['codex'], 0)).toEqual([])
  })
})

describe('moveInChain', () => {
  it('moves an entry later, keeping every other relative order', () => {
    expect(moveInChain(['a', 'b', 'c'], 0, 2)).toEqual(['b', 'c', 'a'])
  })

  it('moves an entry earlier', () => {
    expect(moveInChain(['a', 'b', 'c'], 2, 0)).toEqual(['c', 'a', 'b'])
  })

  it('returns the chain unchanged for an index outside it, dropping nothing', () => {
    expect(moveInChain(['a', 'b'], 0, 5)).toEqual(['a', 'b'])
    expect(moveInChain(['a', 'b'], -1, 1)).toEqual(['a', 'b'])
    expect(moveInChain(['a', 'b'], 1, 1)).toEqual(['a', 'b'])
  })
})

describe('legacyFallbackEngine', () => {
  it('reads the mapped engine when the strategy is fallback', () => {
    expect(legacyFallbackEngine({ rateLimitStrategy: 'fallback', fallbackEngine: 'codex' })).toBe('codex')
  })

  it('reads no mapping when the strategy is wait, absent, or names no engine', () => {
    expect(legacyFallbackEngine({ rateLimitStrategy: 'wait', fallbackEngine: 'codex' })).toBeNull()
    expect(legacyFallbackEngine(undefined)).toBeNull()
    expect(legacyFallbackEngine({ rateLimitStrategy: 'fallback' })).toBeNull()
  })

  it('reads no mapping once migration has nulled the pair', () => {
    expect(legacyFallbackEngine({ rateLimitStrategy: null, fallbackEngine: null })).toBeNull()
  })
})

describe('legacyMigrationMutations', () => {
  it('writes the mapped engine as claude chain', () => {
    expect(legacyMigrationMutations('codex')).toContainEqual({
      path: ['engines', 'claude', 'fallback'],
      value: ['codex'],
    })
  })

  it('sets both legacy keys to null, so the PUT deletes them rather than keeping them', () => {
    const mutations = legacyMigrationMutations('codex')
    const strategy = mutations.find((m) => m.path.join('.') === 'sessions.rateLimitStrategy')
    const engine = mutations.find((m) => m.path.join('.') === 'sessions.fallbackEngine')

    // `null` and not merely absent: the gateway's config merge keeps every key a
    // PUT omits, so an omitted key would leave the legacy pair on disk.
    expect(strategy).toBeDefined()
    expect(engine).toBeDefined()
    expect(strategy?.value).toBeNull()
    expect(engine?.value).toBeNull()
  })
})

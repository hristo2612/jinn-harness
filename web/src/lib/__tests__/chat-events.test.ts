import { describe, expect, it } from 'vitest'
import { applyChatEvents } from '../chat-events'
import { snapshot, type ChatSnapshot } from '../api-chat'

const saved: ChatSnapshot = { 'session-id': 'a', store: 'chat', status: 'running', turns: 1, 'event-after': 1,
  log: [{ 'turn-id': 't', seq: 0, status: 'running', message: 'hello', answer: 'A' }] }
const delta = (seq: number, text: string) => ({ store: 'chat', 'session-id': 'a', seq, kind: 'delta', 'turn-id': 't', text })
const page = (events: unknown[], cursor = 2, dropped = 0) => ({ 'session-id': 'a', events, 'next-after': cursor, dropped })

describe('ordered Chat partials and recovery', () => {
  it('uses the actual flattened session event and preserves repeated token text', () => {
    const next = applyChatEvents(saved, page([delta(2, 'A'), delta(3, 'A')], 3), 1)
    expect(next.snapshot.log[0].answer).toBe('AAA')
    expect(saved.log[0].answer).toBe('A')
  })
  it('ignores replayed sequence numbers, never repeated text', () => {
    const next = applyChatEvents(saved, page([delta(1, 'A'), delta(2, 'B')]), 1)
    expect(next.snapshot.log[0].answer).toBe('AB')
  })
  it('refuses a lost ring, wrong conversation, malformed data and cursor drift', () => {
    expect(() => applyChatEvents(saved, page([delta(4, 'lost')], 4, 4), 1)).toThrow('no longer')
    expect(() => applyChatEvents(saved, page([{ ...delta(2, 'x'), 'session-id': 'b' }]), 1)).toThrow('another')
    expect(() => applyChatEvents(saved, page([{ ...delta(2, 'x'), text: 4 }]), 1)).toThrow('Invalid')
    expect(() => applyChatEvents(saved, page([delta(2, 'x')], 9), 1)).toThrow('cursor')
  })
  it('allows an old dropped count once the authoritative watermark has caught up', () => {
    expect(applyChatEvents(saved, page([delta(2, 'B')], 2, 1), 1).snapshot.log[0].answer).toBe('AB')
  })
  it('reconciles terminal events instead of manufacturing completed text', () => {
    const next = applyChatEvents(saved, page([{ ...delta(2, ''), kind: 'turn-ended' }]), 1)
    expect(next.terminal).toBe(true)
    expect(next.snapshot.log[0]).toEqual(saved.log[0])
  })
  it('rejects incomplete authoritative histories', () => {
    expect(() => snapshot({ ...saved, turns: 2 }, 'a')).toThrow('incomplete')
    expect(() => snapshot(saved, 'b')).toThrow('match')
  })
})

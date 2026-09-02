/**
 * Fan a value out to a set of listeners, isolating each one.
 *
 * Plugin code sits on the far end of both fan-outs the host owns — gateway
 * frames and host state — so a handler that throws must not swallow the
 * handlers queued behind it, and must not surface as a failure inside the host
 * code that published. Iterating a copy is what keeps that true when a listener
 * unsubscribes itself, or another listener, mid-dispatch.
 */
export function notifyEach<T>(
  listeners: Iterable<(value: T) => void>,
  value: T,
  context: string,
): void {
  for (const listener of [...listeners]) {
    try {
      listener(value)
    } catch (error) {
      console.error(`[plugin-sdk] a ${context} listener threw; the listeners after it still ran`, error)
    }
  }
}

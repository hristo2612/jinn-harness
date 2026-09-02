import { type GatewaySocketConnection } from "./gateway-transport"

export class StaleGatewayGenerationError extends DOMException {
  constructor() {
    super("The gateway changed before this response arrived", "AbortError")
  }
}

/**
 * A socket that goes quiet the moment the gateway it was opened against stops
 * being the active one. `live` reports whether that generation still holds;
 * `release` lets the owner forget this socket once it can no longer deliver.
 */
export class GuardedSocket implements GatewaySocketConnection {
  binaryType: BinaryType = "blob"
  onopen: ((event: Event) => void) | null = null
  onmessage: ((event: MessageEvent) => void) | null = null
  onclose: ((event: CloseEvent) => void) | null = null
  onerror: ((event: Event) => void) | null = null
  readonly #listeners = new Set<(event: MessageEvent) => void>()

  constructor(
    private readonly inner: GatewaySocketConnection,
    private readonly live: () => boolean,
    private readonly release: () => void,
  ) {
    inner.onopen = (event) => { if (live()) this.onopen?.(event) }
    inner.onmessage = (event) => {
      if (!live()) return
      this.onmessage?.(event)
      for (const listener of this.#listeners) listener(event)
    }
    inner.onclose = (event) => {
      release()
      if (live()) this.onclose?.(event)
    }
    inner.onerror = (event) => { if (live()) this.onerror?.(event) }
  }

  get readyState() { return this.inner.readyState }

  addEventListener(type: "message", listener: (event: MessageEvent) => void): void {
    if (type === "message") this.#listeners.add(listener)
  }

  send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void {
    if (!this.live()) throw new StaleGatewayGenerationError()
    this.inner.send(data)
  }

  close(code?: number, reason?: string): void {
    this.release()
    this.inner.close(code, reason)
  }
}

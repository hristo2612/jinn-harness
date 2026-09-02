import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { authFetch } from "@/lib/auth";
import type { GatewayEvent, GatewayEventListener } from "@jinn/gateway-events";

export interface GatewayContextValue {
  events: GatewayEvent[];
  connected: boolean;
  connectionSeq: number;
  skillsVersion: number;
  subscribe: (fn: GatewayEventListener) => () => void;
}

const GatewayContext = createContext<GatewayContextValue | null>(null);

/** UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 3): no socket until UI-3.
 *  Gateway status is this poll of `GET /v1/health` through `authFetch`. */
const HEALTH_POLL_MS = 5_000;

async function healthAnswers(): Promise<boolean> {
  try {
    return (await authFetch("/v1/health", { method: "GET" })).ok;
  } catch {
    return false;
  }
}

/**
 * The one gateway status per app, exposed via context. The WebSocket is NOT
 * opened: `events` stays empty, `skillsVersion` stays 0, `subscribe` hands back
 * a no-op unsubscribe, and `connectionSeq` moves when the poll flips to
 * connected. Every consumer reads from here — never polls on its own.
 */
export function GatewayProvider({ children }: { children: ReactNode }) {
  const [events] = useState<GatewayEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const [connectionSeq, setConnectionSeq] = useState(0);
  const [skillsVersion] = useState(0);
  const wasConnected = useRef(false);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      const ok = await healthAnswers();
      if (!alive) return;
      if (ok && !wasConnected.current) setConnectionSeq((prev) => prev + 1);
      wasConnected.current = ok;
      setConnected(ok);
      timer = setTimeout(() => void poll(), HEALTH_POLL_MS);
    };
    void poll();
    return () => {
      alive = false;
      if (timer) clearTimeout(timer);
    };
  }, []);

  const subscribe = useCallback((_fn: GatewayEventListener) => () => {}, []);

  return (
    <GatewayContext.Provider
      value={{ events, connected, connectionSeq, skillsVersion, subscribe }}
    >
      {children}
    </GatewayContext.Provider>
  );
}

/**
 * Consumer hook. Returns the same shape callers already expect.
 * Does NOT open a WebSocket — that's GatewayProvider's job.
 */
export function useGateway(): GatewayContextValue {
  const ctx = useContext(GatewayContext);
  if (!ctx) {
    throw new Error(
      "useGateway must be used inside <GatewayProvider> (mounted in ClientProviders).",
    );
  }
  return ctx;
}

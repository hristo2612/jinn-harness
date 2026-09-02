import type { Runtime } from "./contracts"
import { nativeBridge } from "./native-bridge"

function detectOs(userAgent: string): Runtime["os"] {
  if (/Android/i.test(userAgent)) return "android"
  if (/iPhone|iPad|iPod/i.test(userAgent)) return "ios"
  if (/Windows/i.test(userAgent)) return "windows"
  if (/Macintosh|Mac OS X/i.test(userAgent)) return "macos"
  if (/Linux/i.test(userAgent)) return "linux"
  return "unknown"
}

function detectEngine(userAgent: string): Runtime["engine"] {
  if (/Firefox/i.test(userAgent)) return "gecko"
  if (/AppleWebKit/i.test(userAgent) && !/Chrome|Chromium|Edg/i.test(userAgent)) return "webkit"
  if (/Chrome|Chromium|Edg/i.test(userAgent)) return "blink"
  return "unknown"
}

export function detectRuntime(): Runtime {
  if (typeof window === "undefined" || typeof navigator === "undefined") {
    return {
      container: "browser",
      os: "unknown",
      engine: "unknown",
      secureContext: false,
      appVersion: "unknown",
      userAgent: "unknown",
    }
  }

  const userAgent = navigator.userAgent
  const tauri = nativeBridge()?.runtime === "tauri"
  const standalone = window.matchMedia?.("(display-mode: standalone)").matches === true
  return {
    container: tauri ? "tauri" : standalone ? "pwa" : "browser",
    os: detectOs(userAgent),
    engine: detectEngine(userAgent),
    secureContext: window.isSecureContext,
    appVersion: document.documentElement.dataset.appVersion ?? "unknown",
    userAgent,
  }
}

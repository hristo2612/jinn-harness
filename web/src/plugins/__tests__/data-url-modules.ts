import { vi } from 'vitest'

/**
 * vitest's module runner resolves a dynamic `import()` itself and cannot reach a
 * `blob:` URL, so these tests swap the object-URL factory for one that hands
 * back an equivalent `data:` URL. Everything else is the shipped path: the same
 * rewrite, the same native `import()`, the same evaluation and the same
 * revocation. Only the scheme differs, and the difference belongs to the test
 * environment rather than to the loader.
 */
class RecordedBlob extends Blob {
  /** The parts, kept because a Blob only reads back asynchronously and
   *  `createObjectURL` has to answer now. */
  readonly source: string

  constructor(parts: BlobPart[], options?: BlobPropertyBag) {
    super(parts, options)
    this.source = parts.join('')
  }
}

export interface ModuleUrls {
  /** Every source handed to `createObjectURL`, newest last. */
  created: string[]
  revoked: string[]
}

export function useDataUrlModules(): ModuleUrls {
  const urls: ModuleUrls = { created: [], revoked: [] }

  vi.stubGlobal('Blob', RecordedBlob)
  URL.createObjectURL = (blob: Blob) => {
    const { source } = blob as RecordedBlob
    urls.created.push(source)
    return `data:text/javascript,${encodeURIComponent(source)}`
  }
  URL.revokeObjectURL = (url: string) => void urls.revoked.push(url)

  return urls
}

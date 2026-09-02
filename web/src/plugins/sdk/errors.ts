/**
 * The SDK's own error types, in a module of their own so the host and the
 * permission gate can both raise them without importing each other. `host.ts`
 * re-exports `PluginSdkError`, which is the name a plugin author knows.
 */

/** Every failure the SDK raises, named so a plugin author can tell an SDK
 *  problem from one of their own. */
export class PluginSdkError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'PluginSdkError'
  }
}

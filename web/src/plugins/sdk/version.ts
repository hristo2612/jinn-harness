/**
 * The version of the plugin contract, bumped by hand whenever `sdk.d.ts`
 * changes shape.
 *
 * It is deliberately not the app's version: a plugin is written against a
 * contract, not against a release, and the two move at different speeds. A
 * plugin that reads this can refuse to load against a contract it predates.
 */
export const SDK_CONTRACT_VERSION = '1.2.0'

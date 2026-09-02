import tseslint from "typescript-eslint"

import baseline from "./eslint-baseline.json" with { type: "json" }

// Type-aware lint needs every linted file to belong to a tsconfig. These trees
// are exactly the ones their package tsconfigs already cover, so the project
// service finds each file's project by walking up to the package root.
const PACKAGE_TREES = [
  "src/**/*.{ts,tsx}",
  // The package's own configs (vite, vitest) sat inside `packages/web/**` at the sha.
  "*.ts",
]

// The trees `tsconfig.e2e.json` and `tsconfig.scripts.json` cover. The project
// service cannot find those two: it resolves by walking up for a file named
// `tsconfig.json`, and this repo has none at the root. They are named
// explicitly below instead.
const ROOT_TREES = [
  "scripts/**/*.mjs",
]

export const LINTED = [...PACKAGE_TREES, ...ROOT_TREES]

// Every tree in `LINTED` names its tests, because a test left out of this list
// is a test judged by the size caps the ones beside it are excused from. Each
// entry stays inside a `LINTED` tree on purpose: a file that matches here but
// not there would be linted by this block alone, with neither the parser nor
// the plugin its rules need.
const TESTS = [
  "**/__tests__/**/*.{ts,tsx}",
  "**/*.test.{ts,tsx}",
]

/**
 * The floor. Every one of these is an error, never a warning, because a warning
 * is a number that grows.
 *
 * When one fires, the fix is to change the code, not the config. Split the
 * function at its seams into named helpers; give the branchy part its own
 * function with an early return per case; hoist a nested loop or condition into
 * a helper so the body reads flat; group related parameters into one options
 * object; await the promise, or hand it to `void` if the result is genuinely
 * not wanted. `eslint-baseline.json` grandfathers code that predates this
 * config and is the only place a violation may be recorded — `noInlineConfig`
 * below makes ESLint ignore directive comments entirely, so no file can excuse
 * itself.
 *
 * @type {import("eslint").Linter.RulesRecord}
 */
const CAPS = {
  complexity: ["error", { max: 10 }],
  "max-depth": ["error", { max: 3 }],
  "max-lines-per-function": ["error", { max: 50 }],
  "max-params": ["error", { max: 5 }],
  "@typescript-eslint/no-floating-promises": "error",
  // The `_name` prefix is this codebase's existing marker for a binding a
  // signature forces to exist and the body has no use for. Honouring it keeps
  // the rule pointed at real dead code.
  "@typescript-eslint/no-unused-vars": [
    "error",
    { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" },
  ],
}

export const CAPPED_RULES = Object.keys(CAPS)

// `describe` and `it` bodies are functions, so the two size/shape caps would
// make every long test file a violator for no signal — nobody splits a test
// because its `describe` block is long. The other four stay on: an un-awaited
// assertion is a real bug and a classic source of flakes.
const CAPS_OFF_IN_TESTS = ["complexity", "max-lines-per-function"]

/**
 * The rules themselves, without the grandfather overrides. `scripts/eslint-baseline.mjs`
 * re-applies exactly this to measure the tree as it would be judged with an
 * empty baseline.
 */
export const enforcement = tseslint.config(
  {
    files: LINTED,
    extends: [tseslint.configs.base],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    linterOptions: {
      // Every `eslint-disable` in these trees is ignored, so the grandfather
      // file is the only suppression channel — by construction rather than by
      // intention. A rule that polices disable comments cannot do this job: it
      // is itself named in a disable comment, and ESLint suppresses a report
      // that lands on the directive that silenced it, so the police disable
      // themselves. This is a linter option, and no source file can reach it.
      noInlineConfig: true,
    },
    rules: CAPS,
  },
  {
    files: ROOT_TREES,
    languageOptions: {
      parserOptions: {
        // `languageOptions` merges rather than replaces, so the block above's
        // `projectService: true` wins unless it is switched off here — and with
        // it on, every file in these trees fails at parse with "was not found by
        // the project service". `allowDefaultProject` is not the way out: these
        // trees nest, and a `**` glob is rejected outright as too wide.
        projectService: false,
        project: ["./tsconfig.json"],
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
  {
    files: TESTS,
    rules: {
      ...Object.fromEntries(CAPS_OFF_IN_TESTS.map((rule) => [rule, "off"])),
      // A bare `test()` from `node:test` returns a promise the runner itself
      // owns and awaits; nobody writes `await test(...)`. Reporting it is an
      // idiom mismatch, not the un-awaited assertion the rule exists to catch,
      // so the exemption is scoped to calls that resolve to `node:test` — a
      // vitest `it()` is a different declaration and stays covered.
      "@typescript-eslint/no-floating-promises": [
        "error",
        {
          allowForKnownSafeCalls: [
            {
              from: "package",
              package: "node:test",
              name: ["after", "afterEach", "before", "beforeEach", "describe", "it", "test"],
            },
          ],
        },
      ],
    },
  },
)

/**
 * One override per grandfathered file+rule, so a file excused for `complexity`
 * still fails on `max-params`.
 */
const grandfathered = Object.entries(baseline.files).map(([file, rules]) => ({
  files: [file],
  rules: Object.fromEntries(rules.map((rule) => [rule, "off"])),
}))

export default [
  {
    ignores: [
      "**/dist/**",
      "**/out/**",
      "**/coverage/**",
      "**/test-results/**",
      "**/.turbo/**",
      // `.mjs` is absent: it is the extension the root trees are written in, and
      // a global ignore outranks every `files` entry, so listing it here would
      // mask `LINTED` rather than complement it. A stray `.mjs` outside those
      // trees matches no `files` glob and is skipped anyway.
      "**/*.{js,cjs}",
    ],
  },
  ...enforcement,
  ...grandfathered,
]

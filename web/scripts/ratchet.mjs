#!/usr/bin/env node
// A ratchet on file size. Every file this repo lints, builds or ships is capped
// at 300 lines; the ones already past that carry a recorded budget instead, and
// a budget may only shrink. size-baseline.json is that record, committed so a
// raised budget arrives as a diff a reviewer sees rather than as a threshold
// nobody reads.
//
// It mirrors the ESLint grandfather list too: a file that gains an exemption in
// eslint-baseline.json without gaining one here fails, so the two lists cannot
// drift apart and neither can grow quietly.
//
// `pnpm ratchet` rewrites the baseline downward and is the only thing that ever
// writes it. `pnpm ratchet --check` is the CI gate: it never writes, and it also
// fails when the committed baseline is looser than the tree. Without that half,
// the file only ever drifts loose and the ratchet never actually tightens.
// Re-seed only from main after the merge queue drains: a snapshot taken on a build branch misses files landing on siblings merged around it, which surface later as violations nobody caused.
import { execFileSync } from "node:child_process"
import { existsSync, readFileSync, writeFileSync } from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const BASELINE_FILE = "size-baseline.json"
const ESLINT_BASELINE_FILE = "eslint-baseline.json"
const DEFAULT_LIMIT = 300

const CODE_EXTENSIONS = new Set([".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"])

// The trees the repo lints, builds or ships. `packages/*/template/` is absent by
// construction — it is copied verbatim into a new instance, frozen migrations
// included, so its size is not ours to police. Build output needs no exclusion:
// none of it is tracked, and this reads `git ls-files`.
const SCANNED_TREES = [
  /^src\//,
  /^scripts\//,
]

const SPLIT_REMEDIATION =
  "Split the file by responsibility — move one concern into its own module and import it back."

const EXEMPTION_REMEDIATION =
  "Fix the rule where it fires, or split the file by responsibility so the branchy part gets its own module. The grandfather list may only shrink."

const INSTRUCTIONS =
  "Every entry is a file that predates the 300-line limit, carries an ESLint grandfather exemption, or both. " +
  "`lines` is a budget that may only shrink: growing past it fails CI, and so does a new file over the limit or a " +
  "new entry in eslint-baseline.json. To leave the list, split the file by responsibility — move one concern into " +
  "its own module and import it back — then run `pnpm ratchet` and commit the shrunken file. Add nothing here by " +
  "hand; the ratchet never widens a budget."

function git(...args) {
  // quotePath off so a path with a non-ASCII character arrives as itself rather
  // than as an escape sequence no tree pattern would match.
  return execFileSync("git", ["-c", "core.quotePath=false", ...args], {
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  })
}

function isScanned(relPath) {
  return CODE_EXTENSIONS.has(path.extname(relPath)) && SCANNED_TREES.some((tree) => tree.test(relPath))
}

/** Physical lines. A trailing newline terminates the last line, it does not start another. */
function countLines(text) {
  const lines = text.split("\n")
  if (lines[lines.length - 1] === "") lines.pop()
  return lines.length
}

/** Every scanned file that is both tracked and present, keyed by POSIX path. */
function measureTree() {
  const sizes = new Map()
  for (const file of git("ls-files").split("\n").filter(Boolean)) {
    if (!isScanned(file)) continue
    let text
    try {
      text = readFileSync(file, "utf8")
    } catch (error) {
      // Tracked but gone from the working tree: this change deletes it. A
      // deletion is the one drift the check tolerates, because forcing a
      // baseline regeneration onto every branch that removes a file is noise.
      if (error.code === "ENOENT") continue
      throw error
    }
    sizes.set(file, countLines(text))
  }
  return sizes
}

function readExemptions() {
  let raw
  try {
    raw = readFileSync(ESLINT_BASELINE_FILE, "utf8")
  } catch (error) {
    throw new Error(
      `${ESLINT_BASELINE_FILE} could not be read (${error.message}). The size ratchet mirrors the ESLint grandfather list and cannot run without it.`,
    )
  }
  return new Map(Object.entries(JSON.parse(raw).files))
}

/** null when the baseline has never been written — the seed case, and the only run that enforces nothing. */
function readBaseline() {
  if (!existsSync(BASELINE_FILE)) return null
  const parsed = JSON.parse(readFileSync(BASELINE_FILE, "utf8"))
  return { limit: parsed.limit ?? DEFAULT_LIMIT, files: parsed.files }
}

/**
 * Two kinds of difference. A `violation` asks the baseline to widen, which
 * nothing may do. A `tightening` is the tree having shrunk below its record,
 * which `pnpm ratchet` writes down and `--check` reports as stale.
 */
function compare(sizes, exemptions, baseline) {
  const violations = []
  const tightenings = []
  for (const [file, lines] of sizes) {
    const entry = baseline.files[file]
    const recordedExempt = entry?.exempt ?? []
    const actualExempt = exemptions.get(file) ?? []
    const budget = entry?.lines ?? baseline.limit

    if (lines > budget) {
      const problem =
        entry?.lines === undefined
          ? `exceeds the ${baseline.limit}-line limit: ${lines} lines (+${lines - baseline.limit})`
          : `grew past its baseline: ${entry.lines} → ${lines} lines (+${lines - entry.lines})`
      violations.push({ file, problem, fix: SPLIT_REMEDIATION })
    } else if (entry?.lines !== undefined && lines < entry.lines) {
      tightenings.push({ file, change: `shrank: ${entry.lines} → ${lines} lines (${lines - entry.lines})` })
    }

    const gained = actualExempt.filter((rule) => !recordedExempt.includes(rule))
    if (gained.length > 0) {
      violations.push({
        file,
        problem: `gained a lint exemption the size baseline does not record: ${gained.map((rule) => `+${rule}`).join(", ")}`,
        fix: EXEMPTION_REMEDIATION,
      })
    }

    const dropped = recordedExempt.filter((rule) => !actualExempt.includes(rule))
    if (dropped.length > 0) tightenings.push({ file, change: `no longer exempt from ${dropped.join(", ")}` })
  }
  return { violations, tightenings }
}

/**
 * Only reached once a run is violation-free, so every file sits at or under its
 * budget and its exemptions are a subset of the record: the tree itself is then
 * the tightest honest baseline. A file under the limit gets no `lines`, so
 * ordinary edits to a grandfathered small file do not churn this file.
 */
function rebuild(sizes, exemptions, limit) {
  const files = {}
  for (const file of [...sizes.keys()].sort()) {
    const lines = sizes.get(file)
    const exempt = exemptions.get(file) ?? []
    if (lines <= limit && exempt.length === 0) continue
    const entry = lines > limit ? { lines } : {}
    if (exempt.length > 0) entry.exempt = [...exempt].sort()
    files[file] = entry
  }
  return files
}

function writeBaseline(limit, files) {
  writeFileSync(BASELINE_FILE, `${JSON.stringify({ _instructions: INSTRUCTIONS, limit, files }, null, 2)}\n`)
}

const plural = (count, noun, plurally = `${noun}s`) => `${count} ${count === 1 ? noun : plurally}`

function budgetedLines(files) {
  return Object.values(files).reduce((total, entry) => total + (entry.lines ?? 0), 0)
}

function report(violations, tightenings) {
  for (const { file, problem, fix } of violations) {
    console.log(file)
    console.log(`  ${problem}`)
    console.log(`  fix: ${fix}`)
    console.log("")
  }
  if (tightenings.length > 0) {
    console.log("The committed baseline is looser than the tree:")
    for (const { file, change } of tightenings) console.log(`  ${file}  ${change}`)
    console.log(`  fix: run \`pnpm ratchet\` and commit size-baseline.json to bank the shrink. ${SPLIT_REMEDIATION}`)
    console.log("")
  }
}

function listBaseline(baseline) {
  const budgeted = Object.entries(baseline.files)
    .filter(([, entry]) => entry.lines !== undefined)
    .sort(([, a], [, b]) => b.lines - a.lines)
  for (const [file, entry] of budgeted) console.log(`${String(entry.lines).padStart(6)}  ${file}`)
  console.log(`${plural(budgeted.length, "file")} over the ${baseline.limit}-line limit, largest first.`)
}

function parseMode(argv) {
  if (argv.includes("--list")) return "list"
  if (argv.includes("--check")) return "check"
  return "write"
}

function writeTightened(sizes, exemptions, limit) {
  const files = rebuild(sizes, exemptions, limit)
  writeBaseline(limit, files)
  console.log(`${BASELINE_FILE}: ${plural(Object.keys(files).length, "file")}, ${budgetedLines(files)} budgeted lines`)
}

function main() {
  const mode = parseMode(process.argv.slice(2))

  // The web tree is one directory of a larger repository: the run reads from
  // beside this script, and `git ls-files` reports paths relative to there.
  process.chdir(path.resolve(path.dirname(fileURLToPath(import.meta.url)), ".."))

  const baseline = readBaseline()
  if (mode === "list") {
    if (!baseline) throw new Error(`${BASELINE_FILE} does not exist yet. Run \`pnpm ratchet\` to create it.`)
    listBaseline(baseline)
    return 0
  }

  const sizes = measureTree()
  const exemptions = readExemptions()

  if (!baseline) {
    if (mode === "check") {
      throw new Error(`${BASELINE_FILE} is missing. Run \`pnpm ratchet\` and commit it.`)
    }
    // The first write has nothing to ratchet against, so it records the tree as
    // it stands. Deleting the file to reach this path is itself a visible diff.
    writeTightened(sizes, exemptions, DEFAULT_LIMIT)
    return 0
  }

  const { violations, tightenings } = compare(sizes, exemptions, baseline)
  if (violations.length > 0 || (mode === "check" && tightenings.length > 0)) {
    report(violations, tightenings)
    console.log(
      `size ratchet: ${plural(violations.length, "violation")}, ` +
        `${plural(tightenings.length, "stale entry", "stale entries")} in ${plural(sizes.size, "file")} scanned`,
    )
    if (mode === "write") console.log("Nothing written: the ratchet only tightens.")
    return 1
  }

  if (mode === "check") {
    console.log(
      `size ratchet: ${plural(sizes.size, "file")} scanned, ` +
        `${plural(Object.keys(baseline.files).length, "baselined file")}, ` +
        `${budgetedLines(baseline.files)} budgeted lines (limit ${baseline.limit})`,
    )
    return 0
  }

  writeTightened(sizes, exemptions, baseline.limit)
  return 0
}

process.exitCode = main()

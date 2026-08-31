# The plugins seam

The plugin tree itself as a capability on the kernel — the seventh and
LAST core-port seam under the malleability contract (phase 2.7). Every
seam before this one proved a provider is swappable **in a test**. This
one makes the tree legible and operable through the surface a person or
an agent actually uses, which is the North Star sentence itself: a kernel
that makes a machine *legible, reversible, and safe for an agent to
operate and reshape*.

Roles per the seam-triple naming law (AGENTS.md):

| Role | Package | What it is |
|---|---|---|
| Service definition | `jinn-plugins` | The `jinn:plugins.<catalog-id>` contract: the LIFECYCLE READING law and its legal-transition table, the grant reading and its source, the ledger attribution rule, the read window every answer carries, and the `list`/`describe`/`history`/`describe-catalog` operations. Owns the `plugins` settings namespace. Pure types + logic. |
| Provider | `jinn-plugins-profile` | The LIVE catalog: the entry set derived from the document of record through `jinn:profile`'s read view, so its grant lists are the authority the kernel enforces. |
| Provider | `jinn-plugins-static` | The FIXED catalog: the entry set declared in its own config, for tests and for a read-only appliance. Holds no `jinn:profile` grant at all. |
| Consumer | `jinn-api-http` (`plugins/api/`) | Exposes the tree over the operator API: the catalogs, one catalog's plugins, one plugin, one plugin's ledger lines. |

Neither provider knows about todos, sessions, engines or workflows, and
neither is granted any of their contracts (`tools/plugin-kit`'s authority
tests check it rather than asserting it in prose).

## The catalog is the swappable part; the reading is not

A catalog answers WHICH plugins there are and what each declares. The
lifecycle, the provisions and the ledger lines are read from the kernel by
both providers through the same code in the definition. So a swap changes
where the entry set comes from and changes nothing about how honest the
answer is.

## A reading, not a state machine

This seam does not run plugins — the kernel does. Every value is a READING
of kernel-owned evidence (`jinn:introspect`, `jinn:profile`,
`jinn:ledger`), and the law is written so the DANGEROUS answer is the one
that needs positive proof:

- **`active` is reachable from exactly one input** — the kernel said
  `active`, an incarnation is installed, and the live incarnation owes
  nothing. Every other combination falls to a conservative answer by
  construction, because the match has no other arm that reaches it. All
  three facts ride on the wire beside the claim (`incarnation`, and
  `owes` for the change the live incarnation already carries), so a
  consumer CHECKS the claim rather than trusting the reader on the part
  it cannot see.
- **Mounted-but-never-activated is `mounted`.** An entry with no live
  fiber at all is `no-incarnation`, with a reason from the document or the
  ledger — never `active`, never `activating`.
- **A loading fiber that already owes a change nothing will schedule is
  `interrupted`,** with the word the kernel used as its reason. There is
  no eternal `activating`.
- **There is no `unknown`.** A kernel state this table does not know is
  `unrecognised` carrying the word verbatim. A reason the ledger does not
  record is `no-recorded-cause` carrying the window that was searched, a
  COUNT of the reason-bearing lines it declines to cite, and the
  qualifier that says why — a statement about a read that happened, which
  is why it is admissible where a sentinel is not.

## Two authorities, never mixed

An entry's grants come from the document of record (what the kernel
enforces) or from a catalog's own declaration (a claim about it). Every
answer carries `grants.source` and a qualifier saying how far that
source's word goes, in the response the consumer reads. An empty grant
list is a POSITIVE reading; a source that could not be consulted produces
a typed `unavailable` naming the contract, never an entry with no
authority.

## Known limits

- **The join is three reads at three instants,** not one atomic view. It
  is stated in every answer's `read.qualifier`, and an entry may have
  moved between them.
- **A history is bounded by its window.** `ledger-limit` lines, `jinn:ledger`'s
  own 500 cap above that, one page and no more. `window.truncated` says
  when older lines exist unread, and a `no-recorded-cause` reason under a
  truncated window means less: its `candidates` count is a count of the
  window, never of the entry's whole history.
- **A guest's own activation failure carries no reason at this pin**
  (`FINDINGS.md` #38). Pre-activation faults and broker refusals do, and
  the composition proof counts one of those and cites none of it. The
  answer is always `failed` with `no-recorded-cause`, carrying the window
  it read, a COUNT of the reason-bearing lines it declines to cite, and
  the qualifier that says why. This seam does NOT correlate a failure
  with a neighbouring refusal — round 1 did, and the type that made it
  expressible is gone — because the ledger records no causal link and a
  plausible neighbour presented as a cause is the fabrication this seam
  exists to kill. The lines themselves are read with `history(id)`.
- **`state: null` is four situations and this seam separates two**
  (`FINDINGS.md` #39). `disabled` is read positively from the document; a
  group, a disposed-but-named entry and a spawn-time failure are not
  separable at the pin, and a catalog without a `jinn:profile` grant
  separates none of them.
- **An entry the document could not RESOLVE is in no catalog at all.** It
  is absent from `jinn:introspect.entries()` and from
  `jinn:profile.document()` alike, so a list here silently omits exactly
  the entries that are most broken. `FINDINGS.md` #39.
- **An API-driven provider swap works only because this seam was designed
  for it** (`FINDINGS.md` #37). `patch-entry` writes only `config`, so the
  package-and-hash swap the other six seams prove by file edit is not
  reachable through the operator API.
- **There are no typed events, and that is a recorded decision**
  (`FINDINGS.md` #40). A catalog knows the composition at the instant it
  asks and nothing between two asks: `jinn:introspect` is a pull surface
  and the kernel publishes nothing on the event bus. The only event this
  seam could emit would be produced by a poller diffing two snapshots —
  announcing a transition it did not witness and cannot time, which is
  this seam's own fabrication class one layer up.
- **A fiber BETWEEN two rests is invisible** (`FINDINGS.md` #41). The
  `mounted`, `activating` and `interrupted` readings are real and the
  kernel passes through them; no reader at this pin can catch one. A real
  restart, measured: 189 consecutive catalog reads, all `active`, while
  the kernel's ledger recorded `Active → Unloading → Pending → Loading →
  Active`. Anything built on watching a plugin's life through this seam
  will silently miss every transition. The three words are therefore
  marked UNREACHABLE in the definition itself
  (`jinn_plugins::UNREACHABLE_AT_PIN`, carrying the qualifier and the
  citation), the variants say so where a consumer reads the vocabulary,
  and `no-transient-reading-at-this-pin` is the canary: at this pin a
  catalog answer that DELIVERS one is a defect, so the day the kernel
  gains a publish path the suite goes red and the reading law is re-read
  rather than quietly outliving its measurement.
- **The surface is READ-ONLY.** A plugin is reshaped by patching the
  profile through `/v1/profile`, which this seam consumes. There is no
  enable, disable, restart or remove here, deliberately: two ways to
  change one thing is how they drift apart.
- **Reading costs ledger lines.** Every answer is three ledgered contract
  calls, and `jinn:ledger`'s own consumption receipts are among them. An
  operator polling this surface grows the ledger; nothing here rate-limits
  it.
- **Concurrency is not proven.** The proofs drive one caller at a time.

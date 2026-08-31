# Absence is three things: the reading, the bytes, and the id

*Phase 2.6, round 3. Why round 2's correct fix shipped a new defect one
seam down, and what the fix for it had to encode.*

## The defect

Round 2 taught three journal replays to answer a typed absence and three
`adopt_all`s to honour it: a document holding no complete record installs
nothing, so nothing is recovered and nothing is written back. That is
right, it is proven, and it stands.

Then `Sessions::create` minted `default-1` — the id the record-less
document was named for — and appended the new session's `created` record
straight after the stray `{` it had just declined to read. The two fused
into one undecodable line, and the boot after that refused to replay at
all:

```
create response ... "session-id":"default-1"
journal after create: "{{\"api-version\"..."
next replay: Err("journal line 1: key must be a string at line 1 column 2")
```

An **accepted absence became corruption**, which is the one thing the
card's acceptance forbids by name. It is `FINDINGS.md` #34's fuse — a
tolerable tear that the next append lands on — reached through a door
nobody had looked at.

## Why the reading was only a third of the answer

A document that READS as absent is not a clean slate. It is still two
things:

- **bytes on disk**, which are what the next `append` lands on; and
- **a name**, which is an id the registry will hand out again.

Round 2 removed the sentinel and stopped there. The sessions store had no
heal at all, so the bytes stayed; nothing advanced the mint past a
declined id, so the name came back. Each half is harmless alone. Together
they are the whole defect.

So the answer is three things, and each is proven on its own assertion:

1. **The reading.** Round 2's: `Option<Replayed>` / `RunDocument::Absent`,
   with no sentinel to read a status off.
2. **The bytes.** The document is `fs::remove`d, whole. Every byte in it
   is one the reader's own law says was never a record, so nothing that is
   a record is lost — and unlike an emptied file left in place, a name
   that is gone cannot be appended onto by any later writer. A drop is the
   only permitted repair. Nothing here completes, synthesizes or infers a
   record.
3. **The id.** `Sessions::reserve`, `Todos::reserve`,
   `Workflows::reserve_run` / `reserve_workflow` move the mint past an id
   whose document held no record, installing nothing. Two independent
   reasons the next `create` cannot land in an absent record's place, and
   neither leans on the other.

A torn tail on a document that DOES hold records is still healed to its
whole prefix and counted as `healed-tails`, separately from
`documents-without-a-record`: a trimmed tail leaves the records that were
there, and a record-less document had none. Reporting the second as the
first is describing a repair that did not happen.

`jinn:fs` still cannot drop a suffix (`FINDINGS.md` #34), so a heal is a
whole-prefix rewrite. That cost stands until a `truncate` lands.

## What the siblings were, and were not

The same reproduction was written for all three stores rather than
reasoned about. All three were red, and differently:

| seam | bytes | id | count |
|---|---|---|---|
| sessions | survived (`"{"`) | reused (`default-2`) | no heal at all |
| todos | dropped | reused | counted as `healed-tails: 1` |
| workflows | dropped | reused (`default-r2`) | correct |

Todos and workflows corrupted nothing, because each had emptied the file
before the reuse. That is safety by **derivation** — and round 2's own
`record_less` doc had written the derivation down, in as many words, as
the reason reuse was fine. It was fine. It was not proven, and the seam
where the same reasoning was applied without the emptying is the one that
broke.

## The process half

Round 2 found this class live in the two seams below workflows and fixed
them in passing. Checking the siblings was the right instinct and found a
real defect. But those fixes rode on the primary fix's evidence and got
none of their own: no red test first, no live reproduction, and the
verifier's round never reached the sections that would have caught it.

**A sibling fix gets its own failing test and its own live reproduction,
or it is carded separately and left alone.** A correct instinct with no
evidence of its own is how this one became a new defect with a shorter
fuse than the one it fixed.

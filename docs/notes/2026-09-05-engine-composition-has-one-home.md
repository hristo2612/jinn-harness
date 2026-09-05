# Engine composition has one home

The engine, session, Todo and workflow kits assembled the same engines four
times: default echo, optional process witness, optional vendor providers and
probe. The copies included model lists and executable/environment authority;
three comments claimed those facts already had one home when they did not.

`engine_kit::build_entries` now builds the four artifacts and returns the
mounted entries with the engine ids they serve. The four kit commands use it;
each still owns its distinct store composition. The low-level entry functions
remain available to callers that construct a different profile. This is profile
policy above the kernel (R10), with no contract or pin change (R12).

Entry order, grants, vendor configuration, the echo delay and conditional
process witness are preserved. Vendor artifacts still build when their CLIs
are absent, because later profile edits can mount them. Validation compares
the complete generated profiles before and after, with and without vendor
entries, and runs the existing pinned-loader composition suite.

The README's historical status wall is removed. The limitations map remains:
it contains evidence first recorded there. Kernel findings, rationale notes
and Git history retain the detailed history without competing with the map.

# Local Integration Policy

New repair branches and pull requests start from the latest local integrated
commit containing this contributor's accumulated fixes. Do not start again
from `main` or `origin/main` unless the user explicitly requests that baseline.

Inspect the current worktree, local commit ancestry and pending PR heads before
branching. The most recent remote PR or commit timestamp is not proof that it
contains all local repairs. Preserve local uncommitted work, and keep changes
marked local-only or "do not submit" out of submissions until the user changes
that instruction.

An existing PR can be updated on its own branch, but its resulting fix must be
incorporated in the local integration before another new PR or combined package.
The GitHub target branch does not change the required local starting point.
When a submission inherits open PRs, record those dependencies in its description
instead of discarding their history to produce an isolated upstream-based diff.

Combined validation packages include current related PR heads and authorized
local fixes. Closed, superseded PRs use their accepted successor implementation;
obsolete implementations are not reintroduced. Record exact source commits,
included PRs, local patches, build identifiers and artifact checksums. Always
distinguish a built artifact from the version installed on the user's machine.

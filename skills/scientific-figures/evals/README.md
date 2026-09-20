# Routing trials

The [six scenarios](scenarios.json) are evaluation inputs, not execution receipts.
A static test can validate that files/links exist; it cannot establish that an
agent follows the right route. Do not label these scenarios as passed merely
because the expected route is written down.

Install the pinned bundle in a disposable consumer and supply its actual source,
notation, rendering and optional catalog owners through a short project profile.
Run each prompt in a fresh agent context, with the normal discovery mechanism.
For a negative branch, withhold the corresponding tool deliberately. Preserve
actual permitted tool calls, changed files/artifact identities and the result;
record model/host, skill commit, case ID and evidence type. Avoid credentials
and raw private conversation content in receipts.

Check every `must` and `must_not` against observed behavior. In particular, an
accepted composition, compile-only task or Mermaid syntax repair should bypass
the full design loop. Inspect the artifact and source diff, not just the route
name in the final response. A missing compiler must leave render acceptance
unavailable. Add a live-session case only with the shipped interface from [Studio #20](https://github.com/JanDuchscherer104/cetz-studio/issues/20);
this bundle does not install or simulate that feature.

Use the caller's established agent harness; none is embedded or silently invoked
by the installer. Record unavailable host execution as an outstanding trial.
Author walkthroughs and same-context self-evaluations are supplementary evidence,
not independent fresh-context trial results.

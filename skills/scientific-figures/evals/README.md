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
unavailable. Canvas and voice collaboration remain in the external agent client
and outside these routing trials. This bundle does not install or simulate a
shared session, embedded chat or voice transport.

For `mermaid-repair`, verify the order from the actual receipt: the agent must
run the supplied wrapper and capture its parser error before the first source
edit, apply a minimal syntax repair, then rerun that same wrapper successfully.
A final successful render without the recorded pre-edit failure does not pass
the scenario.

Use the caller's established agent harness; none is embedded or silently invoked
by the installer. Record unavailable host execution as an outstanding trial.
Author walkthroughs and same-context self-evaluations are supplementary evidence,
not independent fresh-context trial results.

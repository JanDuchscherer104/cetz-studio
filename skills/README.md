# Portable figure skills

`skills/scientific-figures` is the canonical source for the portable design
skill. It owns the lesson, claim-to-visual mapping, renderer choice, geometry
and provenance, and visual comparison. It does not own scientific truth,
consumer notation/data, a manuscript, or a gallery registry.

Agent conversation, voice input and orchestration stay in the external coding
agent client. Use Studio separately for native previews and supported visual
edits, with explicit source handoffs between them. This bundle does not install
or simulate a shared session, embedded chat or voice transport; installing a
skill does not establish agent/editor connectivity.

## Install an explicit pin

Requirements: Python 3.10+ and Git, and a trusted local Studio checkout containing
the reviewed commit and this installer. Run from that checkout. Substitute the
full reviewed commit for the placeholder; do not use a moving branch or tag.

```sh
python3 scripts/install_skill.py install \
  --source . \
  --revision <reviewed-40-character-commit-sha> \
  --project /absolute/path/to/consumer

python3 scripts/install_skill.py check \
  --project /absolute/path/to/consumer \
  --revision <reviewed-40-character-commit-sha>
```

The installer reads the committed bundle, not dirty working-tree files. It has
no network step and changes only `.agents/skills/scientific-figures` under the
existing project. References, evaluation inputs, license and attribution travel
with the skill; a sibling ARIA checkout is unnecessary. A consumer's
`AGENTS.md`, `.agents/figure-project.md` and other skills are untouched.

Keep the chosen commit in the consumer's dependency/provenance owner. The
installed `.cetz-studio-install.json` records that pin, file hashes and executable
modes. `check --revision` detects content and pin drift; it is not a signature
or trust attestation. Hosts that discover `.agents/skills` can load the bundle
there. For other hosts, use their documented explicit skill-loading interface;
the installer does not modify global configuration or assume every host uses
the same discovery mechanism.

## Update without overwriting local work

Run `install` with another reviewed full commit. An unchanged install is a no-op.
An existing unmanaged skill, modified/missing/untracked file, unexpected folder,
symlink or executable-mode drift causes refusal. Preserve local changes outside
the managed directory or contribute them upstream before updating. There is no
force-overwrite option. Uninstall only after reviewing `check` and preserving
any desired files; no automatic uninstall or global cleanup is provided.

Updates stage the complete candidate on the destination filesystem, then retain
the previous version until replacement succeeds. A cooperating-installer lock
prevents concurrent installs. Pause other writers: two directory renames are
not a filesystem compare-and-swap or a crash-durable transaction. An interrupted
update may leave a `.scientific-figures-*` recovery directory and lock beside the
skill; inspect these before recovery. Failed rollback retains the previous
bundle there. Paths containing symlinks are refused; use physical paths.

## Consumer context and evaluation

The skill's [project reference](scientific-figures/references/project.md) explains
the optional project profile. Keep it short and point at the actual science,
notation, style, evidence, gallery and integration owners. Installation never
invents those facts or creates a second source of authority.

```sh
python3 -m unittest discover -s tests -p 'test_skill_bundle.py' -v
```

These tests exercise actual pinned Git installs, source/mode fidelity, managed
upgrades, drift/refusal/recovery, CLI behavior and installed reference closure.
They do **not** establish model invocation quality. The six
[routing scenarios](scientific-figures/evals/README.md) require separate real
agent trials and receipts; static checks and author walkthroughs are not those
trials. Compilation, visual inspection and scientific review remain distinct.

The adapted skill bundle is Apache-2.0 with its own LICENSE/NOTICE. The installer
and contract tests are MIT project code. External renderer packages retain their
own terms; no package code or broad manuals are vendored into this bundle.

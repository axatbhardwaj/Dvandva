# Dvandva v4 kernel 0.2.0

This directory contains the non-publishable implementation of the active
skill-only `dvandva.run.v2` Run Baton protocol and role API 2. The crate has
`publish = false`: it is not published on crates.io. The intended distribution
is a checksummed `skills-v0.2.0` GitHub release asset used only by the root
`setup-dvandva`, `vadi`, and `prativadi` skills. It is not installed on `PATH`
and does not invoke or manage Claude Code, Codex, T3 Code, issue trackers,
goals, or publication providers.

The kernel can identify `dvandva.run.v1` only for a dedicated upgrade that is
one-way.
Ordinary v1 role operations fail with `migration_required`; setup installs or
updates the kernel but never migrates runs.

```bash
cargo build --manifest-path v4/Cargo.toml
v4/target/debug/dvandva-v4 --help
```

Kernel development can create one disposable run directly:

```bash
v4/target/debug/dvandva-v4 init \
  --run-dir /tmp/dvandva-example \
  --run-id example \
  --objective "Produce and review one artifact" \
  --worker codex \
  --reviewer claude \
  --repository-id github.com/example/project \
  --required-deliverable implementation="Reviewed implementation"
```

End users do not run these commands. Each independently started session uses
the role skill facade, which retains credentials privately and alternates
role-safe mutations with foreground waiting. See
[`docs/protocol/minimal-run-baton.md`](../docs/protocol/minimal-run-baton.md)
for the state graph and
[`docs/workflows/skill-only-run.md`](../docs/workflows/skill-only-run.md) for
installation and role behavior.

# Dvandva v4 kernel 0.3.0

This directory contains the non-publishable implementation of the active
skill-only `dvandva.run.v2` Run Baton protocol and role API 2. The crate has
`publish = false`: it is not published on crates.io. The distribution is
one checksummed `skills-v0.3.0` GitHub release asset,
`dvandva-kernel-linux-x86_64`, used only by the root `setup-dvandva`, `vadi`,
and `prativadi` skills. It is not installed on `PATH` and does not invoke or
manage Claude Code, Codex, T3 Code, issue trackers, goals, or publication
providers.

The kernel builds and is tested for Linux x86_64 only, for now. It relies on
Linux-specific calls — `renameat2(RENAME_NOREPLACE)` for run archival,
`openat` with `O_NOFOLLOW` for contained artifact reads, `flock` for the run
lock, and explicit Unix directory and file modes — so it has no macOS or
Windows build.

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

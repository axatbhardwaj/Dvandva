# Dvandva v4 kernel

This directory contains the non-publishable implementation of the active
skill-only Run Baton protocol. It is packaged only as a private, checksummed
`skills-v*` release asset used by the root `setup-dvandva`, `vadi`, and
`prativadi` skills. It is not installed on `PATH` and does not invoke or manage
Claude Code, Codex, T3 Code, issue trackers, or publication providers.

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
  --reviewer claude
```

End users do not run these commands. Each independently started session uses
the role skill facade, which retains credentials privately and alternates
role-safe mutations with foreground waiting. See
[`docs/protocol/minimal-run-baton.md`](../docs/protocol/minimal-run-baton.md)
for the state graph and
[`docs/workflows/skill-only-run.md`](../docs/workflows/skill-only-run.md) for
installation and role behavior.

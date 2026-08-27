# Dvandva v4 kernel

This directory contains the non-publishable implementation of the minimal Run
Baton protocol. It does not install, invoke, or manage Claude Code, Codex, T3
Code, issue trackers, or publication providers.

```bash
cargo build --manifest-path v4/Cargo.toml
v4/target/debug/dvandva-v4 --help
```

Create one disposable run:

```bash
v4/target/debug/dvandva-v4 init \
  --run-dir /tmp/dvandva-example \
  --run-id example \
  --objective "Produce and review one artifact" \
  --worker codex \
  --reviewer claude
```

Each independently started session then uses `claim`, retains its returned
token privately, and alternates `apply` with `wait`. See
[`docs/protocol/minimal-run-baton.md`](../docs/protocol/minimal-run-baton.md)
for the state graph, action rules, recovery semantics, and startup prompts.

# Contributing

guideme makes a TypeSafe Jev judgment usable as Rust control flow. It is one workspace with
two published crates: `guideme`, the product, and `guideme-derive`, which holds
`#[derive(Choice)]` and `#[derive(Levels)]`.

Issues and pull requests are welcome. The one exception is a vulnerability, which goes
through [`SECURITY.md`](SECURITY.md) and never through a public issue. Both crates are on
crates.io, so the public surface is an API other people depend on; read the rules before
changing it.

## The rules live in AGENTS.md

[`AGENTS.md`](AGENTS.md) is the contributor guide and it is binding. It says which file owns
which seam, the invariants that hold everywhere in library code, the policy semantics shared
with every other guideme SDK, what a new test is allowed to be, the commands, and the git and
release process. Read it fully before editing.

Three sections carry most of what a first change needs:

- `## Commands` — the gate and the other tasks.
- `## Git` — branching, hooks, commit messages.
- `## Changing the contract` — what to do when a change reaches the wire, the policy or a
  telemetry field name. Those are contract changes and they have a checklist.

## The loop

Tooling is managed by [mise](https://mise.jdx.dev); the toolchain is pinned in
`rust-toolchain.toml`.

```sh
mise install      # fetch the tools
mise run hooks    # activate the tracked git hooks, once per clone
mise run test     # while you work
mise run check    # the full gate; the pre-push hook runs it too
```

## License

Contributions are dual-licensed under [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option, the same terms as the crates. Unless you say
otherwise, anything you submit for inclusion is licensed that way, with no additional terms.

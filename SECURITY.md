# Security policy

## Supported versions

`guideme` and `guideme-derive` are at 0.1.0, which is the only release and therefore the only
supported version. A published version on crates.io can never be replaced, so a fix ships as
a new version and the affected one is yanked.

## Reporting a vulnerability

Report it privately, not in a public issue. GitHub private vulnerability reporting is enabled
on this repository: [open an advisory](https://github.com/pedro-pscunha/guideme-rust/security/advisories/new),
or go to the Security tab and choose **Report a vulnerability**. Only the maintainers can see
it.

This is a small project with nobody on call, so there is no response time to promise. You
will get an answer; it may not be the same day.

## Scope

In scope, because this crate owns them:

- **The API key.** `ApiKey` has no `Display` and no `Serialize`, and its `Debug` is
  `ApiKey(***)`. The compile-fail cases under `guideme/tests/ui/api_key_*.rs` prove the two
  traits are absent; `guideme/tests/redaction.rs` proves the `Debug` output. That the key is
  never recorded on a span or returned in an error is an invariant held in review, not by a
  test. Any path that puts the key somewhere a caller can read it is a vulnerability.
- **State confidentiality.** The state passed to `Guide::ask` is user data. Its content never
  reaches a span unless `record_state(true)` was set; its length, `guideme.state.bytes`,
  always does. Recording the content without that opt-in is a defect here.
- **The wire layer** under `guideme/src/api/`: request construction, transport, status and
  error handling, the retry path, and any response the service could return that makes the
  decoder panic or behave incorrectly.
- Vulnerable dependencies reachable from library code.

Out of scope:

- **The TypeSafe service itself.** Its behaviour, its models, its authentication and its
  handling of the data you send are upstream, not here: see <https://docs.typesafe.ai>. This
  crate is a client.
- What a model decides. A judgment you disagree with is not a vulnerability.
- `examples/`, which is illustrative and builds outside the workspace.
- A key you leaked yourself, by printing it or committing a `.env`.

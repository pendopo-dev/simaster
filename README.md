# simaster

Unofficial terminal-first CLI for SIMASTER UGM: scriptable, JSON-first,
agent-friendly. Built for students who prefer the shell, scripts, and AI agents
over a web UI.

**Status: pre-alpha.** There is no released binary yet. Offline diagnostics and
shell completions, web login, and browser authentication are available. SIMASTER data
commands are still stubs.

[![CI](https://github.com/pendopo-dev/simaster/actions/workflows/ci.yml/badge.svg)](https://github.com/pendopo-dev/simaster/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.92-blue.svg)](Cargo.toml)

## Why

- **Terminal-first.** Read your profile, schedule, grades, and attendance
  without leaving the shell.
- **Scriptable.** Human-readable output on a TTY, stable JSON when you ask for
  it, and documented exit codes.
- **Agent-friendly.** A predictable command grammar, a versioned JSON envelope,
  and no interactive prompts when JSON output is requested.
- **Small surface.** Read-only by default. Write operations are not available.

## Install

No release has been published. Build from source:

```bash
cargo install --git https://github.com/pendopo-dev/simaster
```

Rust 1.92 or newer is required. The crate uses edition 2024.

## Quickstart

```bash
# Inspect the local environment without network access.
simaster doctor
simaster doctor --json | jq .

# Generate completions from the live command tree.
simaster completions bash > ~/.simaster.bash

simaster --help
simaster --version
```

`doctor` runs offline. Browser login requires a DTI-registered UGM ID public
client and an OS keyring. See [Authentication](docs/AUTH.md) for setup.

## Commands

| Command | Status | Notes |
|---|---|---|
| `simaster doctor` | available | Offline environment and config checks. |
| `simaster doctor --json` | available | Stable envelope, `schema_version: 1`. |
| `simaster completions <shell>` | available | `bash`, `zsh`, `fish`, `powershell`, `elvish`. |
| `simaster auth login` | available | Web login or browser PKCE with a registered UGM ID client. |
| `simaster auth status` | available | Local session metadata and expiry. |
| `simaster auth logout` | available | Remove the local keyring session. |
| `simaster profile show` | not implemented | Full profile through UGM ID. |
| `simaster whoami` | not registered | Identity through UGM ID. |

## Authentication

```bash
simaster auth login --method web
simaster auth status --json
simaster auth logout
```

Web login reuses a valid keyring session or prompts for credentials. Image
CAPTCHAs are classified locally with the ddddocr model, which is downloaded
once into the cache directory on first use; audio CAPTCHAs and classifier
misses fall back to a terminal prompt. Browser PKCE is available
with a registered UGM ID public client. See [Authentication](docs/AUTH.md)
for setup and verification status.

## Agentic usage

- Add `--json` to get machine-readable output. The envelope is versioned with
  `schema_version`; see [`docs/CLI-CONTRACT.md`](docs/CLI-CONTRACT.md).
- Diagnostics go to stderr; structured output goes to stdout.
- Login uses exit codes 3 for authentication, 4 for network or rate limit, and
  5 for unexpected upstream responses. See [the CLI contract](docs/CLI-CONTRACT.md).
- The CLI is read-only. Write commands and a read-only `api` escape hatch are
  tracked in [GitHub Issues](https://github.com/pendopo-dev/simaster/issues).
- Browser login is interactive; `--json` does not remove the browser step.
- Set `NO_COLOR=1` for stable terminal output. `RUST_LOG` controls stderr
  logging.

## Documentation

| Document | Purpose |
|---|---|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Layers, module map, current command flow, error model, testing. |
| [`docs/AUTH.md`](docs/AUTH.md) | Authentication methods, flows, endpoints, storage, threat model. |
| [`docs/SIMASTER-VNEXT-API.md`](docs/SIMASTER-VNEXT-API.md) | OpenAPI reference for APK-derived vNext and related API routes. |
| [`docs/CLI-CONTRACT.md`](docs/CLI-CONTRACT.md) | Command grammar, flags, JSON envelope, exit codes, env vars. |
| [`docs/adr/0001-single-crate.md`](docs/adr/0001-single-crate.md) | Why the project uses one crate. |
| [`docs/adr/0002-auth-strategy.md`](docs/adr/0002-auth-strategy.md) | Why UGM ID PKCE is primary and web remains the fallback. |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | Setup, tests, branches, commits, review, release. |
| [`GOVERNANCE.md`](GOVERNANCE.md) | Roles, consensus, RFCs, maintainer lifecycle. |
| [`SECURITY.md`](SECURITY.md) | Reporting, scope, credential rules, disclosure. |
| [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) | Contributor Covenant 2.1. |

Task state lives in [GitHub Issues](https://github.com/pendopo-dev/simaster/issues).

### Browse and update the API docs

From the repository root, serve the documentation directory locally:

```bash
python3 -m http.server 8765 --directory docs
```

Open <http://localhost:8765/api/> for the interactive Scalar reference. The
viewer loads its UI from a CDN and reads the web and vNext OpenAPI files from
`docs/openapi/`.

Regenerate the vNext catalog from a Postman collection and check for drift with
the same collection:

```bash
bun scripts/sync-spec.ts path/to/SIMASTER.postman_collection.json
bun scripts/sync-spec.ts --check path/to/SIMASTER.postman_collection.json
bun test scripts/sync-spec.test.ts
just api-lint
```

## Contributing

Contributions are welcome. Read [`CONTRIBUTING.md`](CONTRIBUTING.md) first.
Small documentation fixes can go straight to a pull request; behavior changes
need an issue and a test. Authentication, endpoint, and security changes need a
second reviewer.

## Security

Do not open a public issue for a vulnerability. Use GitHub Security Advisories
as described in [`SECURITY.md`](SECURITY.md). Never include credentials,
cookies, tokens, authorization codes, or real student data in an issue, pull
request, fixture, or log.

## License

Licensed under either of:

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT License ([`LICENSE-MIT`](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in this project is dual-licensed under
the same terms, without additional conditions.

## Disclaimer

`simaster` is an unofficial project. It is **not affiliated with, endorsed by,
or supported by Universitas Gadjah Mada (UGM)**. Use it only with your own
account, in compliance with UGM policies and applicable law. The project does
not provide credentials, does not bypass authentication or authorization, and
comes with no warranty. You are responsible for the requests you make and for
protecting your own data.

## Acknowledgements

The official UGM ID OAuth 2.0/OIDC documentation is the primary source for the
authentication design:

- [UGM ID Swagger UI](https://oauth.simaster.ugm.ac.id/docs)
- [UGM ID OpenAPI 3.1 specification](https://oauth.simaster.ugm.ac.id/openapi.json)
- [UGM ID OIDC discovery](https://oauth.simaster.ugm.ac.id/.well-known/openid-configuration)
- [UGM ID public health check](https://oauth.simaster.ugm.ac.id/health)

Prior-art static analysis of the official mobile app, especially
[`gafnaa/vnext-reveng`](https://github.com/gafnaa/vnext-reveng), is secondary
evidence for the vNext **data adapter**, not for authentication. Community KKN
automation work informed the web-session and CAS fallback design. Official UGM
ID behavior is `documented by UGM` and `live-tested: no` except the public
health check; prior-art endpoint claims are `unverified`. Static analysis is
evidence of client code, not server behavior. The mobile app's client ID,
embedded secret, tokens, cookies, and custom-scheme redirect are not included.

# Architecture

SIMASTER uses one Cargo package with a CLI binary and Rust library. The binary
parses arguments and maps errors to process exit codes. The library contains
feature modules and the CLI adapter.

```text
src/
├── main.rs      Process entry and exit codes
├── lib.rs       Library modules and CLI tracing setup
├── auth/
│   ├── mod.rs   Pending login, callback validation, local session store
│   └── oidc.rs  UGM ID discovery, code exchange, and ID-token verification
└── cli/         Argument parsing, browser and loopback, JSON and terminal output
```

The CLI calls `auth::begin_login`, opens the authorization URL, and supplies the
loopback request target to `PendingLogin::complete`. Auth validates the callback,
exchanges the code, and persists the session. Callers receive typed session
metadata and errors. The feature module has no dependency on clap, terminal
output, or a browser launcher.

Put each new reusable operation in its feature module and keep its command
mapping in `cli/`. A Rust SDK consumer can call the same feature operations
directly. An MCP entry point can map those operations to tool results and
report when interactive login is needed. See [ADR 0001](adr/0001-single-crate.md)
for package split criteria.

## Verification

Browser login tests use local HTTP mock discovery, token, and JWKS endpoints.
The callback crosses a loopback listener and the test stores the session in
a mock keyring. PR CI makes no authenticated SIMASTER or UGM ID calls.
See [Authentication](AUTH.md) for endpoint evidence and
[the CLI contract](CLI-CONTRACT.md) for help, output, and exit codes.

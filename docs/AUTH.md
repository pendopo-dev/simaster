# Authentication

## Web login from the CLI

```bash
simaster auth login --method web --username <ugm-username>
simaster auth status --json
simaster auth logout
```

Web login checks the saved cookie against SIMASTER before prompting. Omit
`--username` to reuse the saved account or enter a username at the prompt.
A fresh login reads the password with terminal echo disabled. If direct login
doesn't establish a session, the CLI can try UGM CAS. When SIMASTER presents
a CAPTCHA, the CLI first classifies image challenges with the ddddocr model,
which is downloaded once (about 54 MB, SHA-256 pinned) into the cache
directory on first use and then works offline. Challenges the model cannot
answer — audio CAPTCHAs and any image after a rejected answer — are saved to
a temp file, opened in a local viewer, and answered at the terminal prompt.
Every answer goes back in the same HTTP session that fetched the challenge;
the portal allows up to five challenges per login.

If CAS requires browser interaction, the CLI can switch to browser PKCE when
`SIMASTER_CLIENT_ID` is configured. That handoff requires a registered public
client. `--method auto` selects web when the client ID is absent and browser
PKCE when it is configured.

The web route evidence is in the [SIMASTER web OpenAPI reference](openapi/simaster-web/openapi.yaml).
Web login and session reuse are **unverified** against live UGM for this
implementation. Local tests simulate direct login, CAS, CAPTCHA, and session reuse.

## Browser PKCE

Browser Authorization Code with PKCE S256 is available for public/native UGM ID
clients. Set `SIMASTER_CLIENT_ID` to a client ID registered with DTI UGM for
`http://127.0.0.1:53682/callback`.

```bash
SIMASTER_CLIENT_ID=<client-id> simaster auth login
simaster auth status --json
simaster auth logout
```

`--method browser` selects browser PKCE explicitly. Login requests
`openid user.read` and sends the client ID without a client secret. The browser
handles CAS and consent. If the launcher reports a failure, the CLI prints the
authorization URL to stderr. The loopback listener accepts one request, waits
up to five minutes, and closes before code exchange. The CLI checks callback
state and validates the ID token's RS256 signature, key ID, issuer, audience,
expiry, subject, and nonce before saving the session.

## Evidence

| Endpoint | Evidence | Live-tested |
|---|---|---|
| `GET /.well-known/openid-configuration` | documented by UGM | no |
| `GET /oauth/authorize` | documented by UGM | no |
| `POST /oauth/token` | documented by UGM | no |
| `GET /.well-known/jwks.json` | documented by UGM | no |

The [UGM ID discovery document](https://oauth.simaster.ugm.ac.id/.well-known/openid-configuration)
advertises scheme-less endpoints and issuer. The client uses HTTPS for
transport and requires the literal `oauth.simaster.ugm.ac.id` issuer in discovery
and the signed ID token. Mock tests cover the discovery quirk, PKCE exchange,
and rejected state, claims, key ID, and signature. Authenticated testing
with a DTI-registered client is tracked in [#35](https://github.com/pendopo-dev/simaster/issues/35).

## Local sessions

The CLI stores the web cookie or browser access token with its session metadata
in the OS keyring. A working keyring is required. On Linux this is a Secret Service provider.
`auth status` reads local records, preferring the web record when present.
It reports browser token expiry when available. `auth logout` clears both records. It leaves the
browser's UGM session active.

The authentication strategy is recorded in [ADR 0002](adr/0002-auth-strategy.md).

The [SIMASTER web OpenAPI reference](openapi/simaster-web/openapi.yaml) records
web-session behavior observed in the traffic captures from
[#52](https://github.com/pendopo-dev/simaster/issues/52). Those routes remain
unverified. The vNext gateway's acceptance of official UGM ID tokens is also
unverified.

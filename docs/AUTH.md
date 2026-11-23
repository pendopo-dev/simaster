# Authentication

Browser Authorization Code with PKCE S256 is available for public/native UGM ID
clients. Set `SIMASTER_CLIENT_ID` to a client ID registered with DTI UGM for
`http://127.0.0.1:53682/callback`.

```bash
SIMASTER_CLIENT_ID=<client-id> simaster auth login
simaster auth status --json
simaster auth logout
```

Both `--method auto` and `--method browser` select browser PKCE. Login requests
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

The CLI stores one access token and its session metadata in the OS keyring.
A working keyring is required. On Linux this is a Secret Service provider.
`auth status` reads the local record and reports expiry using the token
response's lifetime. `auth logout` deletes the local record. It leaves the
browser's UGM session active.

The prototype supports browser login and local session management. The
approved authentication strategy, including the web/CAS fallback and extended
token lifecycle, is recorded in [ADR 0002](adr/0002-auth-strategy.md).

The [SIMASTER web OpenAPI reference](openapi/simaster-web/openapi.yaml) records
web-session behavior observed in the traffic captures from
[#52](https://github.com/pendopo-dev/simaster/issues/52). Those routes remain
unverified. The vNext gateway's acceptance of official UGM ID tokens is also
unverified.

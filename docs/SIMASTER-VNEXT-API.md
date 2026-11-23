# SIMASTER vNext API reference

[`openapi/simaster-vnext/openapi.yaml`](openapi/simaster-vnext/openapi.yaml) describes the API requests in the supplied Postman collection.

The collection `SIMASTER Student vNEXT - Static APK Endpoint Catalog` records static analysis of supplied SIMASTER Student APK splits on 2026-11-23. It contains no live traffic. The OpenAPI file combines 51 HTTP requests into 41 path items and keeps the original Postman request names as provenance.

## Evidence

Each operation has `x-status`, `x-live-tested`, and `x-provenance` fields.

- `documented by UGM` marks the UGM ID OAuth authorization and token endpoints. See the [official UGM ID OpenAPI specification](https://oauth.simaster.ugm.ac.id/openapi.json).
- `unverified` marks routes derived from APK static analysis or external integration strings.
- `x-live-tested` is `false` for every operation. The collection contains no live-tested response.
- Saved response bodies are illustrative examples from the collection. They do not establish the live response contract.

The catalog includes vNext routes, checkpoint and attendance routes, legacy microservices, low-confidence route candidates, and external integrations. Low-confidence candidates may have an unknown host, method, authentication requirement, or response shape. A request header missing from the collection does not establish that an endpoint is public.

The catalog omits six website page requests: SIMASTER services, the SIMASTER home page, Report, ULT, RSA clinic, and the DTI knowledge base. It also omits the Armada GPS WebSocket URL because OpenAPI does not describe WebSocket messages or connection lifecycle.

## Use limits

This reference does not authorize live requests. Do not use it to access another person's data or to bypass authentication, CAPTCHA, MFA, rate limits, or authorization. Use only an authorized environment and your own account.

The OAuth operations describe the CLI's Authorization Code flow with PKCE and a registered loopback redirect. They do not include the mobile app's client ID, embedded secret, custom-scheme redirect, or write scopes. The CLI requests read-only scopes only.

The checkpoint and legacy attendance catalog includes POST operations that may change attendance records. Their presence documents APK-derived route claims only. The SIMASTER CLI does not implement these writes.

## Regenerate

Run `bun scripts/sync-spec.ts path/to/SIMASTER.postman_collection.json` to regenerate the OpenAPI files and this reference. Add `--check` before the collection path to report drift without writing files.

Run `bun test scripts/sync-spec.test.ts` to check generation, redaction, OAuth contracts, and drift handling in an isolated temporary directory.

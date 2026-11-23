# SIMASTER Student vNEXT APK — static security review

Review date: 2026-11-23 (Asia/Jakarta)

## Scope and limits

Reviewed the supplied split APK set in `apks/`, including every DEX and the x86_64 native split. The app is Flutter; application code is compiled into `lib/x86_64/libapp.so`, so the endpoint and flow inventory below is based on native string/symbol evidence, manifest parsing, and embedded assets. One authorized OAuth login/consent/token exchange was executed using user-provided credentials to verify the flow. No presensi or other application-data write, traffic interception, or token-value retention was performed.

Package: `id.ac.ugm.student.vnext.simaster`

Version: `1.11.2` (version code `66`)

## Executive summary

The highest-priority items to validate and patch are:

1. The production APK contains development, preview, and checking API/OAuth hosts, including a Postman OAuth callback URL. Release builds should not ship non-production configuration or development scopes.
2. Attendance integrity depends on mobile-provided QR/code and location inputs. The client contains fake-GPS checks and radius-related logic, but the server must independently bind the attendance record to the authenticated student, active schedule/session, QR nonce, time window, and server-observed policy.
3. Android cleartext traffic is explicitly enabled, and the compiled app includes an `http://rsa.ugm.ac.id` link. This weakens transport guarantees for any HTTP navigation or request.
4. Access/refresh-token key names and SharedPreferences storage code are present. Treat this as a local token-at-rest hardening issue, especially on rooted/compromised devices.
5. A password-like test literal is compiled near `package:simaster_ugm/app/modules/testing/svg_test.dart`. It was not proven to be an active credential, but it should be removed and any corresponding account/secret should be rotated if valid.
6. A public automation repository cross-checks the OAuth/SSO sequence and contains a hardcoded OAuth client secret. Treat that secret as compromised and rotate it; do not copy it into Postman or another artifact.

## Findings

### F-01 — Non-production hosts and OAuth configuration shipped in release APK

Severity: High priority for release hygiene; impact depends on reachability and server controls.

Evidence in `lib/x86_64/libapp.so`:

- Production API base: `https://api.simaster.ugm.ac.id/vnext/v1/`
- Development API base: `https://api.dev.ugm.ac.id/`
- Preview API base: `https://api.preview.ugm.ac.id/`
- Checking API base: `https://api.checking.ugm.ac.id/`
- Production OAuth token endpoint: `https://oauth.simaster.ugm.ac.id/oauth/token`
- Development OAuth token endpoint: `https://oauth.dev.ugm.ac.id/oauth/token`
- Development authorization URLs use `https://oauth.pstmn.io/v1/browser-callback`.
- Production authorization uses the custom redirect `id.ac.ugm.student.vnext.simaster://oauth2`.

The release binary therefore reveals multiple environments and contains a development OAuth flow with broad scopes such as `student`, `vehicle.read`, `mygate.read-write`, `parent.read-write-update`, and `staff`. The OAuth client IDs are public identifiers, not secrets, but the environment and scope exposure makes accidental production use or staging abuse easier to miss.

Remediation:

- Build production with a single allow-listed production API/OAuth configuration.
- Remove dev/preview/checking URLs and Postman callbacks from release artifacts.
- Enforce environment separation at the gateway and OAuth server; do not rely on obscurity of hostnames.
- Minimize scopes per client and reject production redirect URIs at the OAuth server.
- Add CI scanning that fails a release if non-production hosts, Postman callbacks, test routes, or secret-like literals are present.

### F-02 — Cleartext HTTP traffic is enabled

Severity: Medium.

Evidence: `AndroidManifest.xml` has `android:usesCleartextTraffic="true"` and no custom network security configuration. The native app also contains an HTML link to `http://rsa.ugm.ac.id`.

Impact: Any HTTP request or navigation made by the app can be exposed to network attackers through interception or modification. The main API URLs observed are HTTPS, but the manifest setting is global and the HTTP RSA link creates a concrete downgrade/integrity path for that content.

Remediation:

- Remove `usesCleartextTraffic="true"`; use HTTPS-only transport.
- Change the RSA link to HTTPS and reject HTTP redirects where practical.
- Add a release test that fails on any `http://` application URL, except documented non-network strings such as XML namespaces.

### F-03 — Presensi anti-spoofing is a client/server trust-boundary risk

Severity: High if the server trusts client-supplied values; not proven exploitable from static analysis alone.

Evidence in the compiled app includes `checkDeviceAndLocation`, `is_mocked`, fake-GPS warnings, `latitudeGps`, `longitudeGps`, `scheduleRadius`, `GPSAreaInformation`, `microQrCode`, `checkPointLogId`, `check_point_log_check_in`, and the messages `Your current location is outside the checkpoint radius` and `Access limited, you are detected using fake GPS/Mock Location`.

This shows that the client performs device/location checks and sends attendance-related values. A modified client can normally bypass mobile-side checks, alter coordinates, replay codes, or change identifiers. The server must not treat client-side rejection as an authorization control.

Required server-side controls:

- Derive the student identity only from the authenticated access token.
- Resolve the active course/agenda/session server-side; do not trust a client-selected student, course, organizer, or schedule ID.
- Validate QR contents server-side, with short-lived, single-use, audience-bound nonces and replay protection.
- Validate the attendance time window and one check-in/check-out state transition atomically.
- Validate geofence policy server-side using a clearly defined source of truth. Treat device-provided coordinates as an input signal, not proof of presence; record accuracy, timestamp, mock-location signal, and risk decision.
- Bind checkout to the server-created check-in record and authenticated user; prevent cross-user or cross-session IDs.
- Rate-limit and audit rejected, duplicated, replayed, and out-of-radius attempts.

### F-04 — Token material appears to use SharedPreferences

Severity: Medium local-hardening issue.

Evidence in the native snapshot includes the `shared_preferences` package, `saveUserToSharedPreferences`, `access_token`, `refresh_token`, `access_token_expiration_unix`, and `refresh_token_expiration_unix`.

This indicates token persistence through ordinary Android shared preferences rather than an OS-backed encrypted credential store. It is not a remote vulnerability by itself, and `allowBackup="false"` is a positive control, but tokens may be exposed on rooted devices, through local compromise, or through diagnostic artifacts.

Remediation:

- Store refresh tokens in Android Keystore-backed encrypted storage, such as a maintained secure-storage implementation.
- Keep access tokens short-lived and memory-scoped where possible.
- Clear tokens and cached user data on logout and account switch.
- Never log token values; review release logging and Crashlytics breadcrumbs for authorization headers.

### F-05 — Password-like test literal compiled into the app

Severity: High if it maps to a real account or signing/test service; otherwise code-quality and disclosure issue.

Evidence: the string `_Ini_Adalah_Password_#_Login_*_JWT_Username_Ku_` occurs in `libapp.so` adjacent to `package:simaster_ugm/app/modules/testing/svg_test.dart`.

The string was not proven to be used for authentication. It should still be treated as a potential credential/test secret until the source owner confirms it is inert.

Remediation:

- Remove testing pages and test literals from production builds.
- Search server, CI, test, and developer accounts for this exact value or derived variants; rotate any matching credential.
- Add secret scanning over Dart source and final APK/AAB contents.

### F-06 — Exported components expand the Android attack surface

Severity: Low to Medium hardening issue; validate with component-level tests.

Manifest observations:

- `id.flutter.flutter_background_service.BackgroundService` is exported with no permission.
- `id.flutter.flutter_background_service.BootReceiver` and `WatchdogReceiver` are exported with no permission.
- AppAuth `AuthorizationManagementActivity` and `RedirectUriReceiverActivity` are exported to support OAuth redirects.
- The app uses a custom scheme redirect: `id.ac.ugm.student.vnext.simaster://oauth2`.

The background components are library-provided, but an exported service without a signature permission should be reviewed for externally supplied intents, resource exhaustion, and unintended actions. Custom URI schemes can be claimed by another installed app; whether this is exploitable depends on whether the OAuth flow uses PKCE/state correctly. The bundled AppAuth dependency contains PKCE-related code, but the app’s exact runtime configuration was not established statically.

Remediation:

- Set `exported="false"` for components that do not require external invocation.
- If external invocation is required, protect it with a signature-level permission and validate every intent extra.
- Prefer verified HTTPS App Links or an OS-protected redirect mechanism over a custom scheme where supported.
- Verify state and PKCE on every OAuth callback, and reject callbacks that do not match the in-progress transaction.

### F-07 — Public repository contains a hardcoded OAuth client secret

Severity: High if the secret is accepted by the production token endpoint; at minimum it is credential disclosure.

The public repository [vityasyyy/kkn-automation-standalone](https://github.com/vityasyyy/kkn-automation-standalone) contains the production client ID, redirect URI, OAuth flow, and a hardcoded `CLIENT_SECRET` in `src/utils/attendance.py`. The exact value is intentionally not reproduced here. The repository implementation also confirms the practical flow: OAuth authorize, UGM CAS login, ticket/session-cookie handling, consent authorization, and token exchange. A live authorized exchange accepted the exposed secret and returned HTTP 200 with Bearer token metadata (`access_token`, `refresh_token`, `scope`, `token_type`, `expires_in=900`); the token values were not retained.

Remediation:

- Revoke and rotate the client secret immediately if it is still valid.
- Review repository history, forks, Actions logs, caches, and OAuth-server registrations for reuse.
- Treat the native/mobile client as a public client; prefer authorization code + PKCE and avoid relying on a client secret shipped to or used by automation clients.
- Audit redirect URI and scope allow-lists, especially the custom URI scheme and broad scopes.

## Presensi flow reconstructed from the APK

The following is a static reconstruction, not a claim about server implementation:

1. The user authenticates through OAuth. Production strings point to `oauth.simaster.ugm.ac.id`; other compiled flows point to `oauth.dev.ugm.ac.id`.
2. The attendance screen loads current schedules/agendas and attendance details. Embedded legacy microservice names include `get_jadwal_anda_now_student`, `get_jadwal_anda_agenda_student`, `get_studi_kehadiran_rinci`, `get_studi_kehadiran_rekap`, `get_agenda_kehadiran_rinci_student`, `get_agenda_kehadiran_rekap_student`, and `get_konfirmasi_qr_student`.
3. For checkpoint attendance, the app obtains an active session from `vnext/v1/checkpoint/get_active_session/`.
4. The user scans a QR code or enters a code. The app contains `microQrCode`, `ScanCheckpointParam`, and `checkInWithQRCode`/`scanEntrantCheckpoint` symbols.
5. The app requests location, checks mock-location/device state, and compares the current position with checkpoint/schedule radius data. It displays an out-of-radius or fake-GPS error locally.
6. Check-in is sent through `vnext/v1/checkpoint/checkin/`; the client also contains `vnext/v1/fitur/1/` and an older organizer flow at `https://simaster.ugm.ac.id/presensivnext/api/signin` and `/api/send_peserta`.
7. On success, the app loads the recap/detail data and shows presence history. Checkout uses `vnext/v1/checkpoint/checkout/` and references `checkPointLogId`/`check_point_log_check_in`.

## Endpoint inventory

### Main API and authentication hosts

- `https://api.simaster.ugm.ac.id/vnext/v1/`
- `https://api.dev.ugm.ac.id/`
- `https://api.preview.ugm.ac.id/`
- `https://api.checking.ugm.ac.id/`
- `https://oauth.simaster.ugm.ac.id/oauth/authorize`
- `https://oauth.simaster.ugm.ac.id/oauth/token`
- `https://oauth.dev.ugm.ac.id/oauth/authorize`
- `https://oauth.dev.ugm.ac.id/oauth/token`

### Attendance/checkpoint paths

- `vnext/v1/checkpoint/get_active_session/`
- `vnext/v1/checkpoint/checkin/`
- `vnext/v1/checkpoint/checkout/`
- `vnext/v1/fitur/1/`
- `https://simaster.ugm.ac.id/presensivnext/api/signin`
- `https://simaster.ugm.ac.id/presensivnext/api/send_peserta`

### Other application endpoints observed

- `vnext/v1/students/`
- `vnext/v1/feeds`
- `vnext/v1/feeds/krisis`
- `vnext/v1/news?unit_id=1&limit=...`
- `vnext/v1/banners/get_dynamic?...`
- `vnext/v1/vehicles` and `vnext/v1/vehicles/`
- `vnext/v1/parking-areas`
- `vnext/v1/vehicle/user-rules`
- `vnext/v1/portal/vehicle?vehicle_plate_number=...`
- `vnext/v1/transgama/halte/terdekat?lat=...`
- `vnext/v1/transgama/rute?halte_awal_id=...`
- `vnext/v1/transgama/lanes/`
- `vnext/v1/transgama/jadwal/halte?lane_id=...`
- `https://ruanghijau.ugm.ac.id/ugm-akses/api`
- `https://armada.smartcampus.ugm.ac.id/gps/`
- `wss://armada.smartcampus.ugm.ac.id/gps/ws?room=room2`
- `https://laporkan.ugm.ac.id/`

### Additional low-confidence network candidates

The deeper native-string pass also recovered these exact route strings. The APK does not preserve enough call-site information to prove the base host, method, or schema:

- `/vnext/check-user?username=...` — appears near the My Gate user-search model.
- `/get_menu_vnext?sesId=...` — appears near dashboard/service strings.
- `/service_login_vnext` — likely a legacy service-login route.
- `/ongoing_vnext` — likely an ongoing-status route.
- `/auth/mobile/exchange` — auth-looking mobile exchange route.
- `/tugas-akhir/bimbingan/`, `/tugas-akhir/dosen_pembimbing/`, `/haltes`, `/putaran/`, and `/logs/` — ambiguous resource-looking paths that need staging validation.
- `https://accounts.google.com/o/oauth2/revoke?token=...` — bundled Google SDK endpoint.

The native snapshot also contains Flutter navigation names such as `/news`, `/study`, `/checkpoint`, `/login-ugm`, `/my-gate`, `/organizer-attendance`, and `/oauth-webview`. These are app routes, not evidence of HTTP endpoints.

## Suggested security-team validation

Use a controlled test account and staging environment only:

- Confirm each non-production host is unreachable or rejects production tokens and production users.
- Replay a successful QR request and verify the server rejects it.
- Change latitude/longitude, accuracy, mock-location flags, schedule/session IDs, student IDs, and checkpoint log IDs independently; verify server-side rejection.
- Attempt duplicate check-in, checkout-before-check-in, checkout twice, and cross-account checkout; verify atomic state enforcement.
- Inspect OAuth callback handling for state and PKCE mismatch rejection and test custom-scheme collision on a lab device.
- Verify no access/refresh token appears in SharedPreferences, logs, Crashlytics, screenshots, or exported files after the secure-storage patch.


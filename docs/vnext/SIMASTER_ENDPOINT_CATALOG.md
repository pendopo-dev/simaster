# SIMASTER Student vNEXT endpoint catalog

Static endpoint documentation generated from the supplied APKs on 2026-11-23.

## Scope and confidence

This is a client-side inventory. The inventory was expanded across the manifest, every DEX in the supplied split APKs, the Flutter native snapshot (`lib/x86_64/libapp.so`), and embedded URL/path strings. One authorized OAuth login/consent/token exchange was executed on 2026-11-23 using user-provided credentials to verify the flow; no presensi or other application data write was performed, and no credentials or token values were retained.

"Observed" means the host/path or service name is present in the APK. HTTP methods, exact field names, authentication requirements, and response envelopes are marked inferred unless the APK exposed enough evidence to establish them. Values such as {{access_token}}, {{student_niu}}, and {{qr_code}} are placeholders.

Import SIMASTER.postman_collection.json for the same inventory and representative saved examples. Its OAuth requests include Postman test scripts that persist `lt_token`, `jsessionid`, `oauth_ticket`, `csrf_token`, `authorization_code`, `access_token`, and `refresh_token` into collection variables for the next request. Do not run the collection against production without authorization.

The static scan cannot prove that every literal is reachable at runtime, and it can miss paths assembled from fragments or returned by a server. Therefore this is an exhaustive *static* inventory of the supplied APK contents, not a claim that these routes all remain live.

## Base hosts and OAuth

| Purpose | Base/endpoint | Confidence |
|---|---|---|
| Production API | https://api.simaster.ugm.ac.id/vnext/v1/ | observed |
| Development API | https://api.dev.ugm.ac.id/ | observed |
| Preview API | https://api.preview.ugm.ac.id/ | observed |
| Checking API | https://api.checking.ugm.ac.id/ | observed |
| Production OAuth authorize | https://oauth.simaster.ugm.ac.id/oauth/authorize | observed |
| Production OAuth token | https://oauth.simaster.ugm.ac.id/oauth/token | observed |
| Development OAuth authorize | https://oauth.dev.ugm.ac.id/oauth/authorize | observed |
| Development OAuth token | https://oauth.dev.ugm.ac.id/oauth/token | observed |
| UGM SSO CAS login | https://sso.ugm.ac.id/cas/login;jsessionid={{jsessionid}} | cross-checked implementation |

The production client ID observed is e6abd4e380a5462e83873fe22ab8c219yVaU. The development client ID observed is qYzXDQTIkQJT8icPqGAx0TP8. These are public OAuth identifiers, not secrets.

### OAuth authorize

Method: GET (standard OAuth and URL construction evidence).

Observed query fields:

~~~text
response_type=code
client_id={{oauth_client_id}}
redirect_uri={{oauth_redirect_uri}}
scope={{oauth_scope}}
~~~

Production redirect: id.ac.ugm.student.vnext.simaster://oauth2

Development strings include redirect_uri=https://oauth.pstmn.io/v1/browser-callback.

The public `kkn-automation-standalone` implementation cross-checks the production scope string as:

~~~text
user.read user.read-write-update userDetail.read alumni.read student pegawai.all.read pegawai.unit.read staff tte notif.create-read ldap parent.read parent.read-write-update mygate.read-write vehicle.read transgama.read
~~~

This repository is corroborating evidence for the flow, not an official SIMASTER API specification. Its source is publicly visible at https://github.com/vityasyyy/kkn-automation-standalone.

Illustrative browser redirect, inferred:

~~~text
{{oauth_redirect_uri}}?code={{authorization_code}}&state={{state}}
~~~

### OAuth token exchange

Method: POST and form encoding are inferred from the OAuth flow.

Illustrative body:

~~~text
grant_type=authorization_code
code={{authorization_code}}
redirect_uri={{oauth_redirect_uri}}
client_id={{oauth_client_id}}
client_secret={{oauth_client_secret}}
~~~

Illustrative response, inferred:

~~~json
{
  "access_token": "<redacted>",
  "refresh_token": "<redacted>",
  "token_type": "Bearer",
  "expires_in": 900,
  "scope": "user.read user.read-write-update userDetail.read alumni.read student pegawai.all.read pegawai.unit.read staff tte notif.create-read ldap parent.read parent.read-write-update mygate.read-write vehicle.read transgama.read"
}
~~~

The APK contains access_token, refresh_token, and expiration key names. The actual server envelope was not captured.

### Cross-checked OAuth/SSO sequence

The repository implementation provides the following more complete sequence, which was also verified once against the production OAuth service with a user-provided account:

1. `GET https://oauth.simaster.ugm.ac.id/oauth/authorize` with `response_type=code`, `client_id`, `redirect_uri`, and `scope`. It extracts an SSO `JSESSIONID` cookie and a hidden `lt` field from the HTML.
2. `POST https://sso.ugm.ac.id/cas/login?service={{oauth_service_url}}` with `username`, `password`, `lt`, `_eventId=submit`, and `submit=LOGIN`, preserving the `JSESSIONID` cookie. The returned HTML form uses this no-`;jsessionid` action; the repository implementation also supports the `;jsessionid` form. A successful response is expected to be a 302 containing a CAS `ticket` in `Location`.
3. `GET https://oauth.simaster.ugm.ac.id/oauth/authorize` again with the same OAuth parameters plus `ticket={{oauth_ticket}}` to establish the OAuth session. The observed cookie name was `ugm_oauth_session` (the APK/repository may refer to it generically as `session`).
4. `GET` the same authorize URL to obtain the consent page, then `POST` it with `confirm=Izinkan` and `csrf_token` when present. A 302 `Location` is expected to contain `code={{authorization_code}}` and the custom redirect URI.
5. `POST https://oauth.simaster.ugm.ac.id/oauth/token` as `application/x-www-form-urlencoded` with `grant_type`, `code`, `redirect_uri`, `client_id`, and `client_secret`.

Observed token response metadata from the authorized exchange: HTTP `200`; JSON keys `access_token`, `refresh_token`, `scope`, `token_type`, and `expires_in`; `token_type` was `Bearer` and `expires_in` was `900` seconds. Token values are intentionally not recorded.

The repository source contains a hardcoded OAuth client secret in `src/utils/attendance.py`. The value is intentionally redacted from this catalog and Postman collection. Treat it as compromised: rotate/revoke it and check repository history, forks, CI logs, and deployed OAuth registrations.

### Legacy web authentication endpoints

The same implementation also documents a separate cookie-based web login:

| Method | Endpoint | Request |
|---|---|---|
| POST | `https://simaster.ugm.ac.id/services/simaster/service_login` | form fields `aId`, `username`, `password`; success is checked as JSON `isLogin == 1`. |
| GET | `https://simaster.ugm.ac.id/ugmfw/signin_simaster/signin_proses` | fallback entry point; expected to redirect to UGM SSO. |
| GET | `https://simaster.ugm.ac.id/ugmfw/signin_simaster/captcha_sound/?_={{captcha_cachebuster}}` | captcha image when the fallback requires it. |
| POST | `https://simaster.ugm.ac.id/ugmfw/signin_simaster/captchasound_verification` | form fields `simasterUGM_token` and `captcha`. |

These endpoints create web sessions/cookies and are distinct from the OAuth bearer-token flow.

## Attendance and presensi

All relative paths below use {{api_base}}.

| Likely method | Path | Evidence and confidence |
|---|---|---|
| GET/POST | /vnext/v1/checkpoint/get_active_session/ | Path observed; client symbols include getActiveCheckpointSession and fetchActiveCheckpointSession6. Method inferred. |
| POST | /vnext/v1/checkpoint/checkin/ | Path observed; client symbols include checkInWithQRCode and scanEntrantCheckpoint. Method inferred. |
| POST | /vnext/v1/checkpoint/checkout/ | Path observed; client contains checkOutCheckpoint, checkPointLogId, and check_point_log_check_in. Method inferred. |
| GET | /vnext/v1/checkpoint/get_active_session/{{username}} | Username path parameter and response fields `check_point_log_id`, `check_point_nama`, and `check_point_log_check_in` cross-checked against the public automation implementation. |
| POST | /vnext/v1/checkpoint/checkin/{{username}}/{{qr_code}}?lat={{latitude}}&long={{longitude}} | Username/QR path parameters and lat/long query parameters cross-checked against the public automation implementation; no JSON body used there. |
| GET/POST | /vnext/v1/fitur/1/ | Path observed; method unknown. |
| POST | https://simaster.ugm.ac.id/presensivnext/api/signin | Full URL observed; method inferred. |
| POST | https://simaster.ugm.ac.id/presensivnext/api/send_peserta | Full URL observed; method inferred. |

### Active checkpoint/session

Illustrative request:

~~~http
GET {{api_base}}/vnext/v1/checkpoint/get_active_session/
Authorization: Bearer {{access_token}}
Accept: application/json
~~~

Illustrative response shape only:

~~~json
{
  "data": {
    "session_id": "{{checkpoint_session_id}}",
    "status": "active",
    "start_at": "2026-11-23T07:00:00+07:00",
    "end_at": "2026-11-23T09:00:00+07:00",
    "latitude": -7.7707,
    "longitude": 110.3775,
    "radius": 100
  }
}
~~~

Exact fields/envelope are not confirmed.

### Check-in

Illustrative request template. Field casing and required fields are inferred from symbols and model names; validate in authorized staging:

~~~http
POST {{api_base}}/vnext/v1/checkpoint/checkin/
Authorization: Bearer {{access_token}}
Content-Type: application/json
~~~

~~~json
{
  "microQrCode": "{{qr_code}}",
  "latitudeGps": -7.7707,
  "longitudeGps": 110.3775,
  "accuracy": 8.5,
  "checkPointLogId": "{{checkpoint_log_id}}",
  "check_point_log_check_in": "{{checkin_record_id}}"
}
~~~

Illustrative response:

~~~json
{
  "success": true,
  "message": "Check-in successful",
  "data": {
    "attendance_id": "{{attendance_id}}",
    "status": "present",
    "checked_in_at": "2026-11-23T07:42:10+07:00"
  }
}
~~~

### Checkout

Illustrative request:

~~~http
POST {{api_base}}/vnext/v1/checkpoint/checkout/
Authorization: Bearer {{access_token}}
Content-Type: application/json
~~~

~~~json
{
  "checkPointLogId": "{{checkpoint_log_id}}",
  "check_point_log_check_in": "{{checkin_record_id}}",
  "latitudeGps": -7.7707,
  "longitudeGps": 110.3775
}
~~~

Illustrative response:

~~~json
{
  "success": true,
  "message": "Checkout successful",
  "data": {
    "status": "completed",
    "checked_out_at": "2026-11-23T09:01:12+07:00"
  }
}
~~~

## Legacy microservice routes

| Route | micro value | Observed service names |
|---|---|---|
| {{api_base}}/v1/student/micro/services | student | get_jadwal_anda_studi_student; get_studi_krs; get_berita_list_student; get_jadwal_anda_date_student; get_studi_jadwal_periode; get_student_pengumuman_by_niu; get_banner_student; get_student_oss_list_by_niu; get_riwayat_keaktifan_mhs; get_student_beasiswa_tawar_by_tahun; get_student_jadwal_ujian_by_niu; get_jadwal_anda_agenda_student; get_konfirmasi_qr_student; get_agenda_kehadiran_rekap_student; get_studi_kehadiran_rinci; get_studi_kehadiran_rekap; get_agenda_kehadiran_rinci_student; get_riwayat_pembayaran_mhs; get_student_profil_by_niu; get_student_oss_detail_by_id; get_riwayat_permohonan_mhs; get_jadwal_anda_now_student; get_studi_max_sks; get_student_sks__semester_by_niu; get_semester_student; get_student_krs_by_niu_semester |
| {{api_base}}/v1/staff/micro/services | staff | get_staff_profil_by_staff_id; get_simaster_atribut_by_username |
| {{api_base}}/v1/staff/micro/doscanent | unknown | Route observed; body/service contract not recovered. |

Likely method: POST (inferred from request-template strings).

Generic request:

~~~http
POST {{api_base}}/v1/student/micro/services
Authorization: Bearer {{access_token}}
Content-Type: application/json
~~~

~~~json
{
  "micro": "student",
  "service": "get_studi_kehadiran_rekap",
  "query": [],
  "param": [
    [
      [
        "{{student_niu}}",
        "{{semester_id}}"
      ]
    ]
  ]
}
~~~

The outer JSON shape and service names are observed. The nested values are placeholders because the complete runtime parameters were not preserved in static strings.

Attendance-related service names:
- get_studi_kehadiran_rekap
- get_studi_kehadiran_rinci
- get_agenda_kehadiran_rekap_student
- get_agenda_kehadiran_rinci_student
- get_konfirmasi_qr_student

Schedule services used to populate presensi context:
- get_jadwal_anda_now_student
- get_jadwal_anda_agenda_student
- get_jadwal_anda_date_student
- get_studi_jadwal_periode
- get_jadwal_anda_studi_student

Illustrative microservice response:

~~~json
{
  "success": true,
  "data": [
    {
      "id": "{{record_id}}",
      "status": "present",
      "date": "2026-11-23",
      "course_code": "{{course_code}}"
    }
  ]
}
~~~

This response is illustrative. Model names such as StudyAttendanceDetailModel, StudyAttendanceRecapModel, and RecapPresenceAgendaModel are observed, but the actual response was not captured.

## Other first-party vNEXT paths

| Likely method | Path | Evidence |
|---|---|---|
| GET | /vnext/v1/students/ | Student resource path observed. |
| GET | /vnext/v1/feeds | Feed listing observed. |
| GET | /vnext/v1/feeds/krisis | Crisis feed observed. |
| GET | /vnext/v1/kontak-krisis | Crisis contact path observed. |
| GET | /vnext/v1/news?unit_id=1&limit={{limit}} | unit_id and limit observed. |
| GET | /vnext/v1/news?unit_id=1&limit=7&skip=0 | Exact query form observed. |
| GET | /vnext/v1/banners/get_dynamic?banner_key=pengumuman_home&banner_app={{banner_app}} | Query form observed. |
| GET | /vnext/v1/banners/get_dynamic?banner_key=tosparent&banner_app=1 | Exact query observed. |
| GET | /vnext/v1/banners/get_intro?banner_app=1 | Exact query observed. |
| GET | /vnext/v1/vehicles | Vehicle listing observed. |
| GET | /vnext/v1/vehicles/ | Vehicle resource path observed. |
| GET | /vnext/v1/vehicle/user-rules | Vehicle rules observed. |
| GET | /vnext/v1/portal/vehicle?vehicle_plate_number={{plate_number}} | Query key observed. |
| GET/POST | /vnext/v1/parking-areas | Path observed; method unknown. |
| GET | /vnext/v1/transgama/lanes/ | Lane listing observed. |
| GET | /vnext/v1/transgama/lanes/?show_deleted=false | Exact query observed. |
| GET | /vnext/v1/transgama/lane/{{lane_id}} | Detail path inferred from the observed lane/ID route pattern. |
| GET | /vnext/v1/transgama/halte/terdekat?lat={{latitude}} | lat observed; other coordinates may be appended at runtime. |
| GET | /vnext/v1/transgama/rute?halte_awal_id={{halte_awal_id}} | Query key observed. |
| GET | /vnext/v1/transgama/jadwal/halte?lane_id={{lane_id}} | Query key observed. |

## Additional non-presensi network candidates recovered in the deeper pass

These strings are not part of the normal vNEXT path family above. They are likely legacy or feature-specific network routes, but the APK does not preserve enough call-site information to prove the base host, HTTP method, headers, or request/response schema. They should be validated only in an authorized development or staging environment.

| Candidate path | Confidence | Static evidence / interpretation |
|---|---|---|
| `/auth/mobile/exchange` | medium | Exact auth-looking path observed in the native snapshot; likely a mobile auth/token exchange route. Base host, method, headers, and body not recovered. |
| `/vnext/check-user?username={{username}}` | medium | Exact query-bearing path observed near `my_gate_form` model strings; likely a My Gate user lookup. Base host and method not recovered. |
| `/get_menu_vnext?sesId={{ses_id}}` | low/medium | Exact query-bearing path observed near dashboard/service strings; likely a legacy menu/session lookup. Base host and method not recovered. |
| `/service_login_vnext` | low/medium | Exact path observed in the native snapshot; likely a legacy service-login route. Base host, method, and body not recovered. |
| `/ongoing_vnext` | low/medium | Exact path observed in the native snapshot near shared networking strings; likely an ongoing-status route. Base host, method, and body not recovered. |
| `/tugas-akhir/bimbingan/` | low/medium | Exact resource-looking path observed near thesis/guidance model strings; likely a feature resource. Contract not recovered. |
| `/tugas-akhir/dosen_pembimbing/` | low/medium | Exact resource-looking path observed near OSS/history and guidance strings; likely a feature resource. Contract not recovered. |
| `/haltes` | low/medium | Exact path observed near vehicle-rule and study-schedule strings; may be a transport/halte resource. Contract not recovered. |
| `/putaran/` | low | Exact path observed near attendance recap model strings; likely a feature/resource fragment. Contract not recovered. |
| `/logs/` | low | Exact path observed near OSS history-flow strings; may be a history/log resource. Contract not recovered. |
| `/users/` | low | Exact path observed; could be an API resource or a screen/resource identifier. Runtime call-site not proven. |

The Postman collection includes documentation requests for these candidates pointed at the development API variable by default. That default is only a safer placeholder; it does not prove that the development host serves the route.

## SDK and external network endpoints

The bundled Android DEX files also contain this SDK endpoint:

| Likely method | Endpoint | Confidence |
|---|---|---|
| GET or POST | `https://accounts.google.com/o/oauth2/revoke?token={{google_token}}` | URL observed; method inferred from SDK behavior not recovered |

Additional SDK/telemetry URL prefixes recovered from the Android DEX files:

| URL or prefix | Classification | Notes |
|---|---|---|
| `https://fcmregistrations.googleapis.com/v1/projects/` | Firebase Cloud Messaging | Prefix observed; project and registration suffixes are runtime values. |
| `https://firebase-settings.crashlytics.com/spi/v2/platforms/android/gmp/%s/settings` | Firebase Crashlytics settings | Exact format string observed; `%s` is a runtime app/project identifier. |
| `https://pagead2.googlesyndication.com/pagead/gen_204?id=gmob-apps` | Google mobile telemetry | URL observed in bundled Google SDK. |
| `https://www.googleadservices.com/pagead/conversion/app/deeplink?id_type=adid&sdk_version=%s&rdid=%s&bundleid=%s&retry=%s` | Google mobile telemetry/attribution | Exact format string observed; values are runtime identifiers. |
| `https://accounts.google.com` | Google sign-in host | Host observed alongside Google Sign-In classes. |

The DEX files also contain Google OAuth scope URIs (`https://www.googleapis.com/auth/...`) and issue/documentation URLs. Those are scope identifiers or SDK documentation, not API endpoints exposed by SIMASTER.

Other embedded URLs are content pages, image/CDN URLs, telemetry or store/documentation links. They are listed in the integrations section for completeness but should not be treated as SIMASTER API contracts.

## Flutter navigation routes (not backend endpoints)

The native snapshot contains these slash-prefixed application route names. They are included here so they are not mistaken for omitted APIs:

`/home`, `/not-found`, `/oss_v2`, `/oauth-webview`, `/oauth-webview-old`, `/historyflowview`, `/checkpoint`, `/ScopeValidityDialog`, `/setting`, `/crisis-center`, `/news`, `/other-menus`, `/e-service`, `/course-schedule`, `/forgot-password`, `/onboarding`, `/study-result`, `/login-facebook`, `/login-google`, `/attedence`, `/my-gate-vehicle`, `/my-gate`, `/exam-schedule`, `/login-callback`, `/my-gate-vehicle-details`, `/my-gate-form`, `/notification`, `/course`, `/transgama`, `/auth`, `/history-scholarship`, `/study`, `/home-information-detail-service-clinic`, `/rsa-news-detail`, `/dashboard`, `/watch`, `/login-ugm`, `/landing`, `/history-registration`, `/simaster-core`, `/scholarship`, `/organizer-attendance`, `/ruang-hijau`, `/personal-qr`, `/qr-scanner`, `/e-ktm`, `/bimbingan`, `/parent`, and `/achievement`.

These are GetX/Flutter navigation or screen identifiers based on their location among app package strings. They are not included as HTTP requests in Postman.

## Additional static findings from the deeper pass

- The release native snapshot contains development, preview, staging, and checking environment labels alongside production configuration. This increases release-configuration and environment-isolation risk.
- A build-machine path is embedded: `file:///mnt/Hildr/1._Kerja/UGM/vNext/simaster-mahasiswa/.dart_tool/flutter_build/dart_plugin_registrant.dart`. This is information disclosure and should be removed from release artifacts where practical.
- A password-like test literal appears near `package:simaster_ugm/app/modules/testing/svg_test.dart`: `_Ini_Adalah_Password_#_Login_*_JWT_Username_Ku_`. It was not proven to be an active credential, but it should be treated as a secret candidate and rotated or removed if valid.
- The app contains `access_token`, `refresh_token`, and expiration key names and uses ordinary shared-preference persistence strings. Confirm at runtime whether tokens are stored in Android Keystore-backed storage and whether logout revokes/clears all tokens.

## Other embedded integrations

These are application feature/content URLs, not necessarily SIMASTER API routes:

- https://ruanghijau.ugm.ac.id/ugm-akses/api
- https://armada.smartcampus.ugm.ac.id/gps/
- https://armada.smartcampus.ugm.ac.id/gps/?from={{origin}}
- wss://armada.smartcampus.ugm.ac.id/gps/ws?room=room2
- https://simaster.ugm.ac.id/services/simaster
- https://simaster.ugm.ac.id/beranda/beranda/
- https://laporkan.ugm.ac.id/
- https://ult.ugm.ac.id/eservices/portal/
- https://rsa.ugm.ac.id/klinik-eksekutif-rsa-ugm/
- https://dti.ugm.ac.id/knowledge-base
- https://simaster.ugm.ac.id/ugmfw-assets/images/notfound.png

Other external URLs include WhatsApp, YouTube, Google avatar/placeholder services, Google Fonts, iTunes lookup, and Play Store links. XML namespace and Flutter documentation URLs were excluded.

Exact third-party/content URLs also present in the native library include:

- https://wa.me/
- https://wa.me/08112857027
- https://wa.me/628112826546
- https://ui-avatars.com/api/?name=-&background=DBDFE9&color=fff
- https://ui-avatars.com/api/?name=
- https://via.placeholder.com/72x72
- https://i3.ytimg.com/
- https://www.youtube-nocookie.com
- https://itunes.apple.com/lookup
- https://fonts.gstatic.com/s/a/
- https://play.google.com/store/apps/details?id=id.ac.ugm.student.vnext.pitugm
- https://play.google.com/store/apps/details?id=id.ac.ugm.student.vnext.simaster

## Presensi flow reconstructed from the APK

1. OAuth login obtains access/refresh tokens.
2. The app loads current schedule/agenda and attendance context through vNEXT and/or legacy microservices.
3. It obtains an active checkpoint session.
4. The user scans a QR code or enters a code; the client constructs a check-in request with QR/session/checkpoint and location-related values.
5. The client performs local mock-location and radius checks.
6. The server should validate the authenticated student, schedule/session, QR nonce, time window, geofence policy, and state transition before recording attendance.
7. The app loads attendance recap/detail after success.
8. Checkout uses the server-created checkpoint log and must be bound to the same user/session.

## Next capture needed for authoritative examples

In authorized staging, capture one success and one rejection for active-session lookup, QR check-in, duplicate/replayed QR check-in, checkout, recap/detail, and OAuth token exchange. Redact tokens, authorization codes, QR contents, student IDs, exact coordinates, and personal data before sharing. Then replace the inferred Postman examples with the actual methods, headers, body fields, response envelope, and error codes.

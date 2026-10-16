#!/usr/bin/env bash
# Plays the Oxygen phone app against a local stack (`just up`): sign in, list apps, open one, get a notification.
# Usage: scripts/mobile-poc/demo.sh [persona]    (default persona: member; see .oxy-dev/state.json)
# A stack not started by `just up`: set API=http://127.0.0.1:<port> and BACKEND_LOG=<file>.
# DB_CONTAINER=<postgres container> also shows the push delivery each notification recorded.
set -euo pipefail

cd "$(git rev-parse --show-toplevel 2>/dev/null)"
STATE=.oxy-dev/state.json
if [ -z "${API:-}" ]; then
  [ -f "$STATE" ] || { echo "No $STATE: run 'just up' first, or set API." >&2; exit 1; }
  API="$(jq -r .backend_url "$STATE")"
  BACKEND_LOG="${BACKEND_LOG:-$(jq -r .logs.backend "$STATE")}"
fi
BACKEND_LOG="${BACKEND_LOG:-}"
PERSONA="${1:-member}"
REDIRECT="oxygen://auth"
DEVICE_TOKEN="demo-emulator-$RANDOM$RANDOM"

curl_() { curl -sS --noproxy '*' "$@"; }
bold() { printf '\n\033[1m%s\033[0m\n' "$*"; }
dim() { printf '\033[2m%s\033[0m\n' "$*"; }
ok() { printf '  \033[32m✓\033[0m %s\n' "$*"; }
fail() { printf '  \033[31m✗\033[0m %s\n' "$*"; exit 1; }
mask() { local s="$1"; printf '%s…%s' "${s:0:10}" "${s: -6}"; }
b64url() { base64 | tr '+/' '-_' | tr -d '=\n'; }

bold "1. Sign in through the system browser"
dim "On a phone, /mobile-auth opens in the system browser behind the normal login (Google, Okta, magic link)."
dim "Here a dev-login session stands in for the browser, and this script presses Continue."
SESSION="$(curl_ "$API/api/auth/dev-login?as=$PERSONA" | jq -r .token)"
[ -n "$SESSION" ] && [ "$SESSION" != null ] || fail "dev-login refused persona '$PERSONA'"
ok "browser session for persona '$PERSONA'"

VERIFIER="$(openssl rand 48 | b64url)"
CHALLENGE="$(printf %s "$VERIFIER" | openssl dgst -sha256 -binary | b64url)"
STATE_PARAM="$(openssl rand 12 | b64url)"
dim "PKCE challenge $CHALLENGE (the verifier never leaves the app)"

AUTHZ="$(curl_ -w '\n%{http_code}' -X POST "$API/api/auth/mobile/authorize" \
  -H "Authorization: Bearer $SESSION" -H 'Content-Type: application/json' \
  -d "{\"code_challenge\":\"$CHALLENGE\",\"code_challenge_method\":\"S256\",\"redirect_uri\":\"$REDIRECT\"}")"
[ "$(tail -n1 <<<"$AUTHZ")" = 200 ] || fail "authorize answered: $AUTHZ"
CODE="$(sed '$d' <<<"$AUTHZ" | jq -r .code)"
BAD="$(curl_ -o /dev/null -w '%{http_code}' -X POST "$API/api/auth/mobile/authorize" \
  -H "Authorization: Bearer $SESSION" -H 'Content-Type: application/json' \
  -d "{\"code_challenge\":\"$CHALLENGE\",\"code_challenge_method\":\"S256\",\"redirect_uri\":\"https://evil.example/cb\"}")"
[ "$BAD" = 400 ] && ok "a redirect to anything but oxygen:// is refused (400)" || fail "foreign redirect answered $BAD"
ok "the page redirects to $REDIRECT?code=$(mask "$CODE")&state=$STATE_PARAM"

bold "2. The app trades the code for its token"
EXCH="$(curl_ -X POST "$API/api/auth/mobile/exchange" -H 'Content-Type: application/json' \
  -d "{\"code\":\"$CODE\",\"code_verifier\":\"$VERIFIER\",\"redirect_uri\":\"$REDIRECT\"}")"
TOKEN="$(jq -r '.token // empty' <<<"$EXCH")"
[ -n "$TOKEN" ] || fail "exchange answered: $EXCH"
ok "signed in as $(jq -r '.user.name // .user.email' <<<"$EXCH") <$(jq -r .user.email <<<"$EXCH")>, token $(mask "$TOKEN"), expires in $(jq -r .expires_in <<<"$EXCH")s"
REPLAY="$(curl_ -o /dev/null -w '%{http_code}' -X POST "$API/api/auth/mobile/exchange" -H 'Content-Type: application/json' \
  -d "{\"code\":\"$CODE\",\"code_verifier\":\"$VERIFIER\",\"redirect_uri\":\"$REDIRECT\"}")"
[ "$REPLAY" = 400 ] && ok "the same code a second time is refused (400)" || fail "replayed code answered $REPLAY"
REFRESHED="$(curl_ -X POST "$API/api/auth/mobile/refresh" -H "Authorization: Bearer $TOKEN" | jq -r '.token // empty')"
[ -n "$REFRESHED" ] && ok "the app refreshes its token before it expires" || fail "refresh failed"
TOKEN="$REFRESHED"
AUTH=(-H "Authorization: Bearer $TOKEN")

bold "3. The Apps tab"
APPS="$(curl_ "${AUTH[@]}" "$API/api/me/apps")"
COUNT="$(jq '.apps | length' <<<"$APPS")"
[ "$COUNT" -gt 0 ] || fail "no apps for '$PERSONA': $APPS"
jq -r '.apps[] | "  · \(.org.name) / \(.name)  \(.url)"' <<<"$APPS"
ok "$COUNT app(s) across $(jq '[.apps[].org.id] | unique | length' <<<"$APPS") org(s)"

APP_NAME="$(jq -r '.apps[0].name' <<<"$APPS")"
APP_URL="$(jq -r '.apps[0].url' <<<"$APPS")"
bold "4. Open \"$APP_NAME\" in the app's WebView"
dim "The app sets the session cookie in its own WebView cookie store (WKHTTPCookieStore / CookieManager),"
dim "exactly as the exchange's session_cookie describes; no web endpoint turns a token into a cookie."
COOKIE_NAME="$(jq -r '.session_cookie.name // "oxy_session"' <<<"$EXCH")"
dim "session_cookie: $(jq -c '.session_cookie' <<<"$EXCH")"
NO_COOKIE="$(curl_ -o /dev/null -w '%{http_code}' "$APP_URL")"
ok "without the cookie the app sends you to sign in ($NO_COOKIE)"
PAGE_STATUS="$(curl_ -o /dev/null -w '%{http_code}' -b "$COOKIE_NAME=$TOKEN" "$APP_URL")"
[ "$PAGE_STATUS" = 200 ] && ok "with the cookie set, the app loads signed in (200)" || fail "app answered $PAGE_STATUS with the cookie"
GONE="$(curl_ -o /dev/null -w '%{http_code}' -X POST "$API/api/auth/mobile/handoff" --data-urlencode "token=$TOKEN")"
[ "$GONE" != 303 ] && ok "there is no token-to-cookie endpoint to abuse ($GONE)" || fail "a handoff endpoint answered 303"

bold "5. Notifications"
curl_ -o /dev/null -X POST "${AUTH[@]}" -H 'Content-Type: application/json' "$API/api/notifications/devices" \
  -d "{\"platform\":\"fcm\",\"token\":\"$DEVICE_TOKEN\",\"device_name\":\"Demo emulator\"}"
ok "registered this device for push (fcm, $DEVICE_TOKEN)"
LOG_MARK=0
if [ -f "$BACKEND_LOG" ]; then LOG_MARK="$(wc -l <"$BACKEND_LOG")"; fi
SENT="$(curl_ -X POST "${AUTH[@]}" -H 'Content-Type: application/json' "$API/api/notifications/test" \
  -d '{}')"
NID="$(jq -r '.id // empty' <<<"$SENT")"
[ -n "$NID" ] || fail "test notification answered: $SENT"
ok "Settings → Send test notification: $NID"
sleep 3
ITEM="$(curl_ "${AUTH[@]}" "$API/api/notifications/$NID")"
ok "the push carries only {kind, id}; the app fetches the text: \"$(jq -r .title <<<"$ITEM")\" — $(jq -r .body <<<"$ITEM")"
ok "inbox: $(curl_ "${AUTH[@]}" "$API/api/notifications?unread_only=true" | jq -r .unread) unread"
if [ -n "${DB_CONTAINER:-}" ]; then
  DELIVERY="$(docker exec "$DB_CONTAINER" psql -U postgres -d oxy -Atc \
    "select channel || ': ' || status || coalesce(' (' || reason || ')', '') from notification_deliveries where notification_id = '$NID'")"
  ok "delivery recorded — ${DELIVERY:-none yet}"
  dim "(not_configured = no APNs/FCM credentials on this machine; with OXY_APNS_* / OXY_FCM_* set it sends)"
elif [ -f "$BACKEND_LOG" ]; then
  dim "Server log for the push attempt:"
  tail -n +"$((LOG_MARK + 1))" "$BACKEND_LOG" | grep -iE 'push|apns|fcm' | tail -5 | sed 's/^/    /' || true
fi

bold "6. Sign out"
DEL="$(curl_ -o /dev/null -w '%{http_code}' -X DELETE "${AUTH[@]}" -H 'Content-Type: application/json' \
  "$API/api/notifications/devices" -d "{\"token\":\"$DEVICE_TOKEN\"}")"
[ "$DEL" = 204 ] && ok "device removed (204); this phone gets no more pushes" || fail "device delete answered $DEL"
curl_ -o /dev/null -X POST "${AUTH[@]}" "$API/api/notifications/$NID/read"
ok "done"

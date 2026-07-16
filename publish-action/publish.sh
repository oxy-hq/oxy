#!/usr/bin/env bash
# Trusted-publishing upload. Fails closed at every step.
set -euo pipefail

need() { command -v "$1" >/dev/null || { echo "::error::missing dependency: $1"; exit 1; }; }
need curl; need jq; need tar

: "${OXY_APP:?app is required (org-slug/app-slug)}"
: "${OXY_DIR:?dir is required}"
: "${OXY_TARGET:?target is required}"
: "${ACTIONS_ID_TOKEN_REQUEST_URL:?this job needs 'permissions: id-token: write'}"
: "${ACTIONS_ID_TOKEN_REQUEST_TOKEN:?this job needs 'permissions: id-token: write'}"

org_slug="${OXY_APP%%/*}"
app_slug="${OXY_APP##*/}"
if [[ "$org_slug" == "$OXY_APP" || -z "$app_slug" ]]; then
  echo "::error::app must be 'org-slug/app-slug' (got '$OXY_APP')"; exit 1
fi
if [[ ! -f "$OXY_DIR/index.html" ]]; then
  echo "::error::$OXY_DIR has no index.html — is this the built bundle?"; exit 1
fi

# 1. Mint the OIDC token for OUR audience (must match the server's pinned value).
echo "→ requesting OIDC token"
oidc="$(curl -sSf -H "Authorization: bearer ${ACTIONS_ID_TOKEN_REQUEST_TOKEN}" \
  "${ACTIONS_ID_TOKEN_REQUEST_URL}&audience=oxy-publish" | jq -r '.value')"
[[ -n "$oidc" && "$oidc" != "null" ]] || { echo "::error::could not obtain OIDC token"; exit 1; }

# 2. Exchange it for a short-lived, app-scoped publish token.
echo "→ exchanging for a publish credential"
exchange="$(curl -sS -w '\n%{http_code}' \
  -H "Authorization: Bearer ${oidc}" \
  -H "Content-Type: application/json" \
  -d "$(jq -nc --arg app "$OXY_APP" '{app:$app}')" \
  "${OXY_TARGET%/}/api/customer-apps/publish/oidc-exchange")"
body="$(echo "$exchange" | sed '$d')"; code="$(echo "$exchange" | tail -1)"
if [[ "$code" != "200" ]]; then
  echo "::error::exchange failed ($code): $body"
  echo "  is this workflow registered as a publisher for $OXY_APP, and the app's client consenting?"
  exit 1
fi
token="$(echo "$body" | jq -r '.token')"
[[ -n "$token" && "$token" != "null" ]] || { echo "::error::exchange returned no token"; exit 1; }

# 3. Resolve the project (public build-config endpoint — what `oxy publish` reads).
project_id="$(curl -sSf "${OXY_TARGET%/}/api/apps/${org_slug}/${app_slug}/build-config" | jq -r '.project_id')"
[[ -n "$project_id" && "$project_id" != "null" ]] || { echo "::error::could not resolve project for $OXY_APP"; exit 1; }

# 4. Tar the bundle and upload. The exchanged token authorizes exactly this app.
echo "→ publishing ${OXY_APP} (promote=${OXY_PROMOTE:-false})"
tarball="$(mktemp -t oxy-bundle-XXXX.tgz)"
tar -czf "$tarball" -C "$OXY_DIR" .

pub="$(curl -sS -w '\n%{http_code}' \
  -H "Authorization: Bearer ${token}" \
  -F "org=${org_slug}" \
  -F "app=${app_slug}" \
  -F "project_id=${project_id}" \
  -F "promote=${OXY_PROMOTE:-false}" \
  ${GITHUB_SHA:+-F "commit_sha=${GITHUB_SHA}"} \
  -F "bundle=@${tarball};type=application/gzip" \
  "${OXY_TARGET%/}/api/customer-apps/publish")"
rm -f "$tarball"
pbody="$(echo "$pub" | sed '$d')"; pcode="$(echo "$pub" | tail -1)"
if [[ "$pcode" != "200" ]]; then
  echo "::error::publish failed ($pcode): $pbody"; exit 1
fi
url="$(echo "$pbody" | jq -r '.url // empty')"
echo "✓ published ${OXY_APP}${url:+ → $url}"

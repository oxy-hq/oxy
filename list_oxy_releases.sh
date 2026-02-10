#!/bin/bash
set -euo pipefail

# Configuration
STABLE_REPO="oxy-hq/oxy"
NIGHTLY_REPO="oxy-hq/oxy-nightly"
API_BASE="https://api.github.com/repos"

# Defaults
CHANNEL="all"
LIMIT=10

usage() {
	cat <<'EOF'
List available Oxy releases across all channels.

Usage:
  bash <(curl -sSf https://release.oxy.tech) [OPTIONS]

Options:
  -c, --channel CHANNEL   Filter by channel: stable, edge, nightly, or all (default: all)
  -n, --limit N           Number of releases to show per channel (default: 10)
  -h, --help              Show this help message

Examples:
  bash <(curl -sSf https://release.oxy.tech)
  bash <(curl -sSf https://release.oxy.tech) --channel stable
  bash <(curl -sSf https://release.oxy.tech) -c nightly -n 20
EOF
	exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
	case "$1" in
	--channel | -c)
		CHANNEL="${2:-}"
		shift 2
		;;
	--limit | -n)
		LIMIT="${2:-10}"
		shift 2
		;;
	--help | -h) usage ;;
	*)
		echo "Unknown option: $1"
		echo "Run with --help for usage."
		exit 1
		;;
	esac
done

# Validate
case "$CHANNEL" in
all | stable | edge | nightly) ;;
*)
	echo "Invalid channel: $CHANNEL (expected: all, stable, edge, nightly)"
	exit 1
	;;
esac

if ! [[ "$LIMIT" =~ ^[0-9]+$ ]] || [ "$LIMIT" -lt 1 ]; then
	echo "Invalid limit: $LIMIT (must be a positive integer)"
	exit 1
fi

if ! command -v curl &>/dev/null; then
	echo "Error: curl is required but not found."
	exit 1
fi

# JSON parsing: prefer jq, fall back to python3
if command -v jq &>/dev/null; then
	JSON_CMD="jq"
elif command -v python3 &>/dev/null; then
	JSON_CMD="python3"
else
	echo "Error: This script requires 'jq' or 'python3' to parse GitHub API responses."
	echo "Install jq: https://jqlang.github.io/jq/download/"
	exit 1
fi

# Parse releases JSON into tab-separated: tag\tdate\tname\tcommit_message
# commit_message is extracted from the release body "Message: ..." line
parse_releases_json() {
	local json="$1"
	if [ "$JSON_CMD" = "jq" ]; then
		echo "$json" | jq -r '
			.[]
			| select(.draft == false)
			| [
				.tag_name,
				(.published_at // "" | split("T") | .[0]),
				(.name // ""),
				((.body // "") | split("\n") | map(select(startswith("Message: "))) | .[0] // "" | ltrimstr("Message: "))
			  ]
			| @tsv'
	else
		echo "$json" | python3 -c "
import json, sys
for r in json.load(sys.stdin):
    if r.get('draft'): continue
    tag = r['tag_name']
    date = (r.get('published_at') or '')[:10]
    name = (r.get('name') or '').replace('\t', ' ')
    msg = ''
    for line in (r.get('body') or '').split('\n'):
        if line.startswith('Message: '):
            msg = line[9:].strip().replace('\t', ' ')
            break
    print(f'{tag}\t{date}\t{name}\t{msg}')
"
	fi
}

# Extract short SHA from a tag name
# Handles: nightly-<40char_sha>, nightly-YYYYMMDD-<sha>, edge-<sha>
sha_from_tag() {
	local tag="$1"
	local sha=""

	if [[ "$tag" =~ ^nightly-[0-9]{8}-([0-9a-f]+)$ ]]; then
		# New format: nightly-YYYYMMDD-<sha>
		sha="${BASH_REMATCH[1]}"
	elif [[ "$tag" =~ ^nightly-([0-9a-f]{40})$ ]]; then
		# Old format: nightly-<full_sha>
		sha="${BASH_REMATCH[1]}"
	elif [[ "$tag" =~ ^edge-([0-9a-f]+)$ ]]; then
		# Edge format: edge-<sha>
		sha="${BASH_REMATCH[1]}"
	fi

	echo "${sha:0:7}"
}

# Fetch releases JSON from a GitHub repo
fetch_releases() {
	local repo="$1"
	local per_page="$2"
	curl -sSf -H "Accept: application/vnd.github+json" \
		"${API_BASE}/${repo}/releases?per_page=${per_page}" 2>/dev/null || {
		echo "[]"
	}
}

# ─── Display helpers ─────────────────────────────────────────────────────────────

print_section_header() {
	local title="$1"
	local line
	line=$(printf '%.0s─' {1..60})
	echo ""
	echo "  ${title} ${line:${#title}}"
}

print_no_releases() {
	echo "  (no releases found)"
}

# ─── Render: Stable ──────────────────────────────────────────────────────────────

render_stable() {
	local json="$1"
	local count=0

	print_section_header "Stable"
	echo ""
	printf "  %-14s %s\n" "VERSION" "DATE"
	printf "  %-14s %s\n" "──────────" "──────────"

	while IFS=$'\t' read -r tag date _name _msg; do
		[ -z "$tag" ] && continue
		printf "  %-14s %s\n" "$tag" "$date"
		count=$((count + 1))
		[ "$count" -ge "$LIMIT" ] && break
	done < <(parse_releases_json "$json")

	if [ "$count" -eq 0 ]; then
		print_no_releases
	fi

	echo ""
	echo "  Install: OXY_VERSION=<version> bash <(curl -sSf https://get.oxy.tech)"
}

# ─── Render: Edge ────────────────────────────────────────────────────────────────

render_edge() {
	local json="$1"
	local count=0

	print_section_header "Edge"
	echo ""
	printf "  %-20s %-12s %-10s %s\n" "TAG" "DATE" "COMMIT" "MESSAGE"
	printf "  %-20s %-12s %-10s %s\n" "────────────────" "──────────" "───────" "───────────────────────────────"

	local tag date _name commit msg display_msg
	while IFS=$'\t' read -r tag date _name msg; do
		[ -z "$tag" ] && continue
		[[ "$tag" != edge-* ]] && continue
		commit=$(sha_from_tag "$tag")
		# Truncate long messages
		if [ "${#msg}" -gt 50 ]; then
			display_msg="${msg:0:47}..."
		else
			display_msg="$msg"
		fi
		printf "  %-20s %-12s %-10s %s\n" "$tag" "$date" "$commit" "$display_msg"
		count=$((count + 1))
		[ "$count" -ge "$LIMIT" ] && break
	done < <(parse_releases_json "$json")

	if [ "$count" -eq 0 ]; then
		print_no_releases
	fi

	echo ""
	echo "  Install: OXY_VERSION=<tag> bash <(curl -sSf https://nightly.oxy.tech)"
}

# ─── Render: Nightly ─────────────────────────────────────────────────────────────

render_nightly() {
	local json="$1"
	local count=0

	print_section_header "Nightly"
	echo ""
	printf "  %-10s %-12s %s\n" "COMMIT" "DATE" "INSTALL TAG"
	printf "  %-10s %-12s %s\n" "───────" "──────────" "──────────────────────────────────────────────────"

	# Collect and sort by date descending
	local lines=""
	local tag date _name _msg commit
	while IFS=$'\t' read -r tag date _name _msg; do
		[ -z "$tag" ] && continue
		[[ "$tag" != nightly-* ]] && continue
		commit=$(sha_from_tag "$tag")
		lines+="${commit:-n/a}"$'\t'"${date}"$'\t'"${tag}"$'\n'
	done < <(parse_releases_json "$json")

	if [ -z "$lines" ]; then
		print_no_releases
	else
		echo "$lines" | sort -t$'\t' -k2 -r | head -n "$LIMIT" | while IFS=$'\t' read -r commit date tag; do
			[ -z "$commit" ] && continue
			printf "  %-10s %-12s %s\n" "$commit" "$date" "$tag"
			count=$((count + 1))
		done
	fi

	echo ""
	echo "  Install: OXY_CHANNEL=nightly OXY_VERSION=<tag> bash <(curl -sSf https://nightly.oxy.tech)"
}

# ─── Main ────────────────────────────────────────────────────────────────────────

echo ""
echo "Oxy Releases"

# Fetch data from repos we need
STABLE_JSON=""
NIGHTLY_JSON=""

if [ "$CHANNEL" = "all" ] || [ "$CHANNEL" = "stable" ]; then
	STABLE_JSON=$(fetch_releases "$STABLE_REPO" "$LIMIT")
fi

if [ "$CHANNEL" = "all" ] || [ "$CHANNEL" = "edge" ] || [ "$CHANNEL" = "nightly" ]; then
	# Fetch enough to sort by date (API sorts by tag name, not date)
	FETCH_COUNT=$((LIMIT * 3))
	[ "$FETCH_COUNT" -lt 50 ] && FETCH_COUNT=50
	[ "$FETCH_COUNT" -gt 100 ] && FETCH_COUNT=100
	NIGHTLY_JSON=$(fetch_releases "$NIGHTLY_REPO" "$FETCH_COUNT")
fi

# Render sections
if [ "$CHANNEL" = "all" ] || [ "$CHANNEL" = "stable" ]; then
	render_stable "$STABLE_JSON"
fi

if [ "$CHANNEL" = "all" ] || [ "$CHANNEL" = "edge" ]; then
	render_edge "$NIGHTLY_JSON"
fi

if [ "$CHANNEL" = "all" ] || [ "$CHANNEL" = "nightly" ]; then
	render_nightly "$NIGHTLY_JSON"
fi

echo ""

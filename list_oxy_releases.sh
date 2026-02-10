#!/bin/bash
set -euo pipefail

# Configuration
STABLE_REPO="oxy-hq/oxy"
EDGE_REPO="oxy-hq/oxy-nightly"
API_BASE="https://api.github.com/repos"

# Defaults
CHANNEL="all"
LIMIT=20

usage() {
	echo "List available Oxy releases."
	echo ""
	echo "Usage:"
	echo "  bash <(curl -sSf https://release.oxy.tech) [OPTIONS]"
	echo ""
	echo "Options:"
	echo "  -c, --channel CHANNEL   Filter by channel: stable, edge, or all (default: all)"
	echo "  -n, --limit N           Number of releases to show per channel (default: 10)"
	echo "  -h, --help              Show this help message"
	echo ""
	echo "Examples:"
	echo "  bash <(curl -sSf https://release.oxy.tech)"
	echo "  bash <(curl -sSf https://release.oxy.tech) --channel stable"
	echo "  bash <(curl -sSf https://release.oxy.tech) -c edge -n 20"
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
all | stable | edge) ;;
*)
	echo "Invalid channel: $CHANNEL (expected: all, stable, edge)"
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

# JSON parsing: prefer jq, fall back to python3/python
if command -v jq &>/dev/null; then
	JSON_CMD="jq"
elif command -v python3 &>/dev/null; then
	JSON_CMD="python3"
elif command -v python &>/dev/null; then
	JSON_CMD="python"
else
	echo "Error: This script requires 'jq' or 'python3' to parse GitHub API responses."
	echo "Install jq: https://jqlang.github.io/jq/download/"
	exit 1
fi

# Parse releases JSON into tab-separated: tag\tdate\tname\tcommit_message
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
		echo "$json" | "$JSON_CMD" -c "
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
    print('%s\t%s\t%s\t%s' % (tag, date, name, msg))
"
	fi
}

# Extract short SHA from edge tag: edge-<sha>
sha_from_tag() {
	local tag="$1"
	if [[ "$tag" =~ ^edge-([0-9a-f]+)$ ]]; then
		echo "${BASH_REMATCH[1]:0:7}"
	fi
}

# Fetch releases JSON from a GitHub repo (single page)
fetch_releases_page() {
	local repo="$1"
	local per_page="$2"
	local page="${3:-1}"
	curl -sSf -H "Accept: application/vnd.github+json" \
		"${API_BASE}/${repo}/releases?per_page=${per_page}&page=${page}" 2>/dev/null || echo "[]"
}

# Fetch edge releases with pagination (edge tags are mixed with old nightly tags)
fetch_edge_releases() {
	local needed="$1"
	local collected=""
	local page=1
	local max_pages=10
	local found=0

	while [ "$page" -le "$max_pages" ] && [ "$found" -lt "$needed" ]; do
		local json
		json=$(fetch_releases_page "$EDGE_REPO" 100 "$page")

		# Stop if empty page
		if [ "$json" = "[]" ] || [ -z "$json" ]; then
			break
		fi

		# Extract only edge releases from this page
		local tag date _name msg
		while IFS=$'\t' read -r tag date _name msg; do
			[ -z "$tag" ] && continue
			[[ "$tag" != edge-* ]] && continue
			collected+="${tag}"$'\t'"${date}"$'\t'"${msg}"$'\n'
			found=$((found + 1))
			[ "$found" -ge "$needed" ] && break
		done < <(parse_releases_json "$json")

		page=$((page + 1))
	done

	echo "$collected"
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
	echo "  Install: OXY_VERSION=<version> bash <(curl -sSfL https://get.oxy.tech)"
}

# ─── Render: Edge ────────────────────────────────────────────────────────────────

render_edge() {
	local edge_data="$1"

	print_section_header "Edge"
	echo ""

	if [ -z "$edge_data" ]; then
		print_no_releases
	else
		printf "  %-20s %-12s %-10s %s\n" "TAG" "DATE" "COMMIT" "MESSAGE"
		printf "  %-20s %-12s %-10s %s\n" "────────────────" "──────────" "───────" "───────────────────────────────"

		local tag date msg commit display_msg count=0
		echo "$edge_data" | sort -t$'\t' -k2 -r | while IFS=$'\t' read -r tag date msg; do
			[ -z "$tag" ] && continue
			commit=$(sha_from_tag "$tag")
			if [ "${#msg}" -gt 50 ]; then
				display_msg="${msg:0:47}..."
			else
				display_msg="$msg"
			fi
			printf "  %-20s %-12s %-10s %s\n" "$tag" "$date" "$commit" "$display_msg"
			count=$((count + 1))
			[ "$count" -ge "$LIMIT" ] && break
		done
	fi

	echo ""
	echo "  Install: OXY_VERSION=<tag> bash <(curl -sSfL https://nightly.oxy.tech)"
}

# ─── Main ────────────────────────────────────────────────────────────────────────

echo ""
echo "Oxy Releases"

if [ "$CHANNEL" = "all" ] || [ "$CHANNEL" = "stable" ]; then
	STABLE_JSON=$(fetch_releases_page "$STABLE_REPO" "$LIMIT")
	render_stable "$STABLE_JSON"
fi

if [ "$CHANNEL" = "all" ] || [ "$CHANNEL" = "edge" ]; then
	EDGE_DATA=$(fetch_edge_releases "$LIMIT")
	render_edge "$EDGE_DATA"
fi

echo ""

#!/usr/bin/env bash
# The triage agent's only way to touch the reports board: a curl wrapper over a fixed set of GitHub API calls on
# vdavid/cmdr-reports and its items on the "Cmdr backlog" project (vdavid, number 2). It exists because `gh` (Go)
# can't verify TLS inside the macOS sandbox, and it enforces in code what the prompt only asks for, since card text is
# untrusted and the prompt is the layer an injection can argue with.
#
# Usage:
#   ./board.sh list                       open cards labeled `unprocessed`, oldest first
#   ./board.sh view <n>                   one card: title, labels, body, and every comment
#   ./board.sh search <query>             issue search in this repo (open and closed), for duplicates
#   ./board.sh comment <n>                post a comment, body read from stdin
#   ./board.sh edit <n> [--title "..."] [--add-label X]... [--remove-label unprocessed]
#   ./board.sh rate <n> --severity <Critical|High|Medium|Low|none> --impact <High|Medium|Low> --size <XS|S|M|L|XL>
#
# Guards (each refusal exits non-zero with a message on stderr):
#   - `comment`, `edit`, and `rate` first read the card and refuse unless it's an open issue carrying `unprocessed`.
#   - `comment` prepends a hidden marker and refuses when a comment already starts with it: one triage comment per card.
#   - Labels come from fixed allowlists; `--remove-label` only takes `unprocessed`, and goes last.
#   - Titles are one line, 1–200 chars. Card numbers are positive integers. Search can't add its own scope qualifiers.
#   - `rate` sets only the project's Severity, Impact, and Size fields, only to the option names above, only on the
#     card's own item on project 2. Run it BEFORE `edit --remove-label unprocessed`, which closes the card to it.
#   - Host api.github.com, repo vdavid/cmdr-reports, project vdavid/2, and the HTTP calls below are hardcoded; nothing
#     else is reachable.
#
# Reads GH_TOKEN from the environment (run.sh hands it a PAT scoped to this repo), and for `rate` only,
# TRIAGE_PROJECT_TOKEN (a classic PAT with `project` scope alone; fine-grained PATs can't reach a user-owned project).
# Each reaches curl through a config on a pipe, so it never shows in argv, and the script never prints either. Written
# for macOS's bash 3.2.
set -euo pipefail

readonly repo="vdavid/cmdr-reports"
readonly api="https://api.github.com"
readonly marker="<!-- cmdr:triage -->"
readonly addable_labels=" bug enhancement duplicate "
readonly removable_labels=" unprocessed "
readonly max_title=200
readonly max_comment=20000
readonly max_query=200
readonly project_owner="vdavid"
readonly project_number=2
# The rating each field takes, by the option names on the board (docs/tooling/backlog-board.md defines them). `none`
# clears Severity, which stays blank on anything that isn't a bug or a security issue.
readonly severity_options=" Critical High Medium Low none "
readonly impact_options=" High Medium Low "
readonly size_options=" XS S M L XL "

die() {
  echo "board.sh: $*" >&2
  exit 2
}

usage() {
  sed -n '/^# Usage:/,/^#$/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

[[ -n "${GH_TOKEN:-}" ]] || die "GH_TOKEN isn't set"

check_number() {
  [[ "$1" =~ ^[1-9][0-9]{0,6}$ ]] || die "refusing: '$1' isn't a card number (a positive integer)"
}

# One GitHub API call. $1 is the method, $2 the path under the repo or a fixed top-level path; a request body, if any,
# comes on stdin. Only the methods this script uses get through.
gh_api() {
  local method="$1" path="$2"
  shift 2
  case "$method" in GET | POST | PATCH | DELETE) ;; *) die "internal: method $method not allowed" ;; esac
  curl --fail --silent --show-error --max-time 30 \
    --proto =https \
    --config <(printf 'header = "Authorization: Bearer %s"\n' "$GH_TOKEN") \
    --header "Accept: application/vnd.github+json" \
    --header "X-GitHub-Api-Version: 2022-11-28" \
    --request "$method" \
    "$@" \
    "$api$path"
}

# GET every page of a list endpoint and merge them into one JSON array. $1 is the path, already carrying a `?`.
gh_get_all() {
  local path="$1" page=1 chunk all="[]"
  while :; do
    chunk="$(gh_api GET "$path&per_page=100&page=$page")"
    all="$(jq -s '.[0] + .[1]' <(printf '%s' "$all") <(printf '%s' "$chunk"))"
    [[ "$(jq 'length' <<<"$chunk")" -lt 100 ]] && break
    page=$((page + 1))
    [[ $page -le 20 ]] || die "refusing: more than 2,000 items, something's off"
  done
  printf '%s' "$all"
}

# One call to the "Cmdr backlog" project with TRIAGE_PROJECT_TOKEN, which only `rate` uses. $1 is GET ($2 a REST path)
# or GRAPHQL (the request body on stdin). GraphQL answers most errors with a 200, so those are refused here too.
project_api() {
  local kind="$1" out
  [[ -n "${TRIAGE_PROJECT_TOKEN:-}" ]] || die "TRIAGE_PROJECT_TOKEN isn't set, so cards can't be rated"
  case "$kind" in
    GET) set -- --request GET "$api$2" ;;
    GRAPHQL) set -- --request POST --header "Content-Type: application/json" --data-binary @- "$api/graphql" ;;
    *) die "internal: project call $kind not allowed" ;;
  esac
  out="$(curl --fail --silent --show-error --max-time 30 \
    --proto =https \
    --config <(printf 'header = "Authorization: Bearer %s"\n' "$TRIAGE_PROJECT_TOKEN") \
    --header "Accept: application/vnd.github+json" \
    --header "X-GitHub-Api-Version: 2022-11-28" \
    "$@")"
  jq -e 'type != "object" or ((.errors // []) | length == 0)' >/dev/null <<<"$out" ||
    die "the board refused: $(jq -r '[.errors[].message] | join("; ")' <<<"$out")"
  printf '%s' "$out"
}

# The guard every write starts with: the card must be an open issue (not a PR) carrying `unprocessed`. Leaves the card's
# node id in `card_node_id`, which `rate` matches the card's project item against.
card_node_id=""
require_unprocessed() {
  local n="$1" card
  card="$(gh_api GET "/repos/$repo/issues/$n")"
  card_node_id="$(jq -r '.node_id' <<<"$card")"
  jq -e '.pull_request == null' >/dev/null <<<"$card" || die "refusing: #$n is a pull request, not a card"
  [[ "$(jq -r '.state' <<<"$card")" == "open" ]] || die "refusing: #$n is closed; only open cards can be changed"
  jq -e '[.labels[].name] | index("unprocessed") != null' >/dev/null <<<"$card" ||
    die "refusing: #$n doesn't carry 'unprocessed'; only unprocessed cards can be changed"
}

cmd_list() {
  [[ $# -eq 0 ]] || usage
  gh_get_all "/repos/$repo/issues?state=open&labels=unprocessed&sort=created&direction=asc" |
    jq -r '.[] | select(.pull_request == null)
      | "#\(.number)\t\(.created_at)\t[\([.labels[].name] | join(", "))]\t\(.title)"'
}

cmd_view() {
  [[ $# -eq 1 ]] || usage
  check_number "$1"
  local card comments
  card="$(gh_api GET "/repos/$repo/issues/$1")"
  comments="$(gh_get_all "/repos/$repo/issues/$1/comments?")"
  jq -r '"#\(.number) (\(.state)): \(.title)", "Labels: \([.labels[].name] | join(", "))",
    "Created: \(.created_at)", "", "--- body ---", (.body // "")' <<<"$card"
  jq -r '.[] | "", "--- comment by \(.user.login) at \(.created_at) ---", (.body // "")' <<<"$comments"
}

cmd_search() {
  [[ $# -ge 1 ]] || usage
  local query="$*"
  [[ ${#query} -le $max_query ]] || die "refusing: the query is longer than $max_query chars"
  # GitHub ORs extra `repo:` qualifiers into the scope, so a query could widen the search past this repo.
  shopt -s nocasematch
  [[ ! "$query" =~ (^|[^[:alnum:]_-])(repo|org|user|owner|is|type): ]] ||
    die "refusing: the query can't carry its own repo/org/user/owner/is/type qualifiers"
  shopt -u nocasematch
  local q
  q="$(jq -rn --arg q "repo:$repo is:issue $query" '$q | @uri')"
  gh_api GET "/search/issues?q=$q&per_page=50" |
    jq -r '.items[] | "#\(.number)\t\(.state)\t[\([.labels[].name] | join(", "))]\t\(.title)"'
}

cmd_comment() {
  [[ $# -eq 1 ]] || usage
  check_number "$1"
  local n="$1" body
  body="$(cat)"
  [[ -n "${body//[[:space:]]/}" ]] || die "refusing: the comment body (stdin) is empty"
  [[ ${#body} -le $max_comment ]] || die "refusing: the comment is longer than $max_comment chars"
  require_unprocessed "$n"
  gh_get_all "/repos/$repo/issues/$n/comments?" |
    jq -e --arg m "$marker" 'any(.[]; (.body // "") | startswith($m)) | not' >/dev/null ||
    die "refusing: #$n already has a triage comment; it gets one, so carry on with the title and labels"
  jq -n --arg b "$marker"$'\n'"$body" '{body: $b}' |
    gh_api POST "/repos/$repo/issues/$n/comments" --header "Content-Type: application/json" --data-binary @- |
    jq -r '"Commented on #'"$n"': \(.html_url)"'
}

cmd_edit() {
  [[ $# -ge 1 ]] || usage
  check_number "$1"
  local n="$1" title="" has_title=0 add="" remove="" label
  shift
  [[ $# -ge 1 ]] || die "refusing: nothing to edit; pass --title, --add-label, or --remove-label"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --title)
        [[ $# -ge 2 ]] || die "refusing: --title needs a value"
        [[ $has_title -eq 0 ]] || die "refusing: --title given twice"
        title="$2" has_title=1
        [[ -n "${title//[[:space:]]/}" ]] || die "refusing: the title is empty"
        [[ ${#title} -le $max_title ]] || die "refusing: the title is longer than $max_title chars"
        [[ "$title" != *$'\n'* && "$title" != *$'\r'* ]] || die "refusing: the title must be one line"
        shift 2
        ;;
      --add-label | --remove-label)
        [[ $# -ge 2 ]] || die "refusing: $1 needs a value"
        label="$2"
        # One label per flag: without this, "bug enhancement" would match the space-padded allowlist below.
        [[ "$label" =~ ^[A-Za-z:]+$ ]] || die "refusing: '$label' isn't a single label name"
        if [[ "$1" == --add-label ]]; then
          [[ "$addable_labels" == *" $label "* ]] ||
            die "refusing: can't add '$label'; allowed:$addable_labels"
          add="$add $label"
        else
          [[ "$removable_labels" == *" $label "* ]] ||
            die "refusing: can't remove '$label'; only:$removable_labels"
          remove="$remove $label"
        fi
        shift 2
        ;;
      *) die "refusing: unknown option '$1'" ;;
    esac
  done
  require_unprocessed "$n"
  if [[ $has_title -eq 1 ]]; then
    jq -n --arg t "$title" '{title: $t}' |
      gh_api PATCH "/repos/$repo/issues/$n" --header "Content-Type: application/json" --data-binary @- >/dev/null
    echo "Renamed #$n"
  fi
  if [[ -n "$add" ]]; then
    # shellcheck disable=SC2086 # $add is a space-separated list of allowlisted labels, none containing spaces.
    jq -n '{labels: $ARGS.positional}' --args $add |
      gh_api POST "/repos/$repo/issues/$n/labels" --header "Content-Type: application/json" --data-binary @- >/dev/null
    echo "Added labels to #$n:$add"
  fi
  # Removing `unprocessed` goes last: it's what closes the card to every later write.
  for label in $remove; do
    gh_api DELETE "/repos/$repo/issues/$n/labels/$label" >/dev/null
    echo "Removed '$label' from #$n"
  done
}

# Set the card's Severity, Impact, and Size on the project board. All three are required, so a card is never left
# half-rated; `--severity none` clears Severity.
# shellcheck disable=SC2016 # The single-quoted $names are GraphQL variables, not shell ones.
cmd_rate() {
  [[ $# -ge 1 ]] || usage
  check_number "$1"
  local n="$1" severity="" impact="" size="" value allowed
  shift
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --severity | --impact | --size)
        [[ $# -ge 2 ]] || die "refusing: $1 needs a value"
        value="$2"
        case "$1" in
          --severity) allowed="$severity_options" ;;
          --impact) allowed="$impact_options" ;;
          --size) allowed="$size_options" ;;
        esac
        # One word per flag: without this, "High Low" would match the space-padded allowlist.
        [[ "$value" =~ ^[A-Za-z]+$ && "$allowed" == *" $value "* ]] ||
          die "refusing: '$value' isn't a $1 value; allowed:$allowed"
        case "$1" in
          --severity)
            [[ -z "$severity" ]] || die "refusing: --severity given twice"
            severity="$value"
            ;;
          --impact)
            [[ -z "$impact" ]] || die "refusing: --impact given twice"
            impact="$value"
            ;;
          --size)
            [[ -z "$size" ]] || die "refusing: --size given twice"
            size="$value"
            ;;
        esac
        shift 2
        ;;
      *) die "refusing: unknown option '$1'" ;;
    esac
  done
  [[ -n "$severity" && -n "$impact" && -n "$size" ]] ||
    die "refusing: pass all three of --severity (none for a card that isn't a bug), --impact, and --size"
  require_unprocessed "$n"

  # The project-only token sees a private repo's issue as redacted content over GraphQL, but in full over REST, so the
  # item is found over REST, and matched on the card's node id rather than anything the agent passed.
  local items item_id
  items="$(project_api GET "/users/$project_owner/projectsV2/$project_number/items?per_page=100&q=$(
    jq -rn --arg q "repo:$repo label:unprocessed" '$q | @uri'
  )")"
  [[ "$(jq 'length' <<<"$items")" -lt 100 ]] ||
    die "refusing: 100 or more unprocessed cards on the board, something's off"
  item_id="$(jq -r --arg c "$card_node_id" --arg r "$api/repos/$repo" '[.[]
    | select(.content_type == "Issue" and .content.node_id == $c and .content.repository_url == $r)][0].node_id // ""' \
    <<<"$items")"
  [[ -n "$item_id" ]] ||
    die "refusing: #$n isn't on the board yet, so it can't be rated; say so in your final message and carry on"

  # The field and option ids, looked up by name, so a renamed or missing option refuses rather than guessing.
  local query board project_id ids
  query='query($o: String!, $n: Int!) { user(login: $o) { projectV2(number: $n) { id fields(first: 50) {'
  query+=' nodes { ... on ProjectV2SingleSelectField { id name options { id name } } } } } } }'
  board="$(jq -n --arg q "$query" --arg o "$project_owner" --argjson n "$project_number" \
    '{query: $q, variables: {o: $o, n: $n}}' | project_api GRAPHQL)"
  project_id="$(jq -r '.data.user.projectV2.id // ""' <<<"$board")"
  [[ -n "$project_id" ]] || die "refusing: can't find project $project_owner/$project_number"
  ids="$(jq -c --arg s "$severity" --arg i "$impact" --arg z "$size" '
    [.data.user.projectV2.fields.nodes[] | select(.name != null)] as $fields
    | def field($name): $fields | map(select(.name == $name)) | .[0];
      def option($name; $value): field($name).options // [] | map(select(.name == $value)) | .[0].id;
    {severityField: field("Severity").id, impactField: field("Impact").id, sizeField: field("Size").id,
     impactOption: option("Impact"; $i), sizeOption: option("Size"; $z)}
    + (if $s == "none" then {} else {severityOption: option("Severity"; $s)} end)' <<<"$board")"
  jq -e 'all(.[]; . != null)' >/dev/null <<<"$ids" ||
    die "refusing: the board lacks a Severity, Impact, or Size field, or one of the options passed; nothing was changed"

  # One request, so the three fields change together. Only the ids above go in, as variables.
  local set_field='updateProjectV2ItemFieldValue(input: {projectId: $p, itemId: $item, fieldId: $%sField, value:'
  set_field+=' {singleSelectOptionId: $%sOption}}) { clientMutationId }'
  local vars='$p: ID!, $item: ID!, $severityField: ID!, $impactField: ID!, $sizeField: ID!'
  vars+=', $impactOption: String!, $sizeOption: String!'
  local mutation
  if [[ "$severity" == none ]]; then
    mutation='severity: clearProjectV2ItemFieldValue(input: {projectId: $p, itemId: $item, fieldId: $severityField})'
    mutation+=' { clientMutationId }'
  else
    vars+=', $severityOption: String!'
    # shellcheck disable=SC2059 # The format is ours; %s only names a field.
    mutation="severity: $(printf "$set_field" severity severity)"
  fi
  # shellcheck disable=SC2059
  mutation+=" impact: $(printf "$set_field" impact impact) size: $(printf "$set_field" size size)"
  jq -n --arg q "mutation($vars) { $mutation }" --arg p "$project_id" --arg item "$item_id" --argjson ids "$ids" \
    '{query: $q, variables: ({p: $p, item: $item} + $ids)}' | project_api GRAPHQL >/dev/null
  echo "Rated #$n: Severity $severity, Impact $impact, Size $size"
}

[[ $# -ge 1 ]] || usage
sub="$1"
shift
case "$sub" in
  list) cmd_list "$@" ;;
  view) cmd_view "$@" ;;
  search) cmd_search "$@" ;;
  comment) cmd_comment "$@" ;;
  edit) cmd_edit "$@" ;;
  rate) cmd_rate "$@" ;;
  *) die "refusing: unknown subcommand '$sub'; use list, view, search, comment, edit, or rate" ;;
esac

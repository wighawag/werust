#!/usr/bin/env bash
#
# Measure `.goreleaser.yaml`'s changelog filters against REAL git history.
#
# The defect this exists for is that a filter can LOOK right and match nothing:
# the release changelog's exclude list was `^chore:` / `^ci:` / `^test:` /
# `^docs:` while this repo writes `chore(some-task):`, so over the 77 commits in
# `v0.3.2..main` those four patterns removed one single commit between them, and
# the published `v0.3.2` body carried an `### Others` section full of exactly
# what they were written to delete. Reading them was not enough; running them is.
#
# So this script does what GoReleaser does, on the same input:
#
#   * it READS the exclude patterns and group regexps out of `.goreleaser.yaml`
#     (never a second copy of them, which would drift), and
#   * applies them to `git log --format=%s <range>` the way
#     `internal/pipe/changelog` does: excludes first, then each surviving
#     subject to the FIRST group (in config order) whose regexp matches, with a
#     regexp-less group taking the rest.
#
# Matching uses `grep -P`, because the patterns use the lazy `??` quantifier that
# GoReleaser's RE2 has and POSIX ERE (`grep -E`) does not.
#
# Usage:
#   check-changelog-filters.sh [<git-range>] [--legacy]
#
#   <git-range>   default `v0.3.2..HEAD`
#   --legacy      classify with the SCOPE-BLIND filters this task replaced,
#                 so the before/after is reproducible rather than asserted.
#
# NOT a substitute for the real thing: this measures the FILTERS. Whether the
# changelog reaches the release PAGE is a property of the forge (who creates the
# Release, and `release.mode`) that only a real tag can show. See README.md.
set -euo pipefail

range="v0.3.2..HEAD"
legacy=0
for arg in "$@"; do
  case "${arg}" in
    --legacy) legacy=1 ;;
    -h | --help)
      sed -n '2,32p' "$0" | sed 's/^#\{1,2\} \{0,1\}//'
      exit 0
      ;;
    *) range="${arg}" ;;
  esac
done

repo_root="$(cd "$(dirname "$0")/../../.." && pwd)"
config="${repo_root}/.goreleaser.yaml"
test -f "${config}" || {
  echo "no ${config}" >&2
  exit 1
}

# --- read the rules out of the config ---------------------------------------

# `changelog.filters.exclude`: the `- <pattern>` items between `exclude:` and the
# next key at a shallower indent. Quotes stripped; comments skipped.
mapfile -t excludes < <(
  awk '
    /^    exclude:[[:space:]]*$/ { inblock = 1; next }
    inblock && /^      - / {
      line = substr($0, 9)
      sub(/[[:space:]]+$/, "", line)
      if (line ~ /^'\''.*'\''$/ || line ~ /^".*"$/) line = substr(line, 2, length(line) - 2)
      print line
      next
    }
    inblock && /^      #/ { next }
    inblock && /^[^ ]/ { inblock = 0 }
    inblock && /^  [^ ]/ { inblock = 0 }
  ' "${config}"
)

if [ "${legacy}" = 1 ]; then
  excludes=("^chore:" "^ci:" "^test:" "^docs:" "Merge ")
fi

# `changelog.groups`: title + optional regexp, in CONFIG order (which is the
# order GoReleaser assigns membership in; `order:` only sorts the rendering).
mapfile -t groups < <(
  awk '
    /^  groups:[[:space:]]*$/ { inblock = 1; next }
    inblock && /^    - title:/ {
      if (title != "") print title "\t" re
      title = $0; sub(/^    - title:[[:space:]]*/, "", title)
      gsub(/^["'\'']|["'\'']$/, "", title)
      re = ""
      next
    }
    inblock && /^      regexp:/ {
      re = $0; sub(/^      regexp:[[:space:]]*/, "", re)
      sub(/[[:space:]]+$/, "", re)
      if (re ~ /^'\''.*'\''$/ || re ~ /^".*"$/) re = substr(re, 2, length(re) - 2)
      next
    }
    inblock && /^[^ ]/ { inblock = 0 }
    END { if (title != "") print title "\t" re }
  ' "${config}"
)

# --- apply them --------------------------------------------------------------

cd "${repo_root}"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

git log --format='%s' "${range}" >"${work}/all"
total=$(wc -l <"${work}/all")

echo "range:    ${range}  (${total} commits)"
echo "filters:  $([ "${legacy}" = 1 ] && echo 'the SCOPE-BLIND set this task replaced' || echo "${config#"${repo_root}/"}")"
echo
echo "EXCLUDED (a pattern removed it; it never reaches a release page)"

cp "${work}/all" "${work}/remaining"
excluded_total=0
for pattern in "${excludes[@]}"; do
  n=$(grep -Pc -- "${pattern}" "${work}/remaining" || true)
  grep -Pv -- "${pattern}" "${work}/remaining" >"${work}/next" || true
  mv "${work}/next" "${work}/remaining"
  excluded_total=$((excluded_total + n))
  printf '  %4s  %s\n' "${n}" "${pattern}"
done
printf '  %4s  TOTAL EXCLUDED\n' "${excluded_total}"

echo
echo "PUBLISHED (what the release body would say)"
kept=$(wc -l <"${work}/remaining")
for group in "${groups[@]}"; do
  title="${group%%$'\t'*}"
  re="${group#*$'\t'}"
  if [ -n "${re}" ]; then
    grep -P -- "${re}" "${work}/remaining" >"${work}/group" || true
    grep -Pv -- "${re}" "${work}/remaining" >"${work}/next" || true
    mv "${work}/next" "${work}/remaining"
  else
    cp "${work}/remaining" "${work}/group"
    : >"${work}/remaining"
  fi
  n=$(wc -l <"${work}/group")
  printf '\n  ### %s (%s)\n' "${title}" "${n}"
  sed 's/^/    - /' "${work}/group" | cut -c1-160
done
printf '\n  %s of %s commits published, %s filtered out\n' "${kept}" "${total}" "${excluded_total}"

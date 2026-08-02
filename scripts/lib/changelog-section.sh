# scripts/lib/changelog-section.sh
#
# changelog_section <version>: prints CHANGELOG.md's "## [<version>] -
# YYYY-MM-DD" section (empty output if the section doesn't exist or has no
# entries under it). Reads CHANGELOG.md from the current working directory.
# Shared by check-release-invariants.sh and extract-changelog.sh so the two
# never drift on what counts as "the changelog section for a version" --
# exactly the kind of guardrail-logic duplication that's broken silently in
# this repo before (see the accent-hex guard's own exclusion-pattern bug).
changelog_section() {
  local version="$1"
  awk -v ver="$version" '
    $0 ~ ("^## \\[" ver "\\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$") { in_section=1; next }
    /^## / { in_section=0 }
    in_section { print }
  ' CHANGELOG.md
}

#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/eez-skill-installer-test.XXXXXX")"
trap 'rm -rf -- "$TEMP_ROOT"' EXIT

make_remote() {
  local name="$1"
  shift
  local source="$TEMP_ROOT/source-$name"
  local default_source="$TEMP_ROOT/default-$name"
  local remote="$TEMP_ROOT/$name.git"
  local skill_dir

  mkdir -p "$source"
  git -C "$source" init -q -b main
  git -C "$source" config user.name "Skill installer test"
  git -C "$source" config user.email "skill-installer@example.invalid"
  for skill_dir in "$@"; do
    mkdir -p "$source/$skill_dir"
    printf '# Fixture skill\n' > "$source/$skill_dir/SKILL.md"
  done
  git -C "$source" add .
  git -C "$source" commit -q -m 'fixture: pinned source'
  local revision
  revision="$(git -C "$source" rev-parse HEAD)"

  git clone -q "$source" "$default_source"
  git -C "$default_source" config user.name "Skill installer test"
  git -C "$default_source" config user.email "skill-installer@example.invalid"
  printf 'newer upstream default branch\n' > "$default_source/UPSTREAM_HEAD.txt"
  git -C "$default_source" add UPSTREAM_HEAD.txt
  git -C "$default_source" commit -q -m 'fixture: newer upstream head'
  git init --bare -q -b main "$remote"
  git -C "$default_source" push -q "$remote" main

  printf '%s\n' "$revision"
}

MATT_REVISION="$(make_remote mattpocock \
  skills/engineering/codebase-design \
  skills/engineering/diagnosing-bugs \
  skills/engineering/domain-modeling \
  skills/engineering/pr \
  skills/engineering/prototype \
  skills/engineering/research \
  skills/productivity/writing-for-agents)"
PSTACK_REVISION="$(make_remote cursor-plugins \
  pstack/skills/architect \
  pstack/skills/benchmark-checklist \
  pstack/skills/create-verification-skill \
  pstack/skills/interrogate \
  pstack/skills/no-comments \
  pstack/skills/principle-boundary-discipline \
  pstack/skills/principle-experience-first \
  pstack/skills/principle-make-operations-idempotent \
  pstack/skills/principle-model-the-domain \
  pstack/skills/principle-never-block-on-the-human \
  pstack/skills/principle-prove-it-works \
  pstack/skills/principle-sequence-verifiable-units \
  pstack/skills/principle-test-behavior-not-implementation \
  pstack/skills/principle-type-system-discipline \
  pstack/skills/reflect \
  pstack/skills/tdd)"
IMPECCABLE_REVISION="$(make_remote impeccable .agents/skills/impeccable)"

INSTALL_REPO="$TEMP_ROOT/owner's repo"
mkdir -p "$INSTALL_REPO/scripts" "$INSTALL_REPO/.agents/skills"
git -C "$INSTALL_REPO" init -q
cp "$ROOT/scripts/install-project-skills.sh" "$INSTALL_REPO/scripts/install-project-skills.sh"
python3 - \
  "$INSTALL_REPO/scripts/install-project-skills.sh" \
  "$TEMP_ROOT/mattpocock.git" "$MATT_REVISION" \
  "$TEMP_ROOT/cursor-plugins.git" "$PSTACK_REVISION" \
  "$TEMP_ROOT/impeccable.git" "$IMPECCABLE_REVISION" <<'PY'
from pathlib import Path
import sys

script, *values = sys.argv[1:]
path = Path(script)
content = path.read_text()
replacements = (
    ("https://github.com/mattpocock/skills.git", values[0]),
    ("49dd158d1076134a641b33efb035946536778336", values[1]),
    ("https://github.com/cursor/plugins.git", values[2]),
    ("1dcc2bacde73ddd284803b7af28164d0436198fc", values[3]),
    ("https://github.com/pbakaus/impeccable.git", values[4]),
    ("8a0e198da34ebc5b936a9d5975fb7a3b1cf353b7", values[5]),
)
for old, new in replacements:
    if old not in content:
        raise SystemExit(f"Missing installer fixture token: {old}")
    content = content.replace(old, new)
path.write_text(content)
PY

INSTALLER="$INSTALL_REPO/scripts/install-project-skills.sh"
PINNED="$INSTALL_REPO/.project-skills/pinned"
mkdir -p "$TEMP_ROOT/git-wrapper"
REAL_GIT="$(command -v git)"
cat > "$TEMP_ROOT/git-wrapper/git" <<'SH'
#!/usr/bin/env bash
for arg in "$@"; do
  if [ "$arg" = fetch ] && [ ! -e "$EEZ_FIXTURE_FAIL_ONCE" ]; then
    : > "$EEZ_FIXTURE_FAIL_ONCE"
    echo 'fixture: one-time fetch failure' >&2
    exit 31
  fi
done
exec "$EEZ_FIXTURE_REAL_GIT" "$@"
SH
chmod +x "$TEMP_ROOT/git-wrapper/git"
if EEZ_FIXTURE_FAIL_ONCE="$TEMP_ROOT/fail-once" \
  EEZ_FIXTURE_REAL_GIT="$REAL_GIT" \
  PATH="$TEMP_ROOT/git-wrapper:$PATH" \
  bash "$INSTALLER" > "$TEMP_ROOT/first-install.log" 2>&1; then
  echo 'Installer unexpectedly ignored the injected fetch failure.' >&2
  exit 1
fi
test -e "$TEMP_ROOT/fail-once"
test ! -e "$PINNED/mattpocock-${MATT_REVISION:0:12}"
test -z "$(find "$PINNED" -maxdepth 1 -name '.mattpocock-*' -print -quit)"

EEZ_FIXTURE_FAIL_ONCE="$TEMP_ROOT/fail-once" \
  EEZ_FIXTURE_REAL_GIT="$REAL_GIT" \
  PATH="$TEMP_ROOT/git-wrapper:$PATH" \
  bash "$INSTALLER" > "$TEMP_ROOT/retry.log" 2>&1
grep -F 'Verified 24 selected project skills; added 24 missing links.' "$TEMP_ROOT/retry.log" >/dev/null
bash "$INSTALLER" > "$TEMP_ROOT/idempotent.log" 2>&1
grep -F 'Verified 24 selected project skills; added 0 missing links.' "$TEMP_ROOT/idempotent.log" >/dev/null

wrong_target="../../.project-skills/pinned/mattpocock-${MATT_REVISION:0:12}/skills/engineering/codebase-design"
rm "$INSTALL_REPO/.agents/skills/domain-modeling"
ln -s "$wrong_target" "$INSTALL_REPO/.agents/skills/domain-modeling"
if bash "$INSTALLER" > "$TEMP_ROOT/wrong-link.log" 2>&1; then
  echo 'Installer accepted a link to a different skill at the same source revision.' >&2
  exit 1
fi
grep -F 'points to an unexpected skill directory' "$TEMP_ROOT/wrong-link.log" >/dev/null
test "$(readlink "$INSTALL_REPO/.agents/skills/domain-modeling")" = "$wrong_target"

printf 'local edit\n' >> "$PINNED/mattpocock-${MATT_REVISION:0:12}/skills/engineering/domain-modeling/SKILL.md"
if bash "$INSTALLER" > "$TEMP_ROOT/dirty-source.log" 2>&1; then
  echo 'Installer accepted a locally modified pinned source.' >&2
  exit 1
fi
grep -F 'Preserving local changes' "$TEMP_ROOT/dirty-source.log" >/dev/null
grep -F 'local edit' "$PINNED/mattpocock-${MATT_REVISION:0:12}/skills/engineering/domain-modeling/SKILL.md" >/dev/null

mkdir -p "$TEMP_ROOT/conflict/repo/scripts" "$TEMP_ROOT/conflict/repo/.agents/skills" \
  "$TEMP_ROOT/conflict/repo/.project-skills/pinned"
git -C "$TEMP_ROOT/conflict/repo" init -q
cp "$INSTALLER" "$TEMP_ROOT/conflict/repo/scripts/install-project-skills.sh"
git clone -q "$TEMP_ROOT/default-mattpocock" \
  "$TEMP_ROOT/conflict/repo/.project-skills/pinned/mattpocock-${MATT_REVISION:0:12}"
conflict_head="$(git -C "$TEMP_ROOT/conflict/repo/.project-skills/pinned/mattpocock-${MATT_REVISION:0:12}" rev-parse HEAD)"
if bash "$TEMP_ROOT/conflict/repo/scripts/install-project-skills.sh" > "$TEMP_ROOT/conflict.log" 2>&1; then
  echo 'Installer changed an existing pinned checkout at another revision.' >&2
  exit 1
fi
grep -F 'unexpected revision' "$TEMP_ROOT/conflict.log" >/dev/null
test "$(git -C "$TEMP_ROOT/conflict/repo/.project-skills/pinned/mattpocock-${MATT_REVISION:0:12}" rev-parse HEAD)" = "$conflict_head"

mkdir -p "$TEMP_ROOT/broken/repo/scripts" "$TEMP_ROOT/broken/repo/.agents/skills" \
  "$TEMP_ROOT/broken/repo/.project-skills/pinned"
git -C "$TEMP_ROOT/broken/repo" init -q
cp "$INSTALLER" "$TEMP_ROOT/broken/repo/scripts/install-project-skills.sh"
broken_link="$TEMP_ROOT/broken/repo/.project-skills/pinned/mattpocock-${MATT_REVISION:0:12}"
ln -s "$TEMP_ROOT/missing-source" "$broken_link"
if bash "$TEMP_ROOT/broken/repo/scripts/install-project-skills.sh" > "$TEMP_ROOT/broken.log" 2>&1; then
  echo 'Installer replaced an existing broken source symlink.' >&2
  exit 1
fi
test -L "$broken_link"
test "$(readlink "$broken_link")" = "$TEMP_ROOT/missing-source"

printf 'Project skill installer regression fixtures passed.\n'

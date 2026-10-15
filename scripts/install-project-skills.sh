#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SKILL_ROOT="$ROOT/.project-skills"

MATT_REVISION=49dd158d1076134a641b33efb035946536778336
PSTACK_REVISION=1dcc2bacde73ddd284803b7af28164d0436198fc
IMPECCABLE_REVISION=8a0e198da34ebc5b936a9d5975fb7a3b1cf353b7

ensure_pinned_source() {
  local name="$1" url="$2" revision="$3" preferred="$4" sentinel="$5"
  local pinned_path="$SKILL_ROOT/pinned/$name-${revision:0:12}"

  if [ -d "$preferred/.git" ] \
    && [ "$(git -C "$preferred" rev-parse HEAD)" = "$revision" ] \
    && [ -z "$(git -C "$preferred" status --porcelain)" ] \
    && [ -f "$preferred/$sentinel" ]; then
    printf '%s\n' "$preferred"
    return
  fi

  if [ -e "$pinned_path" ] || [ -L "$pinned_path" ]; then
    if [ ! -d "$pinned_path/.git" ]; then
      echo "Refusing to overwrite non-git skill source: $pinned_path" >&2
      return 1
    fi
    if [ -n "$(git -C "$pinned_path" status --porcelain)" ]; then
      echo "Preserving local changes in $pinned_path" >&2
      return 1
    fi
    if [ "$(git -C "$pinned_path" rev-parse HEAD)" != "$revision" ]; then
      echo "Preserving existing skill source at an unexpected revision: $pinned_path" >&2
      return 1
    fi
    if [ ! -f "$pinned_path/$sentinel" ]; then
      echo "Preserving existing skill source with missing sentinel: $pinned_path/$sentinel" >&2
      return 1
    fi
    printf '%s\n' "$pinned_path"
    return
  fi

  mkdir -p "$(dirname -- "$pinned_path")"
  local temporary_path
  local cleanup_command
  temporary_path="$(mktemp -d "$(dirname -- "$pinned_path")/.$name-${revision:0:12}.XXXXXX")"
  # A fresh --no-checkout clone reports tracked files as deleted until checkout.
  # Keep it temporary so a failed fetch cannot make the next install look user-edited.
  printf -v cleanup_command 'rm -rf -- %q' "$temporary_path"
  trap "$cleanup_command" EXIT
  git clone --depth 1 --no-checkout "$url" "$temporary_path" >&2 || return 1
  git -C "$temporary_path" fetch --depth 1 origin "$revision" >&2 || return 1
  git -C "$temporary_path" checkout --detach "$revision" >&2 || return 1
  if [ ! -f "$temporary_path/$sentinel" ]; then
    echo "Pinned source is missing its expected sentinel: $temporary_path/$sentinel" >&2
    return 1
  fi
  if [ -n "$(git -C "$temporary_path" status --porcelain)" ]; then
    echo "Pinned source is unexpectedly dirty: $temporary_path" >&2
    return 1
  fi
  if [ "$(git -C "$temporary_path" rev-parse HEAD)" != "$revision" ]; then
    echo "Pinned source verification failed for $name at $temporary_path" >&2
    return 1
  fi
  if [ -e "$pinned_path" ] || [ -L "$pinned_path" ]; then
    echo "Preserving a skill source created during install: $pinned_path" >&2
    return 1
  fi
  mv -- "$temporary_path" "$pinned_path"
  printf '%s\n' "$pinned_path"
}

MATT_SOURCE="$(ensure_pinned_source mattpocock https://github.com/mattpocock/skills.git "$MATT_REVISION" "$SKILL_ROOT/mattpocock" skills/engineering/domain-modeling/SKILL.md)"
PSTACK_SOURCE="$(ensure_pinned_source cursor-plugins https://github.com/cursor/plugins.git "$PSTACK_REVISION" "$SKILL_ROOT/cursor-plugins" pstack/skills/tdd/SKILL.md)"
IMPECCABLE_SOURCE="$(ensure_pinned_source impeccable https://github.com/pbakaus/impeccable.git "$IMPECCABLE_REVISION" "$SKILL_ROOT/impeccable" .agents/skills/impeccable/SKILL.md)"

MATT_SOURCE="$MATT_SOURCE" \
PSTACK_SOURCE="$PSTACK_SOURCE" \
IMPECCABLE_SOURCE="$IMPECCABLE_SOURCE" \
MATT_REVISION="$MATT_REVISION" \
PSTACK_REVISION="$PSTACK_REVISION" \
IMPECCABLE_REVISION="$IMPECCABLE_REVISION" \
EEZFUNNELS_ROOT="$ROOT" python3 - <<'PY'
from pathlib import Path
import os
import subprocess

root = Path(os.environ["EEZFUNNELS_ROOT"])
sources = {
    "mattpocock": (Path(os.environ["MATT_SOURCE"]), os.environ["MATT_REVISION"]),
    "pstack": (Path(os.environ["PSTACK_SOURCE"]), os.environ["PSTACK_REVISION"]),
    "impeccable": (Path(os.environ["IMPECCABLE_SOURCE"]), os.environ["IMPECCABLE_REVISION"]),
}

engineering = sources["mattpocock"][0] / "skills/engineering"
selected = [
    ("pstack", "architect", sources["pstack"][0] / "pstack/skills/architect"),
    ("pstack", "benchmark-checklist", sources["pstack"][0] / "pstack/skills/benchmark-checklist"),
    ("pstack", "create-verification-skill", sources["pstack"][0] / "pstack/skills/create-verification-skill"),
    ("pstack", "interrogate", sources["pstack"][0] / "pstack/skills/interrogate"),
    ("pstack", "no-comments", sources["pstack"][0] / "pstack/skills/no-comments"),
    ("pstack", "principle-boundary-discipline", sources["pstack"][0] / "pstack/skills/principle-boundary-discipline"),
    ("pstack", "principle-experience-first", sources["pstack"][0] / "pstack/skills/principle-experience-first"),
    ("pstack", "principle-make-operations-idempotent", sources["pstack"][0] / "pstack/skills/principle-make-operations-idempotent"),
    ("pstack", "principle-model-the-domain", sources["pstack"][0] / "pstack/skills/principle-model-the-domain"),
    ("pstack", "principle-never-block-on-the-human", sources["pstack"][0] / "pstack/skills/principle-never-block-on-the-human"),
    ("pstack", "principle-prove-it-works", sources["pstack"][0] / "pstack/skills/principle-prove-it-works"),
    ("pstack", "principle-sequence-verifiable-units", sources["pstack"][0] / "pstack/skills/principle-sequence-verifiable-units"),
    ("pstack", "principle-test-behavior-not-implementation", sources["pstack"][0] / "pstack/skills/principle-test-behavior-not-implementation"),
    ("pstack", "principle-type-system-discipline", sources["pstack"][0] / "pstack/skills/principle-type-system-discipline"),
    ("pstack", "reflect", sources["pstack"][0] / "pstack/skills/reflect"),
    ("pstack", "tdd", sources["pstack"][0] / "pstack/skills/tdd"),
    ("mattpocock", "codebase-design", engineering / "codebase-design"),
    ("mattpocock", "diagnosing-bugs", engineering / "diagnosing-bugs"),
    ("mattpocock", "domain-modeling", engineering / "domain-modeling"),
    ("mattpocock", "pr", engineering / "pr"),
    ("mattpocock", "prototype", engineering / "prototype"),
    ("mattpocock", "research", engineering / "research"),
    ("mattpocock", "writing-for-agents", sources["mattpocock"][0] / "skills/productivity/writing-for-agents"),
    ("impeccable", "impeccable", sources["impeccable"][0] / ".agents/skills/impeccable"),
]

for source_name, (source_root, revision) in sources.items():
    actual = subprocess.check_output(["git", "-C", str(source_root), "rev-parse", "HEAD"], text=True).strip()
    if actual != revision:
        raise SystemExit(f"Pinned {source_name} source mismatch: expected {revision}, found {actual}")

destination = root / ".agents/skills"
destination.mkdir(parents=True, exist_ok=True)
exclude_value = subprocess.check_output(
    ["git", "rev-parse", "--git-path", "info/exclude"], cwd=root, text=True
).strip()
exclude = Path(exclude_value)
if not exclude.is_absolute():
    exclude = root / exclude
existing = exclude.read_text() if exclude.exists() else ""
lines = existing.splitlines()
for pattern in ("/.project-skills/", "/.agents/skills/"):
    if pattern not in lines:
        existing += ("\n" if existing and not existing.endswith("\n") else "") + pattern + "\n"
exclude.parent.mkdir(parents=True, exist_ok=True)
exclude.write_text(existing)

added = 0
for source_name, name, skill in selected:
    if not (skill / "SKILL.md").is_file():
        raise SystemExit(f"Missing skill source: {skill / 'SKILL.md'}")
    link = destination / name
    if link.is_symlink():
        target = link.resolve(strict=True)
        if not (target / "SKILL.md").is_file():
            raise SystemExit(f"Existing skill link is broken: {link}")
        expected_target = skill.resolve(strict=True)
        if target != expected_target:
            raise SystemExit(
                f"Existing skill link points to an unexpected skill directory: "
                f"{link} -> {target} (expected {expected_target})"
            )
        actual = subprocess.check_output(["git", "-C", str(target), "rev-parse", "HEAD"], text=True).strip()
        expected = sources[source_name][1]
        if actual != expected:
            raise SystemExit(f"Existing skill link has unexpected source revision: {link} ({actual})")
        dirty = subprocess.check_output(["git", "-C", str(target), "status", "--porcelain"], text=True)
        if dirty:
            raise SystemExit(f"Existing skill link has local source changes: {link}")
        continue
    if link.exists():
        raise SystemExit(f"Preserving existing non-symlink skill path: {link}")
    link.symlink_to(os.path.relpath(skill, destination), target_is_directory=True)
    added += 1

print(f"Verified {len(selected)} selected project skills; added {added} missing links.")
print("Existing skill links and .project-skills/README.md were preserved.")
print("No global configuration, hooks, product dependencies, or runtime files were changed.")
PY

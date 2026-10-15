#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SKILL_ROOT="$ROOT/.project-skills"

MATT_REVISION=49dd158d1076134a641b33efb035946536778336
PSTACK_REVISION=1dcc2bacde73ddd284803b7af28164d0436198fc
IMPECCABLE_REVISION=8a0e198da34ebc5b936a9d5975fb7a3b1cf353b7

ensure_pinned_source() {
  local name="$1" url="$2" revision="$3" preferred="$4" sentinel="$5"
  local path="$preferred"

  # Reuse the existing checkout only when it already matches. Never move a
  # user's existing skill-stack checkout to another revision.
  if [ -d "$preferred/.git" ] \
    && [ "$(git -C "$preferred" rev-parse HEAD)" = "$revision" ] \
    && [ -z "$(git -C "$preferred" status --porcelain)" ] \
    && [ -f "$preferred/$sentinel" ]; then
    printf '%s\n' "$preferred"
    return
  fi

  path="$SKILL_ROOT/pinned/$name-${revision:0:12}"
  if [ ! -e "$path" ]; then
    mkdir -p "$(dirname -- "$path")"
    git clone --depth 1 --no-checkout "$url" "$path" >&2
  fi
  if [ ! -d "$path/.git" ]; then
    echo "Refusing to overwrite non-git skill source: $path" >&2
    return 1
  fi
  if [ "$(git -C "$path" rev-parse HEAD 2>/dev/null || true)" != "$revision" ]; then
    if [ -n "$(git -C "$path" status --porcelain)" ]; then
      echo "Preserving local changes in $path" >&2
      return 1
    fi
    git -C "$path" fetch --depth 1 origin "$revision" >&2
    git -C "$path" checkout --detach "$revision" >&2
  fi
  if [ ! -f "$path/$sentinel" ]; then
    git -C "$path" checkout --detach "$revision" >&2
  fi
  if [ -n "$(git -C "$path" status --porcelain)" ]; then
    echo "Preserving local changes in $path" >&2
    return 1
  fi
  if [ "$(git -C "$path" rev-parse HEAD)" != "$revision" ]; then
    echo "Pinned source verification failed for $name at $path" >&2
    return 1
  fi
  printf '%s\n' "$path"
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
        actual = subprocess.check_output(["git", "-C", str(target), "rev-parse", "HEAD"], text=True).strip()
        expected = sources[source_name][1]
        if actual != expected:
            raise SystemExit(f"Existing skill link has unexpected source revision: {link} ({actual})")
        continue
    if link.exists():
        raise SystemExit(f"Preserving existing non-symlink skill path: {link}")
    link.symlink_to(os.path.relpath(skill, destination), target_is_directory=True)
    added += 1

print(f"Verified {len(selected)} selected project skills; added {added} missing links.")
print("Existing skill links and .project-skills/README.md were preserved.")
print("No global configuration, hooks, product dependencies, or runtime files were changed.")
PY

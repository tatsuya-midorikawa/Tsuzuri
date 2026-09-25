#!/usr/bin/env sh
# Fails when a runtime file embedded with include_str! is missing, untracked, or ignored.
set -eu

files=$(
  grep -hE 'include_str!\("runtime/[^"]+"\)' src/llvm.rs src/driver.rs |
    sed -E 's/.*include_str!\("runtime\/([^"]+)"\).*/src\/runtime\/\1/' |
    sort -u
)

for file in $files; do
  if ! test -f "$file"; then
    echo "runtime include is missing: $file" >&2
    exit 1
  fi
  if ! git ls-files --error-unmatch "$file" >/dev/null 2>&1; then
    echo "runtime include is not tracked by Git: $file" >&2
    exit 1
  fi
  if git check-ignore --no-index -q "$file"; then
    echo "tracked runtime include is ignored by .gitignore: $file" >&2
    exit 1
  fi
done
echo "runtime includes are tracked: $(echo "$files" | wc -l | tr -d ' ') files"

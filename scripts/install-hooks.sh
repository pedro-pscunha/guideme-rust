#!/bin/sh
# Installs lefthook stubs into this repository's own hooks directory.
#
# `lefthook install` refuses to run when a global `core.hooksPath` is set. Pedro's global
# hooks dir chains `pre-commit` into the repo's own hooks, so writing the stubs here keeps
# both the global guard and this repo's checks. Run via `mise run hooks`.
set -eu

hooks_dir="$(git rev-parse --git-common-dir)/hooks"
mkdir -p "$hooks_dir"

for hook in pre-commit pre-push; do
  cat > "$hooks_dir/$hook" <<STUB
#!/bin/sh
# lefthook stub, written by scripts/install-hooks.sh
exec lefthook run $hook "\$@"
STUB
  chmod +x "$hooks_dir/$hook"
  echo "installed $hooks_dir/$hook"
done

hooks_path="$(git config --get core.hooksPath || true)"
if [ -n "$hooks_path" ]; then
  dead=0
  for hook in pre-commit pre-push; do
    if ! grep -qs "hooks/$hook" "$hooks_path/$hook"; then
      echo "error: core.hooksPath=$hooks_path has no $hook that chains to the repo hook; the $hook gate is DEAD" >&2
      dead=1
    fi
  done
  if [ "$dead" -eq 1 ]; then
    echo "add a chaining hook there, or run 'mise run check' by hand before pushing" >&2
    exit 1
  fi
fi

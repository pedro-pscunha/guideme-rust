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

if git config --get core.hooksPath >/dev/null 2>&1; then
  echo "note: core.hooksPath is set globally; pre-commit chains here, pre-push only if that dir chains it too"
fi

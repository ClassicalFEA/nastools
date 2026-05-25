#!/usr/bin/env bash
# Build the f06csv web UI locally and rsync it to a remote web server.
#
# Usage:
#   ./deploy.sh                                  # uses defaults below
#   PUBLIC_URL=/tools/f06csv/ ./deploy.sh        # subdir deploy
#   DEST=user@host:/var/www/html/f06csv/ ./deploy.sh
#
# Requires (locally): rustup, the wasm32-unknown-unknown target, trunk, rsync.
# Optional but recommended: wasm-opt (from the `binaryen` package) — trunk
# will pick it up automatically and shave a lot off the .wasm file.

set -euo pipefail

# ---- Config ----------------------------------------------------------------
# Where on the remote to drop the files. Must end in a slash.
DEST="${DEST:-user@example.com:/var/www/html/f06csv/}"
# Path the app will be served under, as seen by the browser. MUST end in '/'.
# Use "/" for the site root, "/f06csv/" for a subdirectory, etc.
PUBLIC_URL="${PUBLIC_URL:-/f06csv/}"
# Extra flags passed to rsync (e.g. ssh options).
RSYNC_EXTRA="${RSYNC_EXTRA:-}"
# ---------------------------------------------------------------------------

cd "$(dirname "$0")"

echo "==> Ensuring wasm32 target is installed"
rustup target add wasm32-unknown-unknown >/dev/null

echo "==> Cleaning dist/"
rm -rf dist

echo "==> Building (release) with public-url=${PUBLIC_URL}"
trunk build --release --public-url "${PUBLIC_URL}"

echo "==> Contents of dist/:"
ls -lh dist

echo "==> Rsyncing to ${DEST}"
# -a   archive (perms, times, recurse)
# -v   verbose
# -z   compress over the wire
# -h   human-readable sizes
# --delete  remove files on the server that no longer exist locally
# Note the trailing slash on dist/ — copies *contents*, not the dir itself.
rsync -avzh --delete ${RSYNC_EXTRA} dist/ "${DEST}"

echo "==> Done."

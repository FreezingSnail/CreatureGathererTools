#!/usr/bin/env bash
# Builds current checkout sources before replacing this install's app and CLI.
set -euo pipefail

if [ "$#" -ne 0 ]; then
  printf 'Usage: %s\n' "$0" >&2
  printf 'Optional destination override: CGFX_INSTALL_PREFIX=/path %s\n' "$0" >&2
  exit 64
fi

for command in cargo npm ditto mktemp; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'error: %s is required\n' "$command" >&2
    exit 69
  fi
done

repo_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
prefix="${CGFX_INSTALL_PREFIX:-$HOME/Applications/CreatureGathererTools}"
mkdir -p "$prefix"
prefix="$(CDPATH= cd -- "$prefix" && pwd -P)"
app_name='CreatureGathererFX.app'
app_destination="$prefix/$app_name"
cli_destination="$prefix/bin/cgfx-tools"
app_source="$repo_dir/target/release/bundle/macos/$app_name"
cli_source="$repo_dir/target/release/cgfx-tools"
staging_dir="$(mktemp -d "$prefix/.cgfx-install.XXXXXX")"

cleanup() {
  [ -z "${staging_dir:-}" ] || rm -rf "$staging_dir"
}
trap cleanup EXIT HUP INT TERM

npm --prefix "$repo_dir/ui" ci
(
  cd "$repo_dir"
  cargo build --locked --release -p cgfx-core --bin cgfx-tools
  "$repo_dir/ui/node_modules/.bin/tauri" build --bundles app
)

if [ ! -d "$app_source" ] || [ ! -f "$cli_source" ]; then
  printf 'error: source build did not produce the app and CLI artifacts\n' >&2
  exit 70
fi

mkdir -p "$staging_dir/bin"
ditto "$app_source" "$staging_dir/$app_name"
ditto "$cli_source" "$staging_dir/bin/cgfx-tools"
chmod +x "$staging_dir/bin/cgfx-tools"

# Replace only managed install targets after both new artifacts are staged.
rm -rf "$app_destination" "$cli_destination"
mkdir -p "$prefix/bin"
mv -f "$staging_dir/$app_name" "$app_destination"
mv -f "$staging_dir/bin/cgfx-tools" "$cli_destination"
staging_dir=''
trap - EXIT HUP INT TERM

printf 'Reinstalled app: %s\n' "$app_destination"
printf 'Reinstalled CLI: %s\n' "$cli_destination"
printf 'Add %s/bin to PATH if needed.\n' "$prefix"

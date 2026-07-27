#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/build-interpreter-release.sh [--target <rust-target>] [--install-dir <dir>] [--home <dir>]

Builds a local Open Interpreter standalone package using the same package
layout as the public installer, stages it under INTERPRETER_HOME, and
installs interpreter/i shims into the visible bin directory.
EOF
}

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
codex_rs_dir="$repo_root/codex-rs"
target=""
install_dir="${OPEN_INTERPRETER_INSTALL_DIR:-${CODEX_INSTALL_DIR:-$HOME/.local/bin}}"
interpreter_home="${INTERPRETER_HOME:-$HOME/.openinterpreter}"
build_jobs="${CARGO_BUILD_JOBS:-1}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      target="${2:?--target requires a value}"
      shift 2
      ;;
    --install-dir)
      install_dir="${2:?--install-dir requires a value}"
      shift 2
      ;;
    --home)
      interpreter_home="${2:?--home requires a value}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unexpected argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

package_root="$interpreter_home/packages/standalone"
releases_dir="$package_root/releases"
current_link="$package_root/current"
target_args=()
target_label="host"
if [[ -n "$target" ]]; then
  target_args=(--target "$target")
  target_label="$target"
fi

package_dir="$releases_dir/local-$target_label"
staging_dir="$releases_dir/.staging.local-$target_label.$$"
cleanup_staging=true

cleanup() {
  if [[ "$cleanup_staging" == "true" ]]; then
    rm -rf "$staging_dir"
  fi
}
trap cleanup EXIT INT TERM

replace_symlink() {
  local target_path="$1"
  local link_path="$2"
  local tmp_link="${link_path}.$$"

  if [[ -d "$link_path" && ! -L "$link_path" ]]; then
    echo "Refusing to replace non-symlink directory: $link_path" >&2
    exit 1
  fi

  rm -f "$tmp_link"
  ln -s "$target_path" "$tmp_link"
  rm -f "$link_path"
  mv -f "$tmp_link" "$link_path"
}

echo "Building local Open Interpreter standalone package..."
echo "Workspace: $codex_rs_dir"
echo "Open Interpreter home: $interpreter_home"
echo "Visible bin directory: $install_dir"
echo "Cargo build jobs: $build_jobs"

mkdir -p "$releases_dir" "$install_dir"
rm -rf "$staging_dir"

(
  cd "$repo_root"
  package_args=(
    --variant open-interpreter
    --cargo-profile release
    --package-dir "$staging_dir"
    --force
  )
  if [[ -n "$target" ]]; then
    package_args=(--target "$target" "${package_args[@]}")
  fi
  CARGO_BUILD_JOBS="$build_jobs" python3 scripts/build_codex_package.py "${package_args[@]}"
)

if [[ -e "$package_dir" || -L "$package_dir" ]]; then
  rm -rf "$package_dir"
fi
mv "$staging_dir" "$package_dir"
cleanup_staging=false

replace_symlink "$package_dir" "$current_link"
replace_symlink "$current_link/bin/interpreter" "$install_dir/interpreter"
replace_symlink "$current_link/bin/interpreter" "$install_dir/i"

echo
echo "Built and staged:"
echo "  $package_dir/bin/interpreter"
echo
echo "Installed shims:"
echo "  $install_dir/interpreter -> $current_link/bin/interpreter"
echo "  $install_dir/i -> $current_link/bin/interpreter"
echo
"$install_dir/interpreter" --version

#!/usr/bin/env bash
# Build the complete, pinned engine for the current Unix OS and architecture.
set -euo pipefail
project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
engine_source=${HASHCAT_SOURCE:-"$project_root/vendor/hashcat-src-8.0.0"}
engine_commit=6a7e3b5a96abcd6373c2e4112bc358abb24dc538
case "$(uname -s)" in Darwin|Linux) ;; *) echo 'Build Hashcat on macOS or Linux.' >&2; exit 2 ;; esac
command -v rustc >/dev/null || { echo 'Rust is required to retain the Rust bridge.' >&2; exit 2; }
if [[ ! -e "$engine_source" ]]; then
  mkdir -p -- "$(dirname -- "$engine_source")"
  git clone --no-checkout --depth 1 --branch v8.0.0 https://github.com/hashcat/hashcat.git "$engine_source"
  git -C "$engine_source" checkout --detach "$engine_commit"
  git -C "$engine_source" submodule update --init --recursive
fi
actual_commit=$(git -C "$engine_source" rev-parse HEAD)
[[ "$actual_commit" == "$engine_commit" ]] || { echo 'Existing Hashcat source is not the pinned 8.0.0 revision; use another HASHCAT_SOURCE directory.' >&2; exit 2; }
if [[ "$(uname -s)" == Darwin ]]; then
  python3 "$project_root/scripts/patch-hashcat-macos.py" "$engine_source"
fi
jobs=${HASHHELM_BUILD_JOBS:-2}
[[ "$jobs" =~ ^[1-9][0-9]*$ ]] || { echo 'HASHHELM_BUILD_JOBS must be positive.' >&2; exit 2; }
# Maintainer mode avoids host-specific AVX/native instructions in distributed builds.
make -C "$engine_source" -j"$jobs" MAINTAINER_MODE=1
"$engine_source/hashcat" --version
printf 'Engine built at: %s\n' "$engine_source"

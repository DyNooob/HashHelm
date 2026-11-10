#!/usr/bin/env bash
set -euo pipefail
project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$project_root"
case "$(uname -s)" in Darwin) export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-15.0} ;; Linux) ;; *) echo 'Use package.ps1 on Windows.' >&2; exit 2 ;; esac
if [[ "${1:-}" == --with-hashcat ]]; then bash scripts/build-hashcat.sh; shift; fi
engine_source=${HASHCAT_SOURCE:-"$project_root/vendor/hashcat-src-8.0.0"}
cargo test --no-default-features --lib --locked
cargo build --release --locked --bin hashhelm --bin runtime-check
python3 scripts/rust-notices.py
target/release/runtime-check "$engine_source/hashcat"
python3 scripts/package_unix.py --engine-dir "$engine_source" "$@"

#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="${repo_root}/output/trixie-release"

rm -rf "${output_dir}"
mkdir -p "${output_dir}"

podman run --rm -it --pull=newer \
  -v "${repo_root}:/src:ro,Z" \
  -v "${output_dir}:/output:Z" \
  docker.io/library/debian:trixie \
  bash -lc '
    set -euo pipefail

    export DEBIAN_FRONTEND=noninteractive

    apt-get update
    apt-get install -y --no-install-recommends \
      ca-certificates \
      curl \
      file \
      unzip \
      pkg-config \
      build-essential \
      libssl-dev \
      libgtk-3-dev \
      libayatana-appindicator3-dev \
      librsvg2-dev \
      patchelf \
      libwebkit2gtk-4.1-dev

    curl https://sh.rustup.rs -sSf | sh -s -- -y
    export PATH="${HOME}/.cargo/bin:${PATH}"

    curl -fsSL https://bun.sh/install | bash
    export BUN_INSTALL="${HOME}/.bun"
    export PATH="${BUN_INSTALL}/bin:${PATH}"

    mkdir -p /work
    cp -a /src/. /work
    rm -rf /work/node_modules /work/dist /work/src-tauri/target

    cd /work
    bun install --frozen-lockfile
    bun run tauri build --no-bundle

    cp -f /work/src-tauri/target/release/duped /output/duped
  '

echo "Copied release binary to ${output_dir}/duped"

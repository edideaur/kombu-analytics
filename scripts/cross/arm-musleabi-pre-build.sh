#!/usr/bin/env bash
set -euo pipefail
for tool in /usr/local/bin/arm-linux-musleabi-*; do
    dest="/usr/local/bin/$(basename "$tool" | sed 's/arm-linux-musleabi-/arm-unknown-linux-musleabi-/')"
    ln -sf "$tool" "$dest"
done

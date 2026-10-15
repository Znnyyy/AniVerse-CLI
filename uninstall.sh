#!/bin/bash

echo ""
echo "  ====================================================="
echo "    aniverse-cli Auto Uninstaller for Linux / macOS"
echo "  ====================================================="
echo ""

if ! command -v cargo &> /dev/null; then
    echo "[ERROR] Rust/cargo not found. Cannot uninstall."
    exit 1
fi

echo "[1/1] Uninstalling aniverse-cli via cargo..."

# Legacy package name from earlier versions (ignore failure silently)
cargo uninstall animeku-cli &> /dev/null || true

if cargo uninstall aniverse-cli; then
    echo ""
    echo "  ====================================================="
    echo "    SUCCESS! aniverse-cli uninstalled!"
    echo "  ====================================================="
else
    echo ""
    echo "  [WARN] aniverse-cli is not installed or already removed."
    exit 0
fi

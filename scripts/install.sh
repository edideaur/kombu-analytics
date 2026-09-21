#!/bin/sh
set -eu

REPO="edideaur/kombu-analytics"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/kombu"
ENV_FILE="${CONFIG_DIR}/kombu.env"

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$ARCH" in
    x86_64|amd64)
        TARGET_ARCH="x86_64"
        ;;
    aarch64|arm64)
        TARGET_ARCH="aarch64"
        ;;
    armv7l|armv7)
        TARGET_ARCH="armv7"
        ;;
    armv6l)
        TARGET_ARCH="arm"
        ;;
    riscv64)
        TARGET_ARCH="riscv64gc"
        ;;
    i686|i386)
        TARGET_ARCH="i686"
        ;;
    ppc64le)
        TARGET_ARCH="powerpc64le"
        ;;
    s390x)
        TARGET_ARCH="s390x"
        ;;
    *)
        echo "Unsupported architecture: $ARCH" >&2
        exit 1
        ;;
esac

case "$OS" in
    linux)
        if ldd /bin/ls 2>&1 | grep -qi musl || [ -f /lib/ld-musl-x86_64.so.1 ] || [ -f /lib/ld-musl-aarch64.so.1 ]; then
            LIBC="musl"
        else
            LIBC="gnu"
        fi
        if [ "$TARGET_ARCH" = "arm" ]; then
            TARGET="${TARGET_ARCH}-unknown-linux-${LIBC}eabihf"
        elif [ "$TARGET_ARCH" = "armv7" ]; then
            TARGET="${TARGET_ARCH}-unknown-linux-${LIBC}eabihf"
        else
            TARGET="${TARGET_ARCH}-unknown-linux-${LIBC}"
        fi
        ;;
    darwin)
        TARGET="universal-apple-darwin"
        ;;
    freebsd)
        TARGET="${TARGET_ARCH}-unknown-freebsd"
        ;;
    *)
        echo "Unsupported operating system: $OS" >&2
        exit 1
        ;;
esac

BINARY_NAME="kombu-${TARGET}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${BINARY_NAME}"
CHECKSUMS_URL="https://github.com/${REPO}/releases/latest/download/SHA256SUMS"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

echo "Downloading ${BINARY_NAME}..."
curl -fsSL "$DOWNLOAD_URL" -o "${TMP_DIR}/${BINARY_NAME}"
curl -fsSL "$CHECKSUMS_URL" -o "${TMP_DIR}/SHA256SUMS"

cd "$TMP_DIR"
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c SHA256SUMS --ignore-missing
elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c SHA256SUMS --ignore-missing
fi

mkdir -p "$INSTALL_DIR"
install -m 755 "${TMP_DIR}/${BINARY_NAME}" "${INSTALL_DIR}/kombu"
echo "Kombu installed successfully to ${INSTALL_DIR}/kombu"

if [ ! -d "$CONFIG_DIR" ]; then
    mkdir -p "$CONFIG_DIR"
fi

if [ ! -f "$ENV_FILE" ]; then
    SECRET="$(head -c 32 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 32)"
    cat <<EOF > "$ENV_FILE"
DATABASE_URL="postgres://kombu:kombu@localhost:5432/kombu"
APP_SECRET="${SECRET}"
DATABASE_MAX_CONNECTIONS=80
COLLECT_RATE_LIMIT=3000
LISTEN="0.0.0.0:3000"
STORAGE_ENGINE="postgres"
EOF
    chmod 600 "$ENV_FILE"
    echo "Generated environment configuration at ${ENV_FILE}"
fi

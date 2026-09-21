# Running Kombu with Rootless Podman

This directory contains configuration to deploy Kombu Analytics rootlessly using `podman-compose` and manage it as a systemd user unit.

## Quick Start

1. Copy the sample environment file:
   ```bash
   cp podman/env.sample podman/.env
   ```

2. Start with podman-compose:
   ```bash
   podman-compose -f podman/podman-compose.yml up -d
   ```

3. (Optional) Install as a systemd user service:
   ```bash
   chmod +x podman/install-systemd-user-service
   ./podman/install-systemd-user-service
   systemctl --user start kombu.service
   ```

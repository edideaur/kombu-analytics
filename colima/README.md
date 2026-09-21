# Colima Deployment for Kombu Analytics

Colima provides container runtimes on macOS and Linux with minimal setup.

## Prerequisites

* Colima CLI installed via Homebrew (`brew install colima`) or package manager
* Docker CLI installed (`brew install docker docker-compose`)

## Quick Start

Run the automated launcher:

```bash
chmod +x colima/start-colima.sh
./colima/start-colima.sh
```

## Manual Start

1. Start the Colima virtual machine:

```bash
colima start kombu --config colima/colima.yaml
```

2. Deploy the container stack:

```bash
docker compose -p kombu -f colima/docker-compose.colima.yml up -d
```

3. Verify health status:

```bash
curl -f http://localhost:3000/api/health
```

## Health Checks and Auto Recovery

The Docker Compose configuration includes active health checks against `/api/health`. If an issue occurs, Docker automatically restarts the container according to the `restart: always` directive.

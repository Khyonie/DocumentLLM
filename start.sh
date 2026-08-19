#!/usr/bin/env bash

set -euo pipefail

project_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
webui_url="${DOCUMENTLLM_WEBUI_URL:-http://localhost:3000}"

open_browser() {
    if command -v xdg-open >/dev/null 2>&1; then
        xdg-open "${webui_url}" >/dev/null 2>&1
    elif command -v gio >/dev/null 2>&1; then
        gio open "${webui_url}" >/dev/null 2>&1
    elif command -v open >/dev/null 2>&1; then
        open "${webui_url}"
    else
        echo "Could not find a browser opener. Open ${webui_url} manually." >&2
    fi
}

cd "${project_directory}"

if ! docker compose version >/dev/null 2>&1; then
    echo "Docker Compose is required. Install the Docker Compose plugin and try again." >&2
    exit 1
fi

# Retire the previous standalone container while preserving its external data volume.
if docker inspect open-webui >/dev/null 2>&1; then
    compose_project="$(docker inspect --format '{{index .Config.Labels "com.docker.compose.project"}}' open-webui 2>/dev/null || true)"
    if [[ "${compose_project}" != "documentllm" ]] \
        && [[ "$(docker inspect --format '{{.State.Running}}' open-webui)" == "true" ]]; then
        echo "Stopping the previous standalone Open WebUI container..."
        docker stop open-webui >/dev/null
    fi
fi

if ! docker volume inspect open-webui >/dev/null 2>&1; then
    echo "Creating the persistent Open WebUI data volume..."
    docker volume create open-webui >/dev/null
fi

echo "Building and starting DocumentLLM, Ollama, and Open WebUI..."
docker compose up --detach --build

for _ in {1..180}; do
    if curl --silent --fail http://localhost:3001/health >/dev/null 2>&1 \
        && curl --silent --fail "${webui_url}" >/dev/null 2>&1; then
        echo "DocumentLLM is ready. Opening ${webui_url}..."
        open_browser
        echo "Use 'docker compose logs --follow' to view logs."
        echo "Use 'docker compose down' to stop the stack."
        exit 0
    fi

    if docker compose ps --status exited --quiet | grep --quiet .; then
        echo "A service exited during startup:" >&2
        docker compose ps >&2
        docker compose logs --tail 100 >&2
        exit 1
    fi

    sleep 1
done

echo "The stack did not become ready within 180 seconds." >&2
docker compose ps >&2
docker compose logs --tail 100 >&2
exit 1

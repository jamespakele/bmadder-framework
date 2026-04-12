#!/usr/bin/env bash
# Build the release binary, package into a Docker image, push to GHCR, then deploy.
#
# Usage: ./scripts/deploy-push.sh [tag]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
R2V2_DIR="$REPO_ROOT/r2v2"
IMAGE="ghcr.io/jamespakele/r2v2"
TAG="${1:-latest}"

# Load .env for GITHUB_PAT
if [ -f "$R2V2_DIR/.env" ]; then
    # shellcheck disable=SC2046
    export $(grep -v '^#' "$R2V2_DIR/.env" | grep -v '^$' | xargs)
fi

if [ -z "${GITHUB_PAT:-}" ]; then
    echo "❌ GITHUB_PAT not set in r2v2/.env"
    exit 1
fi

echo "▶ Logging in to GHCR..."
echo "$GITHUB_PAT" | docker login ghcr.io -u jamespakele --password-stdin

echo "▶ Building release binary..."
cd "$R2V2_DIR"
cargo build --release -p r2-cli

echo "▶ Building Docker image..."
docker build -f Dockerfile.deploy -t "${IMAGE}:${TAG}" .

echo "▶ Pushing ${IMAGE}:${TAG}..."
docker push "${IMAGE}:${TAG}"

echo ""
echo "✅ Image pushed: ${IMAGE}:${TAG}"
echo ""
echo "▶ Triggering deploy workflow on GitHub..."
gh workflow run deploy.yml --repo jamespakele/ai-r2v2 2>/dev/null || \
    echo "  (gh CLI not found — trigger manually at: https://github.com/jamespakele/ai-r2v2/actions/workflows/deploy.yml)"

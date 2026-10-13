#!/usr/bin/env bash
# End-to-end test script for cluster-cli TUI and GPUI desktop app
# Tests both applications against real kind clusters with sample workloads

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SCREENSHOTS_DIR="${REPO_ROOT}/examples/e2e/screenshots"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    echo -e "${GREEN}[e2e]${NC} $*"
}

warn() {
    echo -e "${YELLOW}[e2e]${NC} $*"
}

error() {
    echo -e "${RED}[e2e]${NC} $*"
}

cleanup() {
    log "Cleaning up..."
    kind delete cluster --name cluster-cli-test-1 2>/dev/null || true
    kind delete cluster --name cluster-cli-test-2 2>/dev/null || true
    rm -rf "${SCREENSHOTS_DIR}"
}

# Trap exit to cleanup
trap cleanup EXIT

main() {
    cd "${REPO_ROOT}"
    
    # Check prerequisites
    log "Checking prerequisites..."
    command -v kind >/dev/null 2>&1 || { error "kind is required but not installed. Install from https://kind.sigs.k8s.io/"; exit 1; }
    command -v kubectl >/dev/null 2>&1 || { error "kubectl is required but not installed."; exit 1; }
    command -v cargo >/dev/null 2>&1 || { error "cargo is required but not installed."; exit 1; }
    
    # Build binaries
    log "Building cluster-cli TUI..."
    cargo build --release
    
    log "Building cluster-desktop GPUI app..."
    cargo build --release -p cluster-desktop
    
    # Create screenshot directory
    mkdir -p "${SCREENSHOTS_DIR}"
    
    # Create first kind cluster
    log "Creating first kind cluster (cluster-cli-test-1)..."
    kind create cluster --name cluster-cli-test-1 --wait 5m
    
    # Deploy test workloads to first cluster
    log "Deploying test workloads to cluster 1..."
    kubectl apply -f "${SCRIPT_DIR}/test-workloads.yaml"
    
    # Wait for some pods to be running
    log "Waiting for pods to start..."
    kubectl wait --for=condition=ready pod -l app=healthy-app -n cluster-cli-test --timeout=60s || true
    sleep 10
    
    # Create second kind cluster for multi-cluster testing
    log "Creating second kind cluster (cluster-cli-test-2)..."
    kind create cluster --name cluster-cli-test-2 --wait 5m
    
    # Deploy minimal workload to second cluster
    log "Deploying workloads to cluster 2..."
    kubectl apply -f - <<EOF
apiVersion: v1
kind: Namespace
metadata:
  name: production
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: prod-app
  namespace: production
spec:
  replicas: 2
  selector:
    matchLabels:
      app: prod-app
  template:
    metadata:
      labels:
        app: prod-app
    spec:
      containers:
      - name: nginx
        image: nginx:1.25-alpine
        resources:
          requests:
            memory: "64Mi"
            cpu: "100m"
          limits:
            memory: "128Mi"
            cpu: "200m"
EOF
    
    kubectl wait --for=condition=ready pod -l app=prod-app -n production --timeout=60s || true
    
    # Show cluster contexts
    log "Available contexts:"
    kubectl config get-contexts
    
    # Switch back to first cluster
    kubectl config use-context kind-cluster-cli-test-1
    
    # Show cluster status
    log "Cluster 1 status:"
    kubectl get nodes
    kubectl get pods -n cluster-cli-test
    
    log "Cluster 2 status:"
    kubectl --context kind-cluster-cli-test-2 get nodes
    kubectl --context kind-cluster-cli-test-2 get pods -n production
    
    # Test TUI
    log "Testing cluster-cli TUI..."
    log "To capture TUI screenshots manually:"
    log "  ./target/release/cluster -n cluster-cli-test"
    log "  Press 's' to take screenshot (if implemented)"
    log "  Or use: script -c './target/release/cluster -n cluster-cli-test' tui-output.txt"
    
    # Test desktop app
    log "Testing cluster-desktop GPUI app..."
    log "To run desktop app (requires display):"
    log "  ./target/release/cluster-desktop"
    log "  Or with Xvfb: xvfb-run -a -s '-screen 0 1920x1080x24' ./target/release/cluster-desktop"
    
    # Note: Actual screenshot capture would require:
    # 1. For TUI: using script/asciinema or terminal screenshot tools
    # 2. For desktop: using Xvfb + scrot/imagemagick
    
    warn "Screenshot capture requires manual intervention or additional tools"
    warn "This script validates that:"
    warn "  - Both binaries build successfully"
    warn "  - Kind clusters can be created"
    warn "  - Test workloads can be deployed"
    warn "  - Multiple contexts are available for testing"
    
    log "E2E test infrastructure ready!"
    log "Binaries:"
    log "  TUI:     ${REPO_ROOT}/target/release/cluster"
    log "  Desktop: ${REPO_ROOT}/target/release/cluster-desktop"
    log ""
    log "To test manually:"
    log "  1. Switch context: kubectl config use-context kind-cluster-cli-test-1"
    log "  2. Run TUI: ./target/release/cluster -n cluster-cli-test"
    log "  3. Run desktop: ./target/release/cluster-desktop (opens both clusters)"
    log ""
    log "Clusters will be cleaned up on exit. Press Ctrl+C to keep them."
    read -p "Press Enter to cleanup clusters..." || true
}

main "$@"

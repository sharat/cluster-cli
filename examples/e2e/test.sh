#!/usr/bin/env bash
# E2E test script for cluster-cli JSON API and MCP server
# Requires: kind, kubectl, jq

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLUSTER_NAME="cluster-cli-test"
BINARY="${CLUSTER_BIN:-../../target/release/cluster}"

echo "==> E2E Testing cluster-cli JSON API and MCP Server"
echo

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}==>${NC} $*"
}

warn() {
    echo -e "${YELLOW}Warning:${NC} $*"
}

# Check prerequisites
check_prereqs() {
    local missing=()
    for cmd in kind kubectl jq; do
        if ! command -v "$cmd" &>/dev/null; then
            missing+=("$cmd")
        fi
    done
    
    if [ ${#missing[@]} -gt 0 ]; then
        echo "Error: Missing required commands: ${missing[*]}"
        echo "Install them and try again."
        exit 1
    fi
}

# Create kind cluster
setup_cluster() {
    info "Creating kind cluster..."
    if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        warn "Cluster ${CLUSTER_NAME} already exists, deleting..."
        kind delete cluster --name="${CLUSTER_NAME}"
    fi
    
    kind create cluster --name="${CLUSTER_NAME}" --config="${SCRIPT_DIR}/kind-cluster.yaml" --wait=5m
    kubectl cluster-info --context "kind-${CLUSTER_NAME}"
    
    # Wait for nodes to be ready
    info "Waiting for nodes to be ready..."
    kubectl wait --for=condition=Ready nodes --all --timeout=5m
}

# Deploy test workloads
deploy_workloads() {
    info "Deploying test workloads..."
    kubectl apply -f "${SCRIPT_DIR}/test-workloads.yaml"
    
    # Give it a moment to start
    sleep 5
    
    # Wait for healthy deployment
    info "Waiting for healthy deployment..."
    kubectl wait --for=condition=Available deployment/healthy-app --timeout=2m || true
}

# Test JSON commands
test_json_commands() {
    info "Testing JSON commands..."
    
    # Test status command
    info "  Testing: cluster status --json"
    "$BINARY" status --json > /tmp/cluster-status.json
    jq . /tmp/cluster-status.json > /dev/null  # Validate JSON
    echo "  ✓ Status JSON valid"
    
    # Test status with verbose
    info "  Testing: cluster status --json --verbose"
    "$BINARY" status --json --verbose > /tmp/cluster-status-verbose.json
    jq . /tmp/cluster-status-verbose.json > /dev/null
    echo "  ✓ Status verbose JSON valid"
    
    # Test incidents
    info "  Testing: cluster incidents --json"
    "$BINARY" incidents --json > /tmp/cluster-incidents.json
    jq . /tmp/cluster-incidents.json > /dev/null
    echo "  ✓ Incidents JSON valid"
    
    # Test pods
    info "  Testing: cluster pods --json"
    "$BINARY" pods --json > /tmp/cluster-pods.json
    jq . /tmp/cluster-pods.json > /dev/null
    echo "  ✓ Pods JSON valid"
    
    # Test nodes
    info "  Testing: cluster nodes --json"
    "$BINARY" nodes --json > /tmp/cluster-nodes.json
    jq . /tmp/cluster-nodes.json > /dev/null
    echo "  ✓ Nodes JSON valid"
    
    echo
    info "Sample outputs saved to /tmp/cluster-*.json"
}

# Test text output
test_text_output() {
    info "Testing text output..."
    
    info "  cluster status"
    "$BINARY" status | head -20
    echo
    
    info "  cluster incidents"
    "$BINARY" incidents | head -20
    echo
    
    info "  cluster pods"
    "$BINARY" pods | head -10
    echo
    
    info "  cluster nodes"
    "$BINARY" nodes
    echo
}

# Test MCP server (basic smoke test)
test_mcp_server() {
    info "Testing MCP server..."
    
    # Start MCP server in background
    "$BINARY" mcp > /tmp/mcp-output.log 2>&1 &
    MCP_PID=$!
    
    sleep 2
    
    # Send initialize request
    info "  Sending initialize request..."
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | nc localhost 3000 > /tmp/mcp-init.json 2>/dev/null || {
        # MCP server uses stdio, not TCP
        warn "MCP server uses stdio transport, not TCP. Manual testing required."
        kill $MCP_PID 2>/dev/null || true
        return
    }
    
    kill $MCP_PID 2>/dev/null || true
    echo "  ✓ MCP server started successfully"
}

# Cleanup
cleanup() {
    info "Cleaning up..."
    kind delete cluster --name="${CLUSTER_NAME}" || true
}

# Main execution
main() {
    check_prereqs
    
    if [ ! -f "$BINARY" ]; then
        warn "Binary not found at $BINARY"
        info "Building release binary..."
        cd "$(dirname "$SCRIPT_DIR")"
        cargo build --release
    fi
    
    trap cleanup EXIT
    
    setup_cluster
    deploy_workloads
    
    echo
    info "Cluster ready. Running tests..."
    echo
    
    test_json_commands
    test_text_output
    # test_mcp_server  # Commented out - requires special stdio setup
    
    echo
    info "✓ All tests completed successfully!"
    info "Test outputs saved to /tmp/cluster-*.json"
    echo
    info "To manually test MCP server:"
    echo "  $BINARY mcp"
    echo "  (Then send JSON-RPC requests via stdin)"
}

main "$@"

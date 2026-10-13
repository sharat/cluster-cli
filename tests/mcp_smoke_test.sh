#!/usr/bin/env bash
# MCP smoke test - verifies MCP server starts and responds to basic JSON-RPC requests
# Runs without requiring a Kubernetes cluster

set -euo pipefail

BINARY="${1:-./target/debug/cluster}"

if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found at $BINARY"
    exit 1
fi

echo "==> Testing MCP server smoke test (no cluster required)"
echo "Binary: $BINARY"
echo

# Create test responses file
TEST_OUTPUT=$(mktemp)
trap "rm -f $TEST_OUTPUT" EXIT

# Send test requests to MCP server and capture responses
{
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
    echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_contexts","arguments":{}}}'
} | timeout 5 "$BINARY" mcp 2>/dev/null > "$TEST_OUTPUT" || {
    echo "Error: MCP server failed or timed out"
    cat "$TEST_OUTPUT" 2>/dev/null || true
    exit 1
}

echo "✓ MCP server responded to requests"
echo

# Count responses
RESPONSE_COUNT=$(wc -l < "$TEST_OUTPUT")
if [ "$RESPONSE_COUNT" -lt 3 ]; then
    echo "Error: Expected 3 responses, got $RESPONSE_COUNT"
    cat "$TEST_OUTPUT"
    exit 1
fi

echo "Test 1: Validate initialize response"
LINE1=$(sed -n '1p' "$TEST_OUTPUT")
if ! echo "$LINE1" | jq -e '.result.protocolVersion' > /dev/null 2>&1; then
    echo "Error: Invalid initialize response"
    echo "$LINE1"
    exit 1
fi
PROTOCOL=$(echo "$LINE1" | jq -r '.result.protocolVersion')
SERVER_NAME=$(echo "$LINE1" | jq -r '.result.serverInfo.name')
echo "✓ Initialize response valid: $SERVER_NAME protocol $PROTOCOL"
echo

echo "Test 2: Validate tools/list response"
LINE2=$(sed -n '2p' "$TEST_OUTPUT")
if ! echo "$LINE2" | jq -e '.result.tools | length' > /dev/null 2>&1; then
    echo "Error: Invalid tools/list response"
    echo "$LINE2"
    exit 1
fi
TOOL_COUNT=$(echo "$LINE2" | jq -r '.result.tools | length')
echo "✓ Tools list response valid: $TOOL_COUNT tools"

# Verify expected tools exist
EXPECTED_TOOLS=(get_cluster_health list_incidents list_pods describe_pod explain_pod get_logs list_nodes list_contexts)
for tool in "${EXPECTED_TOOLS[@]}"; do
    if ! echo "$LINE2" | jq -e ".result.tools[] | select(.name == \"$tool\")" > /dev/null 2>&1; then
        echo "Error: Expected tool '$tool' not found"
        exit 1
    fi
done
echo "✓ All 8 expected tools present: ${EXPECTED_TOOLS[*]}"
echo

echo "Test 3: Validate tools/call response (list_contexts)"
LINE3=$(sed -n '3p' "$TEST_OUTPUT")

# Check for valid JSON-RPC response (either success or error is fine without cluster)
if ! echo "$LINE3" | jq -e '.jsonrpc' > /dev/null 2>&1; then
    echo "Error: Invalid JSON-RPC response"
    echo "$LINE3"
    exit 1
fi

if echo "$LINE3" | jq -e '.error' > /dev/null 2>&1; then
    # Error is expected without kubectl/cluster
    ERROR_MSG=$(echo "$LINE3" | jq -r '.error.message' | head -c 80)
    echo "✓ list_contexts returned error (expected without cluster):"
    echo "  $ERROR_MSG..."
elif echo "$LINE3" | jq -e '.result.content[0].text' > /dev/null 2>&1; then
    # Success case (if kubectl is available)
    echo "✓ list_contexts response valid (kubectl available)"
else
    echo "Error: Invalid tools/call response format"
    echo "$LINE3"
    exit 1
fi

echo
echo "==> All MCP smoke tests passed ✓"
echo "The MCP server correctly:"
echo "  - Starts and responds to stdio requests"
echo "  - Implements JSON-RPC 2.0 protocol"
echo "  - Exposes all 8 expected tools"
echo "  - Handles tool calls gracefully (even without cluster)"

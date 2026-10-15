#!/bin/bash
set -e

echo "=== Timeline Feature E2E Test ==="
echo "This script deploys workloads to demonstrate the incident timeline feature."
echo ""

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Check kubectl is available
if ! command -v kubectl &> /dev/null; then
    echo "Error: kubectl not found"
    exit 1
fi

# Check cluster connection
if ! kubectl cluster-info &> /dev/null; then
    echo "Error: Cannot connect to Kubernetes cluster"
    exit 1
fi

echo "Step 1: Deploy healthy application (baseline)"
kubectl apply -f 01-healthy-deployment.yaml
echo "Waiting for healthy-app to be ready..."
kubectl wait --for=condition=available --timeout=60s deployment/healthy-app || true
echo ""

echo "Step 2: Deploy rollout-app with good image"
kubectl apply -f 05-rollout-with-bad-image.yaml
echo "Waiting for rollout-app initial deployment..."
kubectl wait --for=condition=available --timeout=60s deployment/rollout-app || true
echo ""

echo "Step 3: Trigger a rollout with a bad image (creates precursor)"
echo "Updating rollout-app image to trigger rollout..."
kubectl set image deployment/rollout-app web=nginx:nonexistent-tag
echo "Sleeping 10s to let rollout event appear in timeline..."
sleep 10
echo ""

echo "Step 4: Deploy crashloop application"
kubectl apply -f 02-crashloop-deployment.yaml
echo "Waiting for crashloop-app to start crashing (30s)..."
sleep 30
echo ""

echo "Step 5: Deploy OOM-prone application"
kubectl apply -f 03-oom-deployment.yaml
echo "Waiting for OOM events to appear (30s)..."
sleep 30
echo ""

echo "Step 6: Deploy unschedulable pod"
kubectl apply -f 04-unschedulable-pod.yaml
echo "Pod will remain unschedulable due to resource constraints..."
sleep 10
echo ""

echo "=== Deployment complete ==="

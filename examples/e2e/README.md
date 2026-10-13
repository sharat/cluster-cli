# End-to-End Testing for cluster-cli

This directory contains end-to-end test infrastructure for both the TUI (`cluster`) and GPUI desktop app (`cluster-desktop`).

## Test Setup

The e2e test creates two kind clusters with various workload states to validate the full application stack:

### Cluster 1: `kind-cluster-cli-test-1`
Comprehensive test workloads in the `cluster-cli-test` namespace:
- **Healthy deployment**: 3-replica nginx deployment with proper resource requests/limits
- **Crash-looping pod**: Busybox container that exits with error every 5 seconds
- **Pending pod**: Pod with impossible resource requests (999 CPU, 999Gi memory) that will never schedule
- **OOMKilled pod**: Busybox trying to allocate unlimited memory with a 32Mi limit
- **StatefulSet**: 2-replica stateful workload
- **DaemonSet**: Node-level workload
- **HPA**: Horizontal Pod Autoscaler targeting the healthy deployment
- **Service**: ClusterIP service for the healthy app

### Cluster 2: `kind-cluster-cli-test-2`
Simple production-like workload in the `production` namespace:
- 2-replica nginx deployment

## Running the Tests

### Prerequisites

- Docker (for kind)
- kubectl
- Rust toolchain (cargo)

### Quick Start

```bash
# Run full e2e test (creates clusters, deploys workloads, builds binaries)
./examples/e2e/run-e2e-test.sh
```

### Manual Testing

```bash
# 1. Create clusters and deploy workloads
./examples/e2e/run-e2e-test.sh

# 2. In another terminal, test the TUI
./target/release/cluster -n cluster-cli-test

# 3. Test the desktop app (multi-cluster view)
./target/release/cluster-desktop
```

### Testing with Xvfb (Headless)

For CI/testing without a real display:

```bash
# Install Xvfb and screenshot tools
sudo apt-get install -y xvfb scrot imagemagick

# Run desktop app under Xvfb
xvfb-run -a -s '-screen 0 1920x1080x24' ./target/release/cluster-desktop &
XVFB_PID=$!

# Wait for app to load
sleep 5

# Capture screenshot
DISPLAY=:99 scrot -o screenshot.png

# Cleanup
kill $XVFB_PID
```

## Test Workloads

All test workloads are defined in `test-workloads.yaml`. They demonstrate:

1. **Health scoring**: Mix of healthy and unhealthy pods affects the cluster grade
2. **Pod states**: Running, Pending, CrashLoopBackOff, OOMKilled
3. **Workload types**: Deployment, StatefulSet, DaemonSet, Pod
4. **Resource management**: Various resource requests and limits
5. **Scaling**: HPA configuration
6. **Networking**: Service exposure

## What the Tests Validate

### TUI (`cluster`)
- ✅ Dashboard view with cluster health score
- ✅ Node metrics display
- ✅ Pod list with status indicators
- ✅ Incident/event tracking
- ✅ Workload summaries
- ✅ Pod detail view with logs
- ✅ Real-time updates (kubectl watch)

### Desktop App (`cluster-desktop`)
- ✅ Multi-cluster overview (both contexts visible)
- ✅ Cluster cards with health grades
- ✅ Per-cluster detail view
- ✅ Pod list and filtering
- ✅ Pod detail with live logs
- ✅ Namespace picker
- ✅ Desktop notifications for incidents

## Expected Behavior

### Health Scoring
With the test workloads, Cluster 1 should show:
- **Grade**: Likely D or F due to crash-looping and OOM pods
- **Critical Pods**: 2-3 (crashloop, oom, potentially the pending pod)
- **Incidents**: CrashLoopBackOff, OOMKilled, FailedScheduling events

Cluster 2 should show:
- **Grade**: A or B (healthy production deployment)
- **Critical Pods**: 0
- **Incidents**: None or minimal

### Pod States
- `healthy-app-*` pods: Green/Healthy status
- `crashloop-pod`: Red/Critical with increasing restart count
- `pending-pod`: Yellow/Warning, reason: "Insufficient cpu/memory"
- `oom-pod`: Red/Critical, last terminated reason: "OOMKilled"

## Cleanup

The test script automatically cleans up both clusters on exit. To keep the clusters for manual testing:

```bash
# Run script and press Ctrl+C when prompted
./examples/e2e/run-e2e-test.sh
# Press Ctrl+C instead of Enter

# Clusters will remain active. To delete manually later:
kind delete cluster --name cluster-cli-test-1
kind delete cluster --name cluster-cli-test-2
```

## CI Integration

For GitHub Actions or similar CI:

```yaml
- name: Run e2e tests
  run: |
    # Install kind
    curl -Lo ./kind https://kind.sigs.k8s.io/dl/v0.20.0/kind-linux-amd64
    chmod +x ./kind
    sudo mv ./kind /usr/local/bin/kind
    
    # Run tests
    ./examples/e2e/run-e2e-test.sh
    
    # Optional: Capture screenshots for artifacts
    xvfb-run -a ./target/release/cluster-desktop &
    sleep 10
    DISPLAY=:99 scrot screenshots/desktop-view.png
```

## Troubleshooting

### Docker Not Available
```
ERROR: failed to create cluster: failed to get docker info
```
**Solution**: Ensure Docker is running: `sudo systemctl start docker`

### Port Conflicts
```
ERROR: failed to create cluster: port 6443 is already in use
```
**Solution**: Delete existing kind clusters: `kind delete cluster --name <name>`

### Build Failures
```
error: could not compile `cluster-desktop`
```
**Solution**: Install system dependencies:
```bash
sudo apt-get install -y \
  libfreetype6-dev \
  libfontconfig1-dev \
  libxkbcommon-dev \
  libxkbcommon-x11-dev \
  libwayland-dev \
  libvulkan-dev
```

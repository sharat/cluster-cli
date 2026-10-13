# End-to-End Testing for cluster-cli

This directory contains end-to-end tests and sample workloads for testing cluster-cli against a real Kubernetes cluster.

## Overview

The E2E test suite:
- Creates a local kind cluster with test workloads
- Deploys various pod states (healthy, crash-looping, OOMKilled, unschedulable, rolling updates)
- Builds and runs cluster-cli headlessly in a tmux session
- Navigates through key TUI views (dashboard, pod detail, node detail, incidents)
- Captures screenshots as ANSI text and converts to PNG images
- Posts results as PR comments on GitHub

## Files

```
examples/e2e/
├── README.md                      # This file
├── manifests/                     # Kubernetes test workloads
│   ├── healthy-deployment.yaml    # 3-replica healthy nginx deployment
│   ├── crashloop-pod.yaml         # Pod that crashes and restarts
│   ├── oom-pod.yaml               # Pod that gets OOMKilled
│   ├── unschedulable-pod.yaml     # Pod that can't be scheduled
│   └── rollout-deployment.yaml    # Deployment for testing rollouts
├── test-views.sh                  # Script to drive TUI and capture screenshots
├── convert-screenshots.sh         # Convert ANSI text to PNG images
└── ansi-to-png.js                 # Node.js helper for ANSI conversion
```

## Running Locally

### Prerequisites

```bash
# Install kind for local Kubernetes clusters
# See: https://kind.sigs.k8s.io/docs/user/quick-start/
brew install kind  # macOS
# or
curl -Lo ./kind https://kind.sigs.k8s.io/dl/v0.20.0/kind-linux-amd64
chmod +x ./kind
sudo mv ./kind /usr/local/bin/kind

# Install kubectl
brew install kubectl  # macOS
# or follow https://kubernetes.io/docs/tasks/tools/

# Install tmux (for headless terminal)
brew install tmux  # macOS
sudo apt-get install tmux  # Ubuntu/Debian

# Install screenshot conversion tools (choose one)

# Option 1: termshot (Rust, recommended)
cargo install termshot

# Option 2: ansi-to-svg + librsvg
npm install -g ansi-to-svg
brew install librsvg  # macOS
sudo apt-get install librsvg2-bin  # Ubuntu/Debian
```

### Running the Full E2E Test

```bash
# 1. Create a kind cluster
kind create cluster --name cluster-cli-e2e

# 2. Deploy test workloads
kubectl apply -f examples/e2e/manifests/

# 3. Wait for failure states to appear (30-60 seconds)
watch kubectl get pods  # Watch until crashloop-pod shows restarts

# 4. Build cluster-cli
cargo build --release

# 5. Run screenshot capture
mkdir -p screenshots
./examples/e2e/test-views.sh ./target/release/cluster ./screenshots

# 6. Convert to PNG (if you have conversion tools installed)
./examples/e2e/convert-screenshots.sh ./screenshots

# 7. View results
ls -lh screenshots/
# Open PNG files with your image viewer
```

### Cleanup

```bash
# Delete the kind cluster
kind delete cluster --name cluster-cli-e2e
```

## Customizing Views

The `test-views.sh` script accepts a comma-separated list of views to capture:

```bash
# Default views
./examples/e2e/test-views.sh ./target/release/cluster ./screenshots dashboard,pod-detail,node-detail,incidents

# Add timeline view (if implemented on your branch)
./examples/e2e/test-views.sh ./target/release/cluster ./screenshots dashboard,pod-detail,node-detail,incidents,timeline

# Only capture dashboard
./examples/e2e/test-views.sh ./target/release/cluster ./screenshots dashboard
```

### Available Views

- `dashboard` - Main cluster overview (default landing view)
- `pod-detail` - Detailed view of a selected pod
- `node-detail` - Detailed view of a selected node
- `incidents` - Events and incidents view
- `timeline` - Timeline view (if available on branch)

To add a new view to the test suite:

1. Edit `test-views.sh` and add a new case in the view loop
2. Add the appropriate key sequences to navigate to your view
3. Call `capture_screenshot "NN-your-view-name"`

## CI/CD Integration

The E2E tests run automatically on every pull request via the GitHub Actions workflow at `.github/workflows/e2e-screenshots.yml`.

The workflow:
1. Creates a kind cluster
2. Deploys test workloads
3. Waits for failure states
4. Builds cluster-cli
5. Captures screenshots
6. Converts to PNG
7. Uploads as artifacts
8. Posts/updates a sticky PR comment with embedded images

### Viewing Results

- **Artifacts**: Download from the GitHub Actions run page
- **PR Comments**: Screenshots are automatically posted to PRs (for same-repo PRs only)
- **Raw Images**: Available at `https://github.com/sharat/cluster-cli/raw/e2e-screenshots/pr-<number>/`

## Troubleshooting

### Screenshots are blank or incomplete

- Increase `WAIT_DATA` or `WAIT_RENDER` in `test-views.sh`
- Check that cluster-cli successfully connects to the cluster
- Verify test workloads are deployed: `kubectl get pods`

### Conversion to PNG fails

- Install one of the conversion tool combinations listed in Prerequisites
- Check that ANSI text files exist in the screenshots directory
- Try running conversion manually: `./examples/e2e/convert-screenshots.sh ./screenshots`

### kind cluster fails to start

- Check Docker is running
- Try: `kind delete cluster --name cluster-cli-e2e` then recreate
- Check kind logs: `kind export logs ./kind-logs --name cluster-cli-e2e`

### Test workloads don't reach failure states

- Wait longer (crash loops need ~2+ restarts, OOM may need 30-60 seconds)
- Check pod status: `kubectl describe pod crashloop-pod`
- Check events: `kubectl get events --sort-by='.lastTimestamp'`

## Adding New Test Workloads

To add a new test scenario:

1. Create a YAML manifest in `examples/e2e/manifests/`
2. Update the workflow to wait for your specific condition
3. Update this README to document the new workload

Example manifest structure:

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: my-test-pod
  namespace: default
  labels:
    app: my-test
spec:
  containers:
  - name: container
    image: nginx:alpine
    resources:
      requests:
        cpu: 10m
        memory: 16Mi
      limits:
        cpu: 20m
        memory: 32Mi
```

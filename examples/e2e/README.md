# End-to-End Testing for cluster-cli

This directory contains both automated CI/CD testing infrastructure and manual testing scenarios for cluster-cli.

## 📸 Automated CI/CD Testing (GitHub Actions)

The E2E test suite automatically runs on every pull request, creating a kind cluster with test workloads and capturing TUI screenshots.

### Overview

The automated test suite:
- Creates a local kind cluster with test workloads
- Deploys various pod states (healthy, crash-looping, ImagePullBackOff, OOMKilled, unschedulable, job, rolling updates)
- Builds and runs cluster-cli headlessly in a tmux session
- Navigates through key TUI views (dashboard, pod detail, node detail, incidents)
- Captures screenshots as ANSI text and converts to PNG images
- Posts results as PR comments on GitHub

### Files for CI/CD Testing

```
examples/e2e/
├── manifests/                     # Kubernetes test workloads for CI
│   ├── healthy-deployment.yaml    # 3-replica healthy nginx deployment
│   ├── crashloop-pod.yaml         # Pod that crashes and restarts
│   ├── imagepullbackoff-pod.yaml  # Pod with missing image
│   ├── oom-pod.yaml               # Pod that gets OOMKilled
│   ├── unschedulable-pod.yaml     # Pod that can't be scheduled
│   ├── test-job.yaml              # Kubernetes Job
│   ├── rollout-deployment.yaml    # Deployment for testing rollouts
│   └── sidecar-crash-pod.yaml     # Native sidecar that crash-loops next to a running app
├── test-views.sh                  # Script to drive TUI and capture screenshots
├── convert-screenshots.sh         # Convert ANSI text to PNG images
├── test-local.sh                  # Local verification script
└── ansi-to-png.js                 # Node.js helper for ANSI conversion
```

### Running Automated Tests Locally

#### Prerequisites

```bash
# Install kind for local Kubernetes clusters
brew install kind  # macOS
# or
curl -Lo ./kind https://kind.sigs.k8s.io/dl/v0.20.0/kind-linux-amd64
chmod +x ./kind && sudo mv ./kind /usr/local/bin/kind

# Install kubectl
brew install kubectl  # macOS

# Install tmux (for headless terminal)
brew install tmux  # macOS
sudo apt-get install tmux  # Ubuntu/Debian

# Install screenshot conversion tools (choose one)
cargo install termshot  # Option 1: Rust-based
# or
npm install -g ansi-to-svg && brew install librsvg  # Option 2: Node + SVG
```

#### Running the Full E2E Test

```bash
# 1. Create a kind cluster
kind create cluster --name cluster-cli-e2e

# 2. Deploy test workloads
kubectl apply -f examples/e2e/manifests/

# 3. Wait for failure states to appear (30-60 seconds)
watch kubectl get pods

# 4. Build cluster-cli
cargo build --release

# 5. Run screenshot capture
mkdir -p screenshots
./examples/e2e/test-views.sh ./target/release/cluster ./screenshots

# 6. Convert to PNG (if you have conversion tools installed)
./examples/e2e/convert-screenshots.sh ./screenshots

# 7. Cleanup
kind delete cluster --name cluster-cli-e2e
```

---

## 🔍 Manual Timeline Feature Testing

The numbered manifests and test scripts are designed for manually exploring the incident timeline feature.

### Test Scenario

The test deploys several workloads that trigger various timeline events:

1. **01-healthy-deployment** - A baseline healthy deployment (nginx)
2. **02-crashloop-deployment** - A container that crashes repeatedly (busybox with exit 1)
3. **03-oom-deployment** - A memory-intensive workload (stress tool exceeding limits)
4. **04-unschedulable-pod** - A pod with impossible resource requests
5. **05-rollout-with-bad-image** - A deployment that gets updated with a bad image (triggers rollout event)

### Files for Manual Testing

```
examples/e2e/
├── 01-healthy-deployment.yaml
├── 02-crashloop-deployment.yaml
├── 03-oom-deployment.yaml
├── 04-unschedulable-pod.yaml
├── 05-rollout-with-bad-image.yaml
├── kind-cluster.yaml              # Optional: kind cluster config
├── test-workloads.yaml            # Combined manifest
├── run-e2e-test.sh                # Automated deployment script
└── test.sh                        # Test runner
```

### Running Manual Timeline Tests

#### Prerequisites
- A running Kubernetes cluster (kind, minikube, k3d, or real cluster)
- kubectl configured to access the cluster
- cluster-cli binary built (`cargo build --release`)

#### Steps

1. Create a test cluster (if using kind):
```bash
kind create cluster --name test-timeline
# or with custom config:
kind create cluster --name test-timeline --config examples/e2e/kind-cluster.yaml
```

2. Run the deployment script:
```bash
cd examples/e2e
./run-e2e-test.sh
```

3. Run cluster-cli to view the timeline:
```bash
cd ../..
./target/release/cluster
```

4. Navigate the timeline:
- Press `t` to view the full timeline
- Press `T` from an incident to filter the timeline by that workload
- Press `f`/`F`/`c` to filter by namespace/kind/severity
- Press `s` to toggle newest-first sorting
- Press `q` to quit

#### Expected Results

The timeline should show:
- **ImagePullBackOff** events (if images fail to pull)
- **FailedScheduling** events (unschedulable-pod)
- **Rollout** events (deployment updates)
- **CrashLoop** events (crashloop-app restarts)
- **OOMKilled** events (if memory limits are exceeded)

If a rollout occurs within 10 minutes before crashes/restarts, those entries will be marked with a ⚡ precursor indicator.

#### Cleanup

```bash
kind delete cluster --name test-timeline
```

---

## 🛠️ Customizing Views (CI Testing)

The `test-views.sh` script accepts a comma-separated list of views to capture:

```bash
# Default views
./examples/e2e/test-views.sh ./target/release/cluster ./screenshots dashboard,pod-detail,node-detail,incidents

# Add timeline view (if implemented on your branch)
./examples/e2e/test-views.sh ./target/release/cluster ./screenshots dashboard,pod-detail,node-detail,incidents,timeline
```

### Available Views

- `dashboard` - Main cluster overview (default landing view)
- `pod-detail` - Detailed view of a selected pod
- `diagnosis` - Pod detail with the Diagnosis panel for each failing test pod; override the pods with `DIAGNOSIS_PODS=a,b`
- `node-detail` - Detailed view of a selected node
- `incidents` - Events and incidents view
- `timeline` - Timeline view (if available on branch)

---

## 📋 Adding New Test Workloads

To add a new test scenario:

1. Create a YAML manifest in `examples/e2e/manifests/` (for CI) or as a numbered file (for manual testing)
2. Update the appropriate test script to wait for your specific condition
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

---

## 🐛 Troubleshooting

### Screenshots are blank or incomplete
- Increase `WAIT_DATA` or `WAIT_RENDER` in `test-views.sh`
- Check that cluster-cli successfully connects to the cluster
- Verify test workloads are deployed: `kubectl get pods`

### Conversion to PNG fails
- Install one of the conversion tool combinations (see Prerequisites)
- Check that ANSI text files exist in the screenshots directory
- Try running conversion manually: `./examples/e2e/convert-screenshots.sh ./screenshots`

### kind cluster fails to start
- Check Docker is running
- Try: `kind delete cluster --name cluster-cli-e2e` then recreate
- Check kind logs: `kind export logs ./kind-logs --name cluster-cli-e2e`

### Test workloads don't reach failure states
- Wait longer (crash loops need ~2+ restarts, ImagePullBackOff and OOM may need 30-60 seconds)
- Check pod status: `kubectl describe pod crashloop-pod`
- Check events: `kubectl get events --sort-by='.lastTimestamp'`

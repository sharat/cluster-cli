# End-to-End Timeline Feature Test

This directory contains sample Kubernetes manifests and a test script to demonstrate the incident timeline feature in cluster-cli.

## Test Scenario

The test deploys several workloads that trigger various timeline events:

1. **healthy-app** - A baseline healthy deployment (nginx)
2. **rollout-app** - A deployment that gets updated with a bad image (triggers rollout event)
3. **crashloop-app** - A container that crashes repeatedly (busybox with exit 1)
4. **oom-app** - A memory-intensive workload (stress tool exceeding limits)
5. **unschedulable-pod** - A pod with impossible resource requests

## Running the Test

### Prerequisites
- A running Kubernetes cluster (kind, minikube, k3d, or real cluster)
- kubectl configured to access the cluster
- cluster-cli binary built (`cargo build --release`)

### Steps

1. Create a test cluster (if using kind):
```bash
kind create cluster --name test-timeline
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

## Expected Results

The timeline should show:
- **ImagePullBackOff** events (if images fail to pull)
- **FailedScheduling** events (unschedulable-pod)
- **Rollout** events (deployment updates)
- **CrashLoop** events (crashloop-app restarts)
- **OOMKilled** events (if memory limits are exceeded)

If a rollout occurs within 10 minutes before crashes/restarts, those entries will be marked with a ⚡ precursor indicator.

## Cleanup

```bash
kind delete cluster --name test-timeline
```

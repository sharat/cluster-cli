# Changelog

Notable user-facing changes to `cluster-cli` are documented here.

## Unreleased

### Added

- Select each regular container in a multi-container pod with `c`.
- Toggle between live current logs and the selected container's previous logs with `p`.
- Search the buffered log output case-insensitively with `/`.
- Toggle long-line wrapping with `w` and Kubernetes timestamps with `t`.
- Export the current 1000-line in-memory log buffer to a new plain-text file with `E`.
- Filter nodes by common AKS, EKS, GKE, Karpenter, Cluster API, and kOps pool labels with `--node-pool-filter`.

### Changed

- Log streams now request Kubernetes timestamps and surface `kubectl logs` stderr failures in the status bar.
- The focused log panel shows its container, source, follow state, wrapping, timestamp, and search-match status.
- The dashboard header displays the active node-pool filter.

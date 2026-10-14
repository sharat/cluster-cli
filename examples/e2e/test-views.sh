#!/usr/bin/env bash
# Script to drive cluster-cli TUI through various views and capture screenshots
# Usage: test-views.sh <cluster-binary> <output-dir> [view-list]

set -euo pipefail

CLUSTER_BIN="${1:-./target/release/cluster}"
OUTPUT_DIR="${2:-./screenshots}"
VIEW_LIST="${3:-dashboard,pod-detail,node-detail,incidents}"
# Pods opened by the "diagnosis" view (names from examples/e2e/manifests/)
DIAGNOSIS_PODS="${DIAGNOSIS_PODS:-crashloop-pod,oom-killed-pod,imagepullbackoff-pod,unschedulable-pod,sidecar-crash-pod}"

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check dependencies
if ! command -v tmux &> /dev/null; then
    echo "Error: tmux is required but not installed"
    exit 1
fi

# Terminal size for screenshots (80x24 is classic, but modern TUIs need more space)
TERM_WIDTH=120
TERM_HEIGHT=40

# Wait time for UI to render and data to load
WAIT_RENDER=2
WAIT_DATA=5

echo "Starting cluster-cli e2e screenshot capture..."
echo "Binary: $CLUSTER_BIN"
echo "Output: $OUTPUT_DIR"
echo "Views: $VIEW_LIST"
echo "Terminal size: ${TERM_WIDTH}x${TERM_HEIGHT}"

# Kill any existing tmux session
tmux kill-session -t cluster-e2e 2>/dev/null || true

# Create a new tmux session with fixed size
tmux new-session -d -s cluster-e2e -x "$TERM_WIDTH" -y "$TERM_HEIGHT"

# Fail instead of capturing the shell if cluster-cli has exited
assert_running() {
    local running
    # Reported as the basename on macOS but as the full argv[0] on Linux
    running=$(tmux display-message -p -t cluster-e2e '#{pane_current_command}')
    if [ "$(basename -- "$running")" != "$(basename -- "$CLUSTER_BIN")" ]; then
        echo "Error: cluster-cli is not running in the tmux pane (found: $running)"
        tmux capture-pane -p -t cluster-e2e
        tmux kill-session -t cluster-e2e 2>/dev/null || true
        exit 1
    fi
}

# Function to capture screenshot
capture_screenshot() {
    local name="$1"
    local output_file="$OUTPUT_DIR/${name}.txt"
    
    echo "Capturing: $name"
    
    # Wait for render
    sleep "$WAIT_RENDER"
    assert_running
    
    # Capture pane content with ANSI codes
    tmux capture-pane -e -p -t cluster-e2e > "$output_file"
    
    echo "  -> Saved to $output_file"
}

# Function to send keys and wait
send_keys() {
    local keys="$1"
    echo "  Sending keys: $keys"
    tmux send-keys -t cluster-e2e "$keys"
}

# Start cluster-cli in the tmux session
echo "Launching cluster-cli..."
tmux send-keys -t cluster-e2e "$CLUSTER_BIN" C-m

# Wait for initial data load
echo "Waiting for data to load..."
sleep "$WAIT_DATA"
assert_running

# Parse view list and capture each view
IFS=',' read -ra VIEWS <<< "$VIEW_LIST"

for view in "${VIEWS[@]}"; do
    case "$view" in
        dashboard)
            echo "Capturing dashboard view..."
            capture_screenshot "01-dashboard"
            ;;
            
        # Panels are focused directly with 1 (nodes), 2 (events), 3 (pods).
        # Views are left with Escape, because 'q' on the dashboard quits.
        pod-detail)
            echo "Navigating to pod detail view..."
            send_keys "3"
            sleep 1
            # Press Enter on the currently selected pod
            send_keys "C-m"
            sleep "$WAIT_RENDER"
            capture_screenshot "02-pod-detail"
            send_keys "Escape"
            sleep 1
            ;;
            
        # Opens each pod in DIAGNOSIS_PODS through the pod filter, so the
        # Diagnosis panel is captured for each failure type regardless of the
        # pod table's order.
        diagnosis)
            echo "Capturing pod diagnosis for: $DIAGNOSIS_PODS"
            IFS=',' read -ra DIAG_PODS <<< "$DIAGNOSIS_PODS"
            for pod in "${DIAG_PODS[@]}"; do
                send_keys "/"
                tmux send-keys -t cluster-e2e -l "$pod"
                send_keys "C-m"
                send_keys "3"
                sleep 1
                send_keys "C-m"
                sleep "$WAIT_RENDER"
                capture_screenshot "02-diagnosis-$pod"
                send_keys "Escape"
                sleep 1
                # Clear the filter for the next view
                send_keys "/"
                for ((i = 0; i < ${#pod}; i++)); do
                    send_keys "BSpace"
                done
                send_keys "C-m"
                sleep 1
            done
            ;;

        node-detail)
            echo "Navigating to node detail view..."
            send_keys "1"
            sleep 1
            # Press Enter on the selected node
            send_keys "C-m"
            sleep "$WAIT_RENDER"
            capture_screenshot "03-node-detail"
            send_keys "Escape"
            sleep 1
            send_keys "3"
            sleep 1
            ;;
            
        incidents)
            echo "Focusing incidents/events panel..."
            send_keys "2"
            sleep "$WAIT_RENDER"
            capture_screenshot "04-incidents"
            send_keys "3"
            sleep 1
            ;;
            
        timeline)
            echo "Opening timeline view..."
            send_keys "t"
            sleep "$WAIT_RENDER"
            capture_screenshot "05-timeline"
            send_keys "Escape"
            sleep 1
            ;;
            
        *)
            echo "Warning: Unknown view '$view', skipping..."
            ;;
    esac
done

# Quit cluster-cli
echo "Exiting cluster-cli..."
send_keys "q"
sleep 1

# Kill tmux session
tmux kill-session -t cluster-e2e 2>/dev/null || true

echo ""
echo "Screenshot capture complete!"
echo "Output files in: $OUTPUT_DIR"
ls -lh "$OUTPUT_DIR"/*.txt 2>/dev/null || echo "  (no screenshots captured)"

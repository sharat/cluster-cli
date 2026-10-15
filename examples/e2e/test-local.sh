#!/usr/bin/env bash
# Local test script to verify E2E testing setup
# This script tests the screenshot capture and conversion without requiring a kind cluster

set -euo pipefail

echo "=== Local E2E Test Verification ==="
echo ""

# Check dependencies
echo "Checking dependencies..."
HAS_TMUX=false
HAS_CARGO=false

if command -v tmux &> /dev/null; then
    HAS_TMUX=true
    echo "✓ tmux found: $(tmux -V)"
else
    echo "✗ tmux not found"
fi

if command -v cargo &> /dev/null; then
    HAS_CARGO=true
    echo "✓ cargo found: $(cargo --version)"
else
    echo "✗ cargo not found"
fi

echo ""

# Check conversion tools
echo "Checking conversion tools..."
if command -v node &> /dev/null; then
    echo "✓ node found: $(node --version)"
fi

if command -v rsvg-convert &> /dev/null; then
    echo "✓ rsvg-convert found: $(rsvg-convert --version)"
fi

if command -v termshot &> /dev/null; then
    echo "✓ termshot found"
fi

echo ""

# Create test ANSI file
TEST_DIR="./test-output"
mkdir -p "$TEST_DIR"

echo "Creating test ANSI file..."
printf '%b\n' \
    '\033[1;31mRed Text\033[0m' \
    '\033[1;32mGreen Text\033[0m' \
    '\033[1;34mBlue Text\033[0m' \
    '\033[1;33mYellow Text\033[0m' \
    '' \
    'This is a test ANSI file with colors and formatting.' \
    '\033[7mInverted\033[0m \033[4mUnderline\033[0m \033[1mBold\033[0m' \
    > "$TEST_DIR/test.txt"

echo "✓ Test ANSI file created at $TEST_DIR/test.txt"
echo ""

# Test conversion
echo "Testing conversion script..."
if [ -f "./examples/e2e/convert-screenshots.sh" ]; then
    ./examples/e2e/convert-screenshots.sh "$TEST_DIR" "$TEST_DIR" || true
    
    if [ -f "$TEST_DIR/test.png" ]; then
        echo "✓ Conversion successful! PNG created at $TEST_DIR/test.png"
        ls -lh "$TEST_DIR/test.png"
    else
        echo "✗ Conversion failed - no PNG file generated"
        echo "  This is expected if conversion tools are not installed"
    fi
else
    echo "✗ convert-screenshots.sh not found"
fi

echo ""
echo "=== Test Complete ==="
echo ""
echo "To install conversion tools:"
echo "  Option 1: cargo install termshot"
echo "  Option 2: npm install -g ansi-to-svg && brew/apt install librsvg2-bin"

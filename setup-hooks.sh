#!/bin/bash
# Setup script to configure git hooks

echo "Setting up git hooks..."

# Configure git to use the .githooks directory
git config core.hooksPath .githooks

echo "✅ Git hooks configured!"
echo ""
echo "Pre-commit hook will now:"
echo "  1. Check code formatting (cargo fmt)"
echo "  2. Run clippy (cargo clippy -- -D warnings)"
echo "  3. Run tests (cargo test)"
echo ""
echo "To bypass the hook in an emergency, use: git commit --no-verify"

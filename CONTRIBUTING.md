# Contributing to cluster-rs

Thank you for your interest in contributing to cluster-rs! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [How Can I Contribute?](#how-can-i-contribute)
- [Development Setup](#development-setup)
- [Coding Standards](#coding-standards)
- [Pull Request Process](#pull-request-process)

## Code of Conduct

This project and everyone participating in it is expected to:
- Be respectful and inclusive
- Welcome newcomers and help them learn
- Focus on constructive feedback
- Respect different viewpoints and experiences

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally
3. Create a new branch for your contribution
4. Make your changes
5. Submit a pull request

## How Can I Contribute?

### Reporting Bugs

Before creating a bug report, please:
- Check if the issue already exists
- Try to isolate the problem
- Collect relevant information (OS, Kubernetes version, kubectl version)

When reporting bugs, include:
- Clear description of the problem
- Steps to reproduce
- Expected vs actual behavior
- Environment details (OS, Rust version, kubectl version)
- Any error messages or logs

### Suggesting Features

Feature suggestions are welcome! Please:
- Check if the feature has already been suggested
- Explain the use case and why it would be useful
- Provide examples of how it would work
- Consider if it fits the project's scope (read-only monitoring tool)

### Pull Requests

- Fill out the pull request template
- Reference any related issues
- Ensure tests pass
- Update documentation if needed
- Keep changes focused and atomic

## Development Setup

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- `kubectl` configured with cluster access
- Git

### Building

```bash
# Clone the repository
git clone https://github.com/yourusername/cluster-rs.git
cd cluster-rs

# Build in debug mode
cargo build

# Build in release mode
cargo build --release
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

### Code Quality

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy

# Run clippy with all features
cargo clippy --all-features
```

## Coding Standards

### Rust Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` to format code
- Address all `cargo clippy` warnings
- Write descriptive variable and function names
- Add documentation comments for public APIs

### Testing

- Write tests for new functionality
- Ensure existing tests pass
- Aim for good test coverage on critical paths
- Test edge cases and error conditions

### Documentation

- Update README.md if adding new features
- Add inline comments for complex logic
- Update help text for CLI changes
- Keep documentation concise and clear

### Security

- Never introduce write operations to Kubernetes
- Validate all kubectl commands are read-only
- Don't log sensitive information
- Follow security best practices

## Pull Request Process

1. **Create a branch**: `git checkout -b feature/my-feature` or `git checkout -b fix/my-bugfix`

2. **Make your changes**: Write code, add tests, update docs

3. **Test locally**:
   ```bash
   cargo test
   cargo fmt --check
   cargo clippy
   ```

4. **Commit your changes**:
   ```bash
   git add .
   git commit -m "feat: add new feature"
   ```
   
   Follow conventional commit format:
   - `feat:` New feature
   - `fix:` Bug fix
   - `docs:` Documentation changes
   - `test:` Adding tests
   - `refactor:` Code refactoring
   - `perf:` Performance improvements
   - `chore:` Maintenance tasks

5. **Push to your fork**:
   ```bash
   git push origin feature/my-feature
   ```

6. **Open a Pull Request**:
   - Go to the original repository on GitHub
   - Click "New Pull Request"
   - Select your branch
   - Fill out the PR template
   - Link any related issues

7. **Review Process**:
   - Maintainers will review your PR
   - Address any requested changes
   - Once approved, your PR will be merged

## Questions?

Feel free to:
- Open an issue for questions
- Join discussions in existing issues
- Reach out to maintainers

Thank you for contributing to cluster-rs!

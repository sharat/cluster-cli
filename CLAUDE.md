# CLAUDE.md

This file provides guidance for Claude Code (claude.ai/code) when working with code in this repository.

For comprehensive instructions including build commands, testing, changelog maintenance, architecture, and CI/CD, see [`@AGENTS.md`](./AGENTS.md).

## Quick Reference

### Build & Test
```bash
cargo build --release  # optimized build
cargo test --verbose   # run all tests
cargo fmt -- --check   # check formatting
cargo clippy -- -D warnings # lint
```

### Documentation
When features, key bindings, CLI flags, or MCP tools change, use the **`update-docs` skill** at `.agents/skills/update-docs/SKILL.md` to regenerate documentation.

See the [Documentation section in @AGENTS.md](./AGENTS.md#documentation) for details.

### Shared Skills
Repository skills are in `.agents/skills/`. Load the relevant `SKILL.md` before performing:
- Documentation updates: `.agents/skills/update-docs/SKILL.md`
- Release preparation: `.agents/skills/release-cluster-cli/SKILL.md`

For all other guidance, including project architecture, changelog maintenance, pull request practices, and CI/CD workflows, see [`@AGENTS.md`](./AGENTS.md).

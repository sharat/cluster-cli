# GEMINI.md

This file provides guidance for Gemini CLI when working with code in this repository.

## Primary Reference

See [`AGENTS.md`](./AGENTS.md) for comprehensive instructions including:
- Build, test, lint, and format commands
- Shared agent skills (documentation updates, releases)
- Changelog maintenance guidelines
- Pull request best practices
- Project architecture and data flow
- CI/CD workflows

## Documentation Updates

When features, key bindings, CLI flags, or MCP tools change:

1. See the [Documentation section in AGENTS.md](./AGENTS.md#documentation)
2. Use the `update-docs` skill at `.agents/skills/update-docs/SKILL.md`

## Configuration

For Gemini CLI to load AGENTS.md automatically, add to `.gemini/settings.json`:

```json
{
  "contextFileName": "AGENTS.md"
}
```

Or reference it explicitly in your prompts: `@AGENTS.md`

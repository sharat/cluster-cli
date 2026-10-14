---
name: update-docs
description: Update cluster-cli documentation and landing page when features, key bindings, CLI flags, or MCP tools change. Ensures docs stay synchronized with code.
---

# Update Documentation Skill

Use this skill when cluster-cli features, key bindings, CLI flags, or MCP tools have changed and the documentation needs to be regenerated.

## When to Use

- New features added (timeline view, workload popup, export, etc.)
- Key bindings changed or added (dashboard, pod detail, node detail, timeline)
- CLI flags or subcommands modified (status, incidents, pods, nodes, logs, mcp)
- MCP tools added, removed, or changed (get_cluster_health, list_pods, etc.)
- Health scoring algorithm updated
- Configuration options changed

## Steps

### 1. Read the Code as Source of Truth

Before updating any documentation, read the actual implementation:

- **Key bindings**: `src/events/handler.rs` - all keyboard shortcuts
- **CLI interface**: `src/cli.rs` - all flags, subcommands, aliases
- **MCP tools**: `src/mcp.rs` - tool names, parameters, descriptions
- **Health scoring**: `src/data/health.rs` - penalties, thresholds, constants
- **Timeline**: `src/data/timeline.rs` and `src/ui/views/timeline.rs`
- **Configuration**: `src/config.rs` - all config fields
- **JSON schema**: `src/json_output.rs` - headless API structure

**Critical**: Never document features that don't exist in the code. Verify every key binding, flag, and tool.

### 2. Update Documentation Files

Update each affected file in `docs/`:

- **`overview.md`**: High-level features, architecture, troubleshooting
- **`installation.md`**: Installation methods, platforms, prerequisites
- **`quick-start.md`**: 5-minute getting started guide
- **`ui.md`**: Complete key binding reference for all views
- **`health.md`**: Health score calculation, incident system, thresholds
- **`configuration.md`**: Config file, CLI args, environment variables
- **`headless.md`**: JSON API for all subcommands
- **`mcp.md`**: MCP server setup, tools, examples for all AI clients
- **`timeline.md`**: Timeline features, entry types, precursor detection
- **`README.md`**: Documentation index with links

### 3. Regenerate the Landing Page

Update `docs/index.html` using the structure from `landing-page-template.md`:

**Requirements**:
- Self-contained HTML (inline CSS and JS)
- Fonts: Geist Sans (text) and Geist Mono (code) from Google Fonts
- Minimal, solid color palette: no gradients; teal (`#0f766e`) accent on white/slate neutrals
- Dark mode: respect `prefers-color-scheme`
- Reduced motion: respect `prefers-reduced-motion`
- WCAG AA contrast requirements
- Sections in order: Hero, Features, Terminal Demo, Install, Key Bindings, Headless, **MCP (last)**
- Dynamic features: animated terminal mock, copy buttons, sticky nav, smooth scroll
- Semantic HTML with ARIA labels
- Responsive design (mobile-first)
- Open Graph and Twitter meta tags
- Inline SVG favicon (Kubernetes helm `⎈`)
- Canonical URL: `https://sharat.github.io/cluster-cli/`
- Relative asset paths for GitHub Pages

**Sections to Update**:
- Features grid: Reflect current feature set
- Key bindings: Match `ui.md` exactly
- Headless examples: Match `headless.md` commands
- MCP section: Keep at the end, include all AI clients (Cursor, Claude Desktop, VS Code)

### 4. Verify Documentation Accuracy

Run this checklist:

- [ ] Every documented key binding exists in `src/events/handler.rs`
- [ ] Every documented CLI flag exists in `src/cli.rs`
- [ ] Every documented MCP tool exists in `src/mcp.rs`
- [ ] Health score thresholds match `src/data/models.rs` constants
- [ ] JSON schema version matches `src/json_output.rs::JSON_SCHEMA_VERSION`
- [ ] Config options match `src/config.rs` struct fields
- [ ] Timeline entry types match `src/data/timeline.rs::TimelineEntryKind`
- [ ] No invented features, flags, or keys

### 5. Render a Screenshot for PR

Use headless Chromium or Playwright to render `docs/index.html` and capture a screenshot:

```bash
# Option 1: Playwright (if available)
npx playwright screenshot docs/index.html landing-page-screenshot.png --viewport-size=1200,800

# Option 2: Headless Chromium
google-chrome --headless --screenshot=landing-page-screenshot.png --window-size=1200,800 docs/index.html

# Option 3: Firefox headless
firefox --headless --screenshot landing-page-screenshot.png docs/index.html
```

Include the screenshot in the PR body or commit it to the repository at `docs/images/landing-page.png`.

### 6. Update Links

Ensure these files link to the documentation:

- **`README.md`**: Link to https://sharat.github.io/cluster-cli/ in the header or features section
- **`AGENTS.md`**: Link to the hosted docs in the Documentation section

## Checklist

Complete this checklist for every documentation update:

- [ ] Read all relevant source files (handlers, CLI, MCP, health, etc.)
- [ ] Update `docs/overview.md` with architectural changes
- [ ] Update `docs/ui.md` with new/changed key bindings
- [ ] Update `docs/health.md` with scoring changes
- [ ] Update `docs/headless.md` with CLI subcommand changes
- [ ] Update `docs/mcp.md` with MCP tool changes
- [ ] Update `docs/timeline.md` if timeline features changed
- [ ] Update other affected docs files
- [ ] Regenerate `docs/index.html` landing page
- [ ] Verify all documented features exist in code
- [ ] Render landing page screenshot
- [ ] Include screenshot in PR
- [ ] Update README and AGENTS.md links
- [ ] Check that `.nojekyll` file exists
- [ ] Verify GitHub Pages workflow is present

## Template Structure

See `landing-page-template.md` for the canonical structure and style tokens for the landing page.

## Tool Integration

This skill is discoverable by:

- **Claude Code**: via `@AGENTS.md` and `.claude/skills/update-docs/SKILL.md` symlink
- **Cursor**: via `.cursor/rules/docs.mdc` pointer
- **Gemini CLI**: via `GEMINI.md` pointer
- **Codex**: via `AGENTS.md` (reads natively)
- **Any MCP client**: via `AGENTS.md` convention

## Notes

- **Single source of truth**: This skill file. All tool pointers reference this location.
- **No duplication**: Use symlinks or one-line references, never full copies.
- **Code is the truth**: Always verify features exist before documenting.
- **PR screenshots**: Provide visual proof that the landing page renders correctly.
- **GitHub Pages**: Docs are automatically deployed from `main` branch via workflow.

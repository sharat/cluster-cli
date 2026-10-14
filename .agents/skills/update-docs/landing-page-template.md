# Landing Page Template

Canonical structure and style tokens for `docs/index.html`.

## Structure

```
<!DOCTYPE html>
<html>
<head>
  - Meta tags (charset, viewport, description, keywords)
  - Canonical URL: https://sharat.github.io/cluster-cli/
  - Open Graph tags (og:type, og:url, og:title, og:description, og:image)
  - Twitter Card tags
  - Inline SVG favicon (⎈)
  - Geist Sans & Geist Mono fonts from Google Fonts
  - <style> block (inline CSS)
</head>
<body>
  <header>
    <nav> logo + links (Features, Install, Keys, Headless, AI Agents, GitHub)
  </header>
  
  <main>
    <section class="hero">
      - Title: cluster-cli
      - Tagline
      - Install command with copy button
      - CTA buttons (Download, Documentation)
    </section>
    
    <section id="features">
      - Features grid (6 cards)
      - Icons + descriptions
    </section>
    
    <section id="terminal-demo">
      - Animated terminal mock
      - Typing effect with commands
    </section>
    
    <section id="install">
      - Quick install
      - Build from source
      - Code blocks with copy buttons
    </section>
    
    <section id="keys">
      - Navigation keys
      - Action keys
      - Pod detail keys
      - Timeline keys
      - Grid layout with key badges
    </section>
    
    <section id="headless">
      - JSON API examples
      - status, incidents, pods, nodes, logs
      - CI/CD example
    </section>
    
    <section id="mcp" class="mcp-section">
      - MCP server introduction
      - Cursor config
      - Claude Desktop config
      - VS Code config
      - Available tools list
      - Example queries
    </section>
  </main>
  
  <footer>
    - License
    - Links (GitHub, Docs, Releases, Issues)
  </footer>
  
  <script> inline JavaScript for animations and copy buttons
</body>
</html>
```

## Color Tokens

### Light Mode (Default)
```css
--bg-primary: #ffffff;
--bg-secondary: #f8fafc;    /* slate-50 */
--bg-tertiary: #f1f5f9;     /* slate-100 */
--text-primary: #0f172a;    /* slate-900 */
--text-secondary: #475569;  /* slate-600 */
--text-muted: #64748b;      /* slate-500 */
--border: #e2e8f0;          /* slate-200 */
--accent: #1d4ed8;          /* blue-700 */
--accent-text: #ffffff;
--code-bg: #1e293b;         /* slate-800 */
--code-text: #f1f5f9;       /* slate-100 */
```

### Dark Mode (@media prefers-color-scheme: dark)
```css
--bg-primary: #0f172a;      /* slate-900 */
--bg-secondary: #1e293b;    /* slate-800 */
--bg-tertiary: #334155;     /* slate-700 */
--text-primary: #f1f5f9;    /* slate-100 */
--text-secondary: #cbd5e1;  /* slate-300 */
--text-muted: #94a3b8;      /* slate-400 */
--border: #334155;          /* slate-700 */
--accent: #93c5fd;          /* blue-300 */
--accent-text: #0f172a;
--code-bg: #0f172a;         /* slate-900 */
--code-text: #e2e8f0;       /* slate-200 */
```

## Typography

- **Body**: `font-family: 'Geist', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;`
- **Code**: `font-family: 'Geist Mono', 'Monaco', 'Courier New', monospace;`
- **Heading sizes**: h1: 3.5rem, h2: 2.5rem, h3: 1.5rem
- **Line height**: 1.6

## Accessibility

- Semantic HTML: `<header>`, `<main>`, `<section>`, `<footer>`
- ARIA labels on navigation and interactive controls
- Skip link targeting the main content
- Focus styles: `outline: 2px solid var(--accent); outline-offset: 2px;`
- Keyboard navigation: Tab, Enter, Esc
- Color + text indicators (not color-only)
- WCAG AA text and control contrast in light and dark modes
- `.sr-only` class for screen reader text
- `alt` text on all images (if any)

## Animations

### Fade In Up
```css
@keyframes fadeInUp {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
```

### Blink (Terminal Cursor)
```css
@keyframes blink {
  50% { opacity: 0; }
}
```

### Reduced Motion
```css
@media (prefers-reduced-motion: reduce) {
  * {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
```

## Interactive Elements

### Copy Buttons
- Position: absolute top-right of code blocks
- State: default → hover → copied (accent)
- Feedback: "Copy" → "Copied!" for 2 seconds

### Terminal Animation
- Commands array with typed output
- Typing speed: 50ms per character
- Output delay: 100ms per line
- Cycle through commands with 2s pause

### Smooth Scroll
- `scroll-behavior: smooth` on html
- Offset for sticky header (80px)

## Responsive Breakpoints

```css
@media (max-width: 768px) {
  .hero h1 { font-size: 2.5rem; }
  .nav-links { flex-direction: column; }
  .features-grid { grid-template-columns: 1fr; }
  .keybindings-grid { grid-template-columns: 1fr; }
}
```

## Section Order (Critical)

1. Hero (plain background with border)
2. Features
3. Terminal Demo
4. Install
5. Key Bindings
6. Headless
7. **MCP (LAST)** ← Always at the end

## Meta Tags Template

```html
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>cluster-cli - Kubernetes Cluster Health Monitoring TUI</title>
<meta name="description" content="A fast, interactive terminal UI for monitoring Kubernetes cluster health in real-time. Built in Rust with ratatui.">
<link rel="canonical" href="https://sharat.github.io/cluster-cli/">

<!-- Open Graph -->
<meta property="og:type" content="website">
<meta property="og:url" content="https://sharat.github.io/cluster-cli/">
<meta property="og:title" content="cluster-cli - Kubernetes Cluster Health Monitoring TUI">
<meta property="og:description" content="A fast, interactive terminal UI for monitoring Kubernetes cluster health in real-time.">
<meta property="og:image" content="https://sharat.github.io/cluster-cli/images/screenshot.png">

<!-- Twitter -->
<meta property="twitter:card" content="summary_large_image">
<meta property="twitter:url" content="https://sharat.github.io/cluster-cli/">
<meta property="twitter:title" content="cluster-cli - Kubernetes Cluster Health Monitoring TUI">
<meta property="twitter:description" content="A fast, interactive terminal UI for monitoring Kubernetes cluster health in real-time.">
<meta property="twitter:image" content="https://sharat.github.io/cluster-cli/images/screenshot.png">

<!-- Favicon -->
<link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='0.9em' font-size='90'>⎈</text></svg>">
```

## Contrast Requirements

All text must meet WCAG AA contrast ratios:
- **Normal text**: 4.5:1 minimum
- **Large text** (≥18pt or ≥14pt bold): 3:1 minimum
- **Interactive elements**: 3:1 minimum

Test with: https://webaim.org/resources/contrastchecker/

## Notes

- **Self-contained**: All CSS and JS must be inline
- **External fonts only**: Google Fonts CDN is allowed
- **No other external dependencies**: No jQuery, Bootstrap, etc.
- **Relative paths**: All asset links use relative paths (e.g., `./overview.html`, not `/overview.html`)
- **Single file**: Entire landing page in one HTML file
- **GitHub Pages compatible**: Works under `/cluster-cli/` path

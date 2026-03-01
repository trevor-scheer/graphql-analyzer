# VS Code Extension - Claude Guide

Architecture and guidance for the VS Code extension.

---

## Extension Architecture

The extension has three separate systems - don't confuse them:

| System             | Purpose                                     | Scope                                            |
| ------------------ | ------------------------------------------- | ------------------------------------------------ |
| `documentSelector` | LSP features (diagnostics, hover, goto def) | Controls which files get IDE features            |
| Grammar injection  | Syntax highlighting only                    | Visual coloring, no semantic understanding       |
| File watcher       | Disk events only                            | File create/delete/rename, NOT real-time editing |

**Common mistake:** Thinking grammar injection provides embedded GraphQL support. It only provides colors. The `documentSelector` MUST include TS/JS for actual LSP features.

---

## Protected Features

- **NEVER** remove TS/JS from `documentSelector` - most users write queries in TS/JS files
- **NEVER** solve performance problems by removing language support from the selector

---

## Key Files

| File                  | Purpose                      |
| --------------------- | ---------------------------- |
| `src/extension.ts`    | Extension entry point        |
| `src/binaryManager.ts`| LSP binary lifecycle         |
| `syntaxes/`           | TextMate grammars            |
| `package.json`        | Extension manifest           |
| `e2e/`                | Playwright end-to-end tests  |

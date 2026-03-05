---
description: VS Code extension development rules
paths:
  - "editors/vscode/**"
---

# VS Code Extension Rules

- NEVER remove TypeScript or JavaScript from the `documentSelector` - most users write GraphQL in TS/JS files
- Test extension changes with the Playwright e2e test suite
- Keep activation events minimal - lazy load where possible
- All user-facing strings must be localizable

# Changesets

This directory contains changeset files that document changes to packages in this workspace.

## What is a Changeset?

A changeset is a Markdown file describing a change to one or more packages. Each changeset specifies:
- Which packages are affected
- The type of version bump (major, minor, patch)
- A summary of the change for the changelog

## Creating a Changeset

Run the following command to create a new changeset:

```bash
knope document-change
```

This will prompt you to:
1. Select which packages are affected
2. Choose the version bump type for each package
3. Write a summary of the change

Alternatively, create a changeset manually:

```markdown
---
graphql-lsp: minor
graphql-cli: minor
---

Add new feature X that improves Y
```

## When to Create a Changeset

Create a changeset when making changes that should be documented in the changelog:

- **major**: Breaking changes (API changes, removed features)
- **minor**: New features, significant improvements
- **patch**: Bug fixes, documentation updates, internal changes

Not every commit needs a changeset. Skip changesets for:
- Internal refactoring that doesn't change behavior
- CI/CD changes
- Documentation-only changes (unless significant)

## Releasing

When ready to release, run:

```bash
knope release --dry-run  # Preview what will happen
knope release            # Execute the release
```

This will:
1. Combine all changesets into changelog entries
2. Bump package versions appropriately
3. Create git tags
4. (Optionally) Create GitHub releases

## More Information

- [Knope Documentation](https://knope.tech)
- [Changesets Concept](https://knope.tech/reference/concepts/changeset/)

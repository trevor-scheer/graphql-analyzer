#!/usr/bin/env node
// Syncs intra-workspace npm dependency refs to match the current workspace
// version of each referenced package. Knope only updates the `version` field
// of `versioned_files`, not dependency strings, so without this consumers of a
// published package would pick up a stale (or non-existent) sibling version.
//
// Auto-discovers workspace packages by reading `pnpm-workspace.yaml` (with
// glob support) so adding a new package doesn't require updating this script.
//
// Usage:
//   node scripts/sync-workspace-deps.mjs           # rewrite files in place
//   node scripts/sync-workspace-deps.mjs --check   # exit non-zero on drift
//
// `--check` is the CI mode — it asserts the committed state is already in
// sync (every intra-workspace dep ref equals the referenced package's current
// version). After knope bumps versions, the release-prep workflow runs the
// in-place mode to fix up dep refs before regenerating the lock file.

import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { readPnpmWorkspaces } from "./lib/pnpm-workspaces.mjs";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DEP_SECTIONS = [
  "dependencies",
  "devDependencies",
  "peerDependencies",
  "optionalDependencies",
];

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function writeJson(path, data) {
  writeFileSync(path, JSON.stringify(data, null, 2) + "\n");
}

function expandWorkspaceGlob(pattern) {
  // Supports the two forms used in this repo: literal paths ("editors/vscode")
  // and one-level globs ("packages/*"). Avoid pulling in a glob library for
  // such a tiny surface.
  if (!pattern.includes("*")) return [pattern];
  if (!pattern.endsWith("/*")) {
    throw new Error(`Unsupported workspace glob: ${pattern}`);
  }
  const parent = pattern.slice(0, -2);
  const parentAbs = join(REPO_ROOT, parent);
  return readdirSync(parentAbs)
    .filter((name) => statSync(join(parentAbs, name)).isDirectory())
    .map((name) => `${parent}/${name}`);
}

function collectWorkspacePackages() {
  const dirs = readPnpmWorkspaces().flatMap(expandWorkspaceGlob);

  // Workspace packages can themselves contain nested package.json files (e.g.
  // packages/core/npm/<platform>/package.json). Knope versions these, so we
  // need to discover them too.
  const queue = [...dirs];
  const seen = new Set();
  const packages = [];

  while (queue.length > 0) {
    const dir = queue.shift();
    if (seen.has(dir)) continue;
    seen.add(dir);

    const pkgPath = join(REPO_ROOT, dir, "package.json");
    let pkg;
    try {
      pkg = readJson(pkgPath);
    } catch {
      continue;
    }
    if (!pkg.name) continue;

    packages.push({ dir, path: pkgPath, pkg });

    // Walk one level of subdirectories looking for further package.json files
    // (covers `packages/core/npm/<platform>`). Skip node_modules.
    const abs = join(REPO_ROOT, dir);
    let entries;
    try {
      entries = readdirSync(abs, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (!entry.isDirectory() || entry.name === "node_modules" || entry.name.startsWith("."))
        continue;
      queue.push(join(dir, entry.name));
      // Two levels deep also (covers nested napi platform stubs).
      const sub = join(abs, entry.name);
      let subEntries;
      try {
        subEntries = readdirSync(sub, { withFileTypes: true });
      } catch {
        continue;
      }
      for (const subEntry of subEntries) {
        if (
          !subEntry.isDirectory() ||
          subEntry.name === "node_modules" ||
          subEntry.name.startsWith(".")
        )
          continue;
        queue.push(join(dir, entry.name, subEntry.name));
      }
    }
  }

  return packages;
}

// Only rewrite exact-version pins. Wildcards (`*`), ranges (`^1.0.0`, `~1.0.0`,
// `>=1.0.0`), and protocol refs (`workspace:*`, `file:`, `link:`) are
// intentional in test fixtures and dev-only deps — leave them alone.
const EXACT_VERSION_RE = /^\d+\.\d+\.\d+(?:-[\w.+-]+)?$/;

function syncPackages(packages, { write }) {
  const versionByName = new Map(packages.map((p) => [p.pkg.name, p.pkg.version]));
  const changes = [];

  for (const { path, pkg } of packages) {
    for (const section of DEP_SECTIONS) {
      const deps = pkg[section];
      if (!deps) continue;
      for (const [name, current] of Object.entries(deps)) {
        const target = versionByName.get(name);
        if (target === undefined) continue; // not an intra-workspace dep
        if (!EXACT_VERSION_RE.test(current)) continue; // wildcards/ranges intentional
        if (current === target) continue;
        deps[name] = target;
        changes.push({ path, section, name, from: current, to: target });
      }
    }
    // Always re-write publishable packages (even when no dep refs changed) so
    // the on-disk formatting is canonical. Knope's serializer omits the
    // trailing newline that the project's formatter (oxfmt) requires; this
    // guarantees `oxfmt --check` passes on the release PR. Private packages
    // (test fixtures, root) aren't in knope's `versioned_files`, so they're
    // left alone.
    if (write && !pkg.private) writeJson(path, pkg);
  }

  return changes;
}

const checkMode = process.argv.includes("--check");
const packages = collectWorkspacePackages();
const changes = syncPackages(packages, { write: !checkMode });

if (changes.length === 0) {
  if (!checkMode) console.log("Workspace dep refs already in sync.");
  process.exit(0);
}

for (const c of changes) {
  console.log(`${c.path} ${c.section}["${c.name}"]: ${c.from} -> ${c.to}`);
}

if (checkMode) {
  console.error(
    "\nError: intra-workspace dep refs are out of sync. " +
      "Run `node scripts/sync-workspace-deps.mjs` and commit the changes.",
  );
  process.exit(1);
}

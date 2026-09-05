#!/usr/bin/env node
// Simulates knope's release-prep transformations and verifies the result still
// works. Run on every PR so we catch drift between what's tested on main and
// what knope would produce on release — without this, those transformations
// (version bumps + dep-ref rewrites + lockfile regen) are first exercised in
// the auto-generated release PR, making any breakage block the release.
//
// Steps:
//   1. Bump every npm workspace package's `version` to a synthetic value
//      (mimics what knope writes to its `versioned_files`).
//   2. Run `sync-workspace-deps.mjs` to re-pin intra-workspace dep refs.
//   3. Regenerate `pnpm-lock.yaml` via `pnpm install --lockfile-only`.
//   4. Run `pnpm install --frozen-lockfile` against it to confirm it resolves.
//
// CI runs this in a throwaway checkout, so we don't bother restoring state.
// For local runs, use a clean working tree (the script will refuse otherwise).

import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

import { readPnpmWorkspaces } from "./lib/pnpm-workspaces.mjs";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SYNTHETIC_VERSION = "999.0.0-ci-release-prep";

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function writeJson(path, data) {
  writeFileSync(path, JSON.stringify(data, null, 2) + "\n");
}

function expandWorkspaceGlob(pattern) {
  if (!pattern.includes("*")) return [pattern];
  if (!pattern.endsWith("/*")) throw new Error(`Unsupported glob: ${pattern}`);
  const parent = pattern.slice(0, -2);
  const parentAbs = join(REPO_ROOT, parent);
  return readdirSync(parentAbs)
    .filter((name) => statSync(join(parentAbs, name)).isDirectory())
    .map((name) => `${parent}/${name}`);
}

// Mirrors the discovery logic in sync-workspace-deps.mjs so we bump exactly
// the same set of files the sync script considers.
function collectWorkspacePackages() {
  const dirs = readPnpmWorkspaces().flatMap(expandWorkspaceGlob);
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

function ensureCleanTree() {
  if (process.env.CI) return; // throwaway checkout
  const out = execFileSync("git", ["status", "--porcelain"], { cwd: REPO_ROOT, encoding: "utf8" });
  if (out.trim().length > 0) {
    console.error("Refusing to run: working tree has uncommitted changes.");
    console.error("This script mutates package.json and pnpm-lock.yaml. Stash or commit first.");
    process.exit(2);
  }
}

function run(cmd, args) {
  console.log(`\n$ ${cmd} ${args.join(" ")}`);
  execFileSync(cmd, args, { cwd: REPO_ROOT, stdio: "inherit" });
}

ensureCleanTree();

const packages = collectWorkspacePackages();

// Only bump packages that are publishable (i.e. not `private: true` test
// fixtures). Knope's versioned_files only ever cover publishable packages.
const bumped = packages.filter((p) => !p.pkg.private);
console.log(`Bumping ${bumped.length} packages to ${SYNTHETIC_VERSION}:`);
for (const p of bumped) {
  console.log(`  ${p.dir} (${p.pkg.name})`);
  p.pkg.version = SYNTHETIC_VERSION;
  writeJson(p.path, p.pkg);
}

run("node", ["scripts/sync-workspace-deps.mjs"]);
run("pnpm", ["install", "--lockfile-only"]);
run("pnpm", ["install", "--frozen-lockfile"]);
// Catch any formatting drift introduced by the bump+sync round-trip — knope's
// JSON writer omits the trailing newline that oxfmt requires, and the sync
// script is responsible for normalizing that.
run("pnpm", ["exec", "oxfmt", "--check", "packages/", "editors/"]);

console.log("\nRelease-prep simulation passed.");

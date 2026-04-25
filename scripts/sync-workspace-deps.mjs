#!/usr/bin/env node
// Syncs intra-workspace npm dependency refs to match the current workspace
// version. Knope only updates the `version` field of `versioned_files`, not
// dependency strings, so without this consumers of a published package would
// pick up a stale (or non-existent) sibling version.

import { readFileSync, writeFileSync } from "node:fs";

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function writeJson(path, data) {
  writeFileSync(path, JSON.stringify(data, null, 2) + "\n");
}

function setDep(pkg, section, name, version) {
  const deps = pkg[section];
  if (!deps || !(name in deps)) return false;
  if (deps[name] === version) return false;
  deps[name] = version;
  return true;
}

const corePkgPath = "packages/core/package.json";
const coreVersion = readJson(corePkgPath).version;

const targets = [{ path: "packages/eslint-plugin/package.json", section: "dependencies" }];

for (const { path, section } of targets) {
  const pkg = readJson(path);
  if (setDep(pkg, section, "@graphql-analyzer/core", coreVersion)) {
    writeJson(path, pkg);
    console.log(`Updated ${path} ${section}["@graphql-analyzer/core"] -> ${coreVersion}`);
  }
}

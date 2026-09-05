// Tiny reader for pnpm-workspace.yaml limited to the structure this repo uses:
//
//   packages:
//     - "literal/path"
//     - "glob/*"
//
// Avoids adding a YAML library for one script.

import { readFileSync } from "node:fs";
import { resolve, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(SCRIPT_DIR, "..", "..");

export function readPnpmWorkspaces() {
  const yamlPath = join(REPO_ROOT, "pnpm-workspace.yaml");
  const yaml = readFileSync(yamlPath, "utf8");
  const lines = yaml.split("\n");
  const packages = [];
  let inPackages = false;

  for (const raw of lines) {
    const line = raw.replace(/#.*/, "").trimEnd();
    if (line === "" && inPackages) continue;
    if (line === "packages:") {
      inPackages = true;
      continue;
    }
    if (!inPackages) continue;
    const match = line.match(/^\s*-\s*['"]?([^'"]+)['"]?\s*$/);
    if (match) {
      packages.push(match[1]);
      continue;
    }
    if (line.match(/^\S/)) {
      // Top-level key after `packages:` — done.
      break;
    }
  }

  return packages;
}

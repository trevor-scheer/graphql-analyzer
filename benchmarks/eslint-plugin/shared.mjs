import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const directory = dirname(fileURLToPath(import.meta.url));
export const root = resolve(directory, "../..");
export const cache = join(directory, ".cache");
export const projects = join(cache, "projects");
export const schemas = join(cache, "schemas");
export const reportDirectory = join(directory, "reports");
export const pluginDirectory = join(root, "packages/eslint-plugin");
export const requirePlugin = createRequire(join(pluginDirectory, "package.json"));
export const graphql = requirePlugin("graphql");

export function json(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

export function sha256(input) {
  return createHash("sha256").update(input).digest("hex");
}

export function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

export function summary(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return {
    median: Number(median(sorted).toFixed(2)),
    p95: Number(sorted[Math.ceil(0.95 * sorted.length) - 1].toFixed(2)),
    min: Number(sorted[0].toFixed(2)),
    max: Number(sorted.at(-1).toFixed(2)),
    samples: sorted.length,
  };
}

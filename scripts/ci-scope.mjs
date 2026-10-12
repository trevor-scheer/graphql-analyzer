import { execFileSync } from "node:child_process";
import { extname } from "node:path";

export function requiresFullCi(paths) {
  return paths.length === 0 || paths.some((path) => !path.startsWith("docs/"));
}

export function requiresDocsCi(paths) {
  return paths.length === 0 || paths.some((path) => path.startsWith("docs/"));
}

export function requiresPackagePreview(paths) {
  const inputs = new Set([
    ".github/workflows/pkg-pr-new.yml",
    ".node-version",
    ".npmrc",
    "Cargo.lock",
    "Cargo.toml",
    "mise.toml",
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "rust-toolchain.toml",
    "scripts/ci-scope.mjs",
  ]);
  return (
    paths.length === 0 ||
    paths.some(
      (path) =>
        inputs.has(path) ||
        [".cargo/", "crates/", "packages/core/", "packages/eslint-plugin/"].some((prefix) =>
          path.startsWith(prefix),
        ),
    )
  );
}

export function requiresRustCi(paths) {
  const rootFiles = new Set([
    ".npmrc",
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "tsconfig.json",
  ]);
  const prefixes = ["docs/", "editors/vscode/", "packages/eslint-plugin/", "packages/web-ide/"];
  const extensions = new Set([
    ".astro",
    ".cjs",
    ".css",
    ".html",
    ".js",
    ".json",
    ".jsonc",
    ".md",
    ".mdx",
    ".mjs",
    ".png",
    ".svg",
    ".ts",
    ".tsx",
  ]);
  return (
    paths.length === 0 ||
    paths.some((path) => {
      if (rootFiles.has(path)) return false;
      if (!prefixes.some((prefix) => path.startsWith(prefix))) return true;
      const filename = path.split("/").at(-1);
      return !extensions.has(extname(path)) && ![".gitignore", ".vscodeignore"].includes(filename);
    })
  );
}

function changedPaths(base, head) {
  try {
    const diff = execFileSync(
      "git",
      ["diff", "--name-only", "--no-renames", "-z", base, head, "--"],
      { encoding: "utf8" },
    );
    return diff.split("\0").filter(Boolean);
  } catch {
    console.error("Unable to compare commits; running full CI");
    return [];
  }
}

if (process.argv[1]?.endsWith("/ci-scope.mjs")) {
  const base = process.env.BASE_SHA;
  const head = process.env.HEAD_SHA;
  const validCommits = /^[0-9a-f]{40}$/.test(base ?? "") && /^[0-9a-f]{40}$/.test(head ?? "");
  if (!validCommits) console.error("Commit IDs unavailable; running full CI");
  const paths = validCommits ? changedPaths(base, head) : [];
  const full = requiresFullCi(paths);
  const docs = requiresDocsCi(paths);
  const preview = requiresPackagePreview(paths);
  const rust = requiresRustCi(paths);
  console.error(`${paths.length} changed files; ${full ? "full" : "docs-only"} CI`);
  process.stdout.write(`full=${full}\ndocs=${docs}\npreview=${preview}\nrust=${rust}\n`);
}

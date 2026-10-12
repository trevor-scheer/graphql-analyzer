import { execFileSync } from "node:child_process";

export function requiresFullCi(paths) {
  return paths.length === 0 || paths.some((path) => !path.startsWith("docs/"));
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
  console.error(`${paths.length} changed files; ${full ? "full" : "docs-only"} CI`);
  process.stdout.write(`full=${full}\n`);
}

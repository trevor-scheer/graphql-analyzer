import assert from "node:assert/strict";
import { test } from "node:test";
import { requiresDocsCi, requiresFullCi } from "./ci-scope.mjs";

test("documentation site changes use docs CI", () => {
  assert.equal(requiresFullCi(["docs/package.json", "docs/package-lock.json"]), false);
  assert.equal(requiresFullCi(["docs/src/content/index.mdx"]), false);
  assert.equal(requiresDocsCi(["docs/package.json", "docs/package-lock.json"]), true);
});

test("mixed changes and uncertain diffs use full CI", () => {
  assert.equal(requiresFullCi(["docs/package.json", "Cargo.lock"]), true);
  assert.equal(requiresDocsCi(["docs/package.json", "Cargo.lock"]), true);
  assert.equal(requiresFullCi(["README.md"]), true);
  assert.equal(requiresDocsCi(["README.md"]), false);
  assert.equal(requiresFullCi([]), true);
  assert.equal(requiresDocsCi([]), true);
});

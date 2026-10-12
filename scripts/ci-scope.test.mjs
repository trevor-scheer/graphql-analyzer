import assert from "node:assert/strict";
import { test } from "node:test";
import {
  requiresDocsCi,
  requiresFullCi,
  requiresPackagePreview,
  requiresRustCi,
} from "./ci-scope.mjs";

test("documentation site changes use docs CI", () => {
  assert.equal(requiresFullCi(["docs/package.json", "docs/package-lock.json"]), false);
  assert.equal(requiresFullCi(["docs/src/content/index.mdx"]), false);
  assert.equal(requiresDocsCi(["docs/package.json", "docs/package-lock.json"]), true);
  assert.equal(requiresPackagePreview(["docs/package.json", "docs/package-lock.json"]), false);
});

test("mixed changes and uncertain diffs use full CI", () => {
  assert.equal(requiresFullCi(["docs/package.json", "Cargo.lock"]), true);
  assert.equal(requiresDocsCi(["docs/package.json", "Cargo.lock"]), true);
  assert.equal(requiresFullCi(["README.md"]), true);
  assert.equal(requiresDocsCi(["README.md"]), false);
  assert.equal(requiresFullCi([]), true);
  assert.equal(requiresDocsCi([]), true);
  assert.equal(requiresPackagePreview([]), true);
  assert.equal(requiresRustCi([]), true);
});

test("package previews run for source, manifest, and workflow changes", () => {
  assert.equal(requiresPackagePreview(["crates/napi/src/lib.rs"]), true);
  assert.equal(requiresPackagePreview(["packages/eslint-plugin/src/index.ts"]), true);
  assert.equal(requiresPackagePreview(["pnpm-lock.yaml"]), true);
  assert.equal(requiresPackagePreview([".node-version"]), true);
  assert.equal(requiresPackagePreview([".github/workflows/pkg-pr-new.yml"]), true);
  assert.equal(requiresPackagePreview(["scripts/ci-scope.mjs"]), true);
  assert.equal(requiresPackagePreview([".github/workflows/ci.yml"]), false);
  assert.equal(requiresPackagePreview(["packages/web-ide/src/index.ts"]), false);
});

test("JavaScript-only changes skip Rust jobs", () => {
  assert.equal(requiresRustCi(["package.json", "pnpm-lock.yaml"]), false);
  assert.equal(requiresRustCi(["packages/eslint-plugin/src/parser.ts"]), false);
  assert.equal(requiresRustCi(["packages/web-ide/src/App.tsx"]), false);
  assert.equal(requiresRustCi(["editors/vscode/package.json"]), false);
  assert.equal(requiresRustCi(["docs/package-lock.json"]), false);
});

test("Rust inputs and unknown paths keep Rust jobs", () => {
  assert.equal(requiresRustCi(["pnpm-lock.yaml", "Cargo.lock"]), true);
  assert.equal(requiresRustCi(["crates/napi/src/lib.rs"]), true);
  assert.equal(requiresRustCi(["test-workspace/.graphqlrc.yaml"]), true);
  assert.equal(requiresRustCi(["packages/core/index.js"]), true);
  assert.equal(requiresRustCi(["packages/eslint-plugin/test/schema.graphql"]), true);
  assert.equal(requiresRustCi([".github/workflows/ci.yml"]), true);
});

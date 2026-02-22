# Plan: Publish graphql-analyzer CLI on Homebrew

## Overview

This document tracks the work to distribute the `graphql-analyzer` CLI via a
custom Homebrew tap (`trevor-scheer/homebrew-graphql-analyzer`). A custom tap
is the right starting point for a specialized developer tool; homebrew-core
requires significant install volume before a formula is accepted there.

Users will install with:

```sh
brew install trevor-scheer/graphql-analyzer/graphql-analyzer
```

---

## Step 1: Create the Homebrew Tap Repository

Create a new GitHub repository: **`trevor-scheer/homebrew-graphql-analyzer`**

Homebrew resolves `brew install trevor-scheer/graphql-analyzer/graphql-analyzer`
by looking for a repo named `homebrew-graphql-analyzer` under the
`trevor-scheer` org/user.

**Repository structure:**

```
homebrew-graphql-analyzer/
├── Formula/
│   └── graphql-analyzer.rb
└── README.md
```

**Initial formula (`Formula/graphql-analyzer.rb`):**

The formula distributes pre-built binaries from GitHub releases. No
compilation on the user's machine. The `on_macos`/`on_linux` + `on_arm`/
`on_intel` blocks handle platform dispatch.

```ruby
class GraphqlAnalyzer < Formula
  desc "GraphQL validation and linting CLI"
  homepage "https://github.com/trevor-scheer/graphql-analyzer"
  version "PLACEHOLDER_VERSION"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/trevor-scheer/graphql-analyzer/releases/download/graphql-analyzer-cli%2Fv#{version}/graphql-cli-aarch64-apple-darwin.tar.xz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/trevor-scheer/graphql-analyzer/releases/download/graphql-analyzer-cli%2Fv#{version}/graphql-cli-x86_64-apple-darwin.tar.xz"
      sha256 "PLACEHOLDER"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/trevor-scheer/graphql-analyzer/releases/download/graphql-analyzer-cli%2Fv#{version}/graphql-cli-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/trevor-scheer/graphql-analyzer/releases/download/graphql-analyzer-cli%2Fv#{version}/graphql-cli-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "graphql"
  end

  test do
    system "#{bin}/graphql", "--version"
  end
end
```

**Notes on the formula:**

- The formula is named `graphql-analyzer` (for `brew install`) but installs
  the binary as `graphql` so users just type `graphql validate`. Homebrew
  doesn't require them to match.
- The `%2F` in the URL is the URL-encoded `/` in the release tag
  `graphql-analyzer-cli/v0.1.3`. GitHub release asset URLs encode slashes
  in tag names this way.
- No `bottle` block is needed. Bottles are for formulae that build from
  source — we're distributing pre-built binaries, so the formula is
  inherently "portable".
- The archive contains a single binary at the root named `graphql`.
  `bin.install "graphql"` installs it exactly as-is.

---

## Step 2: Automate Formula Updates in the Release Workflow

Every CLI release should automatically update the tap formula. Add a new job
to `.github/workflows/release.yml` that runs after the existing `release` job.

**New secret required:** `HOMEBREW_TAP_TOKEN` — a fine-grained GitHub PAT
scoped to write access on `trevor-scheer/homebrew-graphql-analyzer`. Add it
in the main repo's Settings → Secrets and variables → Actions.

**New job to add in `release.yml`:**

```yaml
update-homebrew-formula:
  needs: [check-release, release]
  if: needs.check-release.outputs.should_release == 'true'
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6

    - name: Download CLI artifacts
      uses: actions/download-artifact@v7
      with:
        pattern: binaries-*
        path: artifacts
        merge-multiple: true

    - name: Compute SHA256 hashes
      id: hashes
      run: |
        echo "aarch64_darwin=$(sha256sum artifacts/graphql-cli-aarch64-apple-darwin.tar.xz | awk '{print $1}')" >> "$GITHUB_OUTPUT"
        echo "x86_64_darwin=$(sha256sum artifacts/graphql-cli-x86_64-apple-darwin.tar.xz | awk '{print $1}')" >> "$GITHUB_OUTPUT"
        echo "aarch64_linux=$(sha256sum artifacts/graphql-cli-aarch64-unknown-linux-gnu.tar.xz | awk '{print $1}')" >> "$GITHUB_OUTPUT"
        echo "x86_64_linux=$(sha256sum artifacts/graphql-cli-x86_64-unknown-linux-gnu.tar.xz | awk '{print $1}')" >> "$GITHUB_OUTPUT"
        echo "version=$(grep -m1 '^version = ' crates/cli/Cargo.toml | sed 's/version = \"\(.*\)\"/\1/')" >> "$GITHUB_OUTPUT"

    - name: Checkout tap repository
      uses: actions/checkout@v6
      with:
        repository: trevor-scheer/homebrew-graphql-analyzer
        token: ${{ secrets.HOMEBREW_TAP_TOKEN }}
        path: homebrew-tap

    - name: Update formula
      env:
        VERSION: ${{ steps.hashes.outputs.version }}
        SHA_AARCH64_DARWIN: ${{ steps.hashes.outputs.aarch64_darwin }}
        SHA_X86_64_DARWIN: ${{ steps.hashes.outputs.x86_64_darwin }}
        SHA_AARCH64_LINUX: ${{ steps.hashes.outputs.aarch64_linux }}
        SHA_X86_64_LINUX: ${{ steps.hashes.outputs.x86_64_linux }}
      run: |
        python3 scripts/update-homebrew-formula.py \
          homebrew-tap/Formula/graphql-analyzer.rb \
          "$VERSION" \
          "$SHA_AARCH64_DARWIN" \
          "$SHA_X86_64_DARWIN" \
          "$SHA_AARCH64_LINUX" \
          "$SHA_X86_64_LINUX"

    - name: Commit and push
      working-directory: homebrew-tap
      run: |
        git config user.name "github-actions[bot]"
        git config user.email "github-actions[bot]@users.noreply.github.com"
        git add Formula/graphql-analyzer.rb
        git commit -m "chore: update graphql-analyzer to v${VERSION}"
        git push
      env:
        VERSION: ${{ steps.hashes.outputs.version }}
```

**Why a Python script for the formula update?** `sed` multi-line replacements
are fragile across macOS/GNU differences and easy to get wrong with escaped
characters in SHA256 hashes. A small Python script that parses the formula
and replaces the `version` and four `sha256` lines is more reliable and
testable. The script lives at `scripts/update-homebrew-formula.py` in the
main repo and is checked in alongside the workflow.

**Script interface:**

```
update-homebrew-formula.py <formula_path> <version> \
    <sha_aarch64_darwin> <sha_x86_64_darwin> \
    <sha_aarch64_linux> <sha_x86_64_linux>
```

The script does a targeted replacement of the `version` line and the four
`sha256` lines in the formula. It should be idempotent and diff cleanly.

---

## Step 3: Update README Installation Section

Replace or augment the current "Installation" section in `README.md` to list
Homebrew first (for macOS/Linux users) with the shell script as the fallback:

````markdown
## Installation

### Homebrew (macOS and Linux)

```sh
brew install trevor-scheer/graphql-analyzer/graphql-analyzer
```
````

Upgrade with `brew upgrade graphql-analyzer`.

### Shell script (Linux, macOS, Windows)

...existing content...

```

---

## Implementation Checklist

- [ ] **Create tap repo** — `trevor-scheer/homebrew-graphql-analyzer` with initial `Formula/graphql-analyzer.rb` (placeholders for first version, then replaced on next release).
- [ ] **Create PAT** — Fine-grained token with write access to the tap repo. Add as `HOMEBREW_TAP_TOKEN` secret in the main repo.
- [ ] **Write update script** — `scripts/update-homebrew-formula.py` to reliably replace version and SHA256 values in the formula.
- [ ] **Add release job** — `update-homebrew-formula` job in `release.yml`.
- [ ] **Update README** — Add Homebrew to the Installation section.
- [ ] **End-to-end test** — Do a release (or dry run) and verify `brew install trevor-scheer/graphql-analyzer/graphql-analyzer` works and installs the right binary.

---

## Open Questions

**Ship LSP via brew too?** The LSP binary is bundled into the VS Code
extension but could also be distributed via a separate `graphql-analyzer-lsp`
formula for users of Neovim, Helix, or other editors. Low priority for now;
document as a follow-up once the CLI formula is stable.

**homebrew-core someday?** homebrew-core requires a formula to have
significant install numbers and meet strict audit requirements. The custom tap
is the right home until that bar is met. `brew audit --strict
Formula/graphql-analyzer.rb` should be run periodically to track compliance.
```

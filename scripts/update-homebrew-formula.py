#!/usr/bin/env python3
"""Update the graphql-analyzer Homebrew formula with a new version and SHA256 checksums.

Usage:
    python3 scripts/update-homebrew-formula.py <version> <sha256_mac_arm> <sha256_mac_x64> <sha256_linux_arm> <sha256_linux_x64>

Example:
    python3 scripts/update-homebrew-formula.py 0.1.8 abc123... def456... ghi789... jkl012...

The script updates Formula/graphql-analyzer.rb in the trevor-scheer/homebrew-graphql-analyzer
tap via the GitHub API (requires HOMEBREW_TAP_TOKEN env var with write access to that repo).
"""

import hashlib
import json
import os
import re
import sys
import urllib.request
import urllib.error
import base64

REPO = "trevor-scheer/homebrew-graphql-analyzer"
FORMULA_PATH = "Formula/graphql-analyzer.rb"
RELEASE_BASE = "https://github.com/trevor-scheer/graphql-analyzer/releases/download/graphql-analyzer-cli"

TARGETS = [
    ("aarch64-apple-darwin", "mac_arm"),
    ("x86_64-apple-darwin", "mac_x64"),
    ("aarch64-unknown-linux-gnu", "linux_arm"),
    ("x86_64-unknown-linux-gnu", "linux_x64"),
]


def gh_api(path, method="GET", body=None):
    token = os.environ.get("HOMEBREW_TAP_TOKEN")
    if not token:
        raise RuntimeError("HOMEBREW_TAP_TOKEN environment variable is not set")
    url = f"https://api.github.com{path}"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "Content-Type": "application/json",
    }
    data = json.dumps(body).encode() if body else None
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"GitHub API error {e.code}: {e.read().decode()}") from e


def sha256_url(url):
    """Download a URL and return its SHA256 checksum."""
    print(f"  Downloading {url.split('/')[-1]}...", flush=True)
    with urllib.request.urlopen(url) as resp:
        data = resp.read()
    return hashlib.sha256(data).hexdigest()


def build_formula(version, shas):
    mac_arm, mac_x64, linux_arm, linux_x64 = (
        shas["mac_arm"],
        shas["mac_x64"],
        shas["linux_arm"],
        shas["linux_x64"],
    )
    return f'''\
class GraphqlAnalyzer < Formula
  desc "Fast, Rust-powered GraphQL validation and linting CLI"
  homepage "https://github.com/trevor-scheer/graphql-analyzer"
  version "{version}"
  license "MIT"

  on_macos do
    on_arm do
      url "{RELEASE_BASE}/v#{{version}}/graphql-cli-aarch64-apple-darwin.tar.xz"
      sha256 "{mac_arm}"
    end
    on_intel do
      url "{RELEASE_BASE}/v#{{version}}/graphql-cli-x86_64-apple-darwin.tar.xz"
      sha256 "{mac_x64}"
    end
  end

  on_linux do
    on_arm do
      url "{RELEASE_BASE}/v#{{version}}/graphql-cli-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "{linux_arm}"
    end
    on_intel do
      url "{RELEASE_BASE}/v#{{version}}/graphql-cli-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "{linux_x64}"
    end
  end

  def install
    bin.install "graphql"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/graphql --version")
  end
end
'''


def main():
    if len(sys.argv) not in (2, 6):
        print(__doc__)
        sys.exit(1)

    version = sys.argv[1]

    if len(sys.argv) == 6:
        # SHA256s provided explicitly
        shas = {
            "mac_arm": sys.argv[2],
            "mac_x64": sys.argv[3],
            "linux_arm": sys.argv[4],
            "linux_x64": sys.argv[5],
        }
    else:
        # Compute SHA256s by downloading the release assets
        print(f"Computing SHA256s for v{version}...", flush=True)
        shas = {}
        for target, key in TARGETS:
            url = f"{RELEASE_BASE}/v{version}/graphql-cli-{target}.tar.xz"
            shas[key] = sha256_url(url)

    new_content = build_formula(version, shas)

    # Get current file SHA (required for update)
    print(f"Fetching current formula from {REPO}...", flush=True)
    current = gh_api(f"/repos/{REPO}/contents/{FORMULA_PATH}")
    file_sha = current["sha"]

    # Push the updated formula
    print(f"Updating formula to v{version}...", flush=True)
    gh_api(
        f"/repos/{REPO}/contents/{FORMULA_PATH}",
        method="PUT",
        body={
            "message": f"chore: update graphql-analyzer to v{version}",
            "content": base64.b64encode(new_content.encode()).decode(),
            "sha": file_sha,
        },
    )
    print(f"Done. Formula updated to v{version}.")


if __name__ == "__main__":
    main()

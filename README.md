# diff-log

Terminal PR review for GitHub. Run `diff-log` inside a repo, pick an open PR, review the diff, add comments, and submit the review without leaving your terminal.

`diff-log` is built from the existing Lumen diff viewer, but the product path is focused: open PRs first, clean review workflow, no browser tab juggling.

## What You Get

- Open-PR picker by default: `diff-log`
- Side-by-side terminal diff viewer
- Inline review annotations for selections, hunks, or whole files
- Submit annotations as GitHub PR review comments
- GitHub viewed-file sync with `space`
- Fast file tree, search, syntax highlighting, and stacked-diff support
- No hosted service and no separate API key; GitHub access is handled by `gh`

## Fresh Mac Setup

These steps are for a brand-new machine.

### 1. Install Homebrew

If `brew --version` already works, skip this step.

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

After Homebrew finishes, make sure your shell can find it:

```bash
if [ -x /opt/homebrew/bin/brew ]; then eval "$(/opt/homebrew/bin/brew shellenv)"; fi
if [ -x /usr/local/bin/brew ]; then eval "$(/usr/local/bin/brew shellenv)"; fi
```

### 2. Install diff-log

```bash
brew install jnsahaj/diff-log/diff-log
```

This Homebrew formula installs the runtime dependencies too:

- `git`
- `gh` GitHub CLI
- `diff-log`
- `difflog` alias, if you prefer no hyphen

### 3. Log In To GitHub

`diff-log` uses GitHub CLI authentication. Homebrew can install `gh`, but you still need to authorize GitHub once.

```bash
```

If the browser does not open automatically, GitHub CLI will print a one-time code. Open this URL and paste the code:

https://github.com/login/device

For private repos or org repos with SSO, you may also need to authorize GitHub CLI for the org:

```bash
```

Then verify:

```bash
diff-log --version
```

## Start Reviewing

From inside a GitHub repo:

```bash
cd path/to/repo
diff-log
```

That opens a picker with all open PRs for the repo. Select one and press `Enter`.

If you do not have the repo cloned yet:

```bash
cd REPO
diff-log
```

If your `origin` remote is not a GitHub URL, pass the repo explicitly:

```bash
```

## Review Workflow

1. Run `diff-log`.
2. Pick an open PR.
3. Move through files and hunks in the TUI.
4. Press `i` to add a review annotation.
5. Press `I` to view, edit, delete, copy, or export annotations.
6. Press `s` to submit annotations to GitHub as a PR review.

Line and range annotations become GitHub review comments. File-level annotations are included in the review body.

## Common Commands

```bash
# Default product flow: pick an open PR
diff-log

# Same command, no hyphen
difflog

# Pick from open PRs explicitly
diff-log diff --pr

# Review a specific PR
diff-log diff --pr 123
diff-log diff https://github.com/OWNER/REPO/pull/123

# Review local uncommitted changes
diff-log diff

# Review a commit or range
diff-log diff HEAD~1
diff-log diff main..feature

# Focus or filter files
diff-log diff --focus src/main.rs
diff-log diff --file src/main.rs --file src/lib.rs

# Soft-wrap long lines
diff-log diff --wrap
```

The legacy `lumen` command is still installed by Cargo builds and keeps the older AI-oriented commands available.

## Keybindings

- `j/k` or arrow keys: move
- `ctrl+j` / `ctrl+k`: next / previous file
- `{` / `}`: previous / next hunk
- `space`: mark file as viewed, synced to GitHub in PR mode
- `i`: annotate selection, focused hunk, or whole file
- `I`: manage annotations
- `s`: submit PR review in PR mode
- `/`: search current file
- `ctrl+f`: search across all files
- `tab`: toggle sidebar
- `?`: show all keybindings
- `q` or `esc`: quit

## What Homebrew Installs

The Homebrew formula is intentionally small and product-safe. It installs:

- `git`, used for repo detection and local diff workflows
- `gh`, used for GitHub PR listing, PR metadata, viewed-file sync, and review submission
- prebuilt `diff-log` and `difflog` binaries

It cannot perform GitHub login for you. Run `gh auth login --web --scopes "repo"` once after install.

## Troubleshooting

### `diff-log: command not found`

Homebrew may not be on your shell path yet. Run:

```bash
if [ -x /opt/homebrew/bin/brew ]; then eval "$(/opt/homebrew/bin/brew shellenv)"; fi
if [ -x /usr/local/bin/brew ]; then eval "$(/usr/local/bin/brew shellenv)"; fi
```

Then retry:

```bash
```

### GitHub Says You Are Not Logged In

```bash
```

Manual login URL if needed:

https://github.com/login/device

### Private Repo PRs Do Not Show Up

Refresh `gh` permissions:

```bash
gh auth status
```

If the repo belongs to a GitHub organization with SSO, authorize GitHub CLI for that organization in GitHub settings.

### No Open PRs Found

Check what GitHub CLI sees:

```bash
```

If this fails, fix `gh` auth or repo remotes first.

### Repo Remote Is Not GitHub

Pass the repo manually:

```bash
```

### Review Submission Fails

Common causes:

- `gh` is not authenticated with `repo` scope
- the PR changed since you opened it; press `r` to refresh and retry
- your account does not have permission to review that repo

## Privacy And Security

- `diff-log` does not run a hosted service.
- GitHub API calls go through the local `gh` CLI.
- Auth tokens are managed by GitHub CLI, not by `diff-log`.
- Review comments are only submitted when you confirm with `s`.

## Maintainer Release Checklist

The release script expects a Homebrew tap next to this repo:

```bash
gh repo create jnsahaj/homebrew-diff-log --public --clone=false
git clone git@github.com:jnsahaj/homebrew-diff-log.git ../homebrew-diff-log
```

Then release:

```bash
./release.sh
```

The script builds macOS Intel and Apple Silicon binaries, uploads GitHub release tarballs, and writes `Formula/diff-log.rb` with:

- `depends_on "git"`
- `depends_on "gh"`
- `bin.install "diff-log"`
- `bin.install "difflog"`

Users then install with:

```bash
brew install jnsahaj/diff-log/diff-log
```

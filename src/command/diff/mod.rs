mod annotation;
mod app;
mod context;
mod coordinates;
mod diff_algo;
pub mod git;
mod global_search;
pub mod highlight;
mod render;
mod search;
mod state;
mod sticky_lines;
mod text_edit;
pub mod theme;
mod types;
mod watcher;

use std::collections::HashSet;
use std::io::{self, Write};
use std::process::{self, Command, Stdio};
use std::thread;

use inquire::Select;
use serde::{Deserialize, Serialize};
use spinoff::{spinners, Color, Spinner};

use crate::commit_reference::CommitReference;
use crate::vcs::VcsBackend;

use self::state::{Annotation, AnnotationTarget};
use self::types::DiffPanelFocus;

pub struct DiffOptions {
    pub reference: Option<CommitReference>,
    pub pr: Option<String>,
    pub file: Option<Vec<String>>,
    pub watch: bool,
    pub theme: Option<String>,
    pub stacked: bool,
    pub focus: Option<String>,
    pub origin: Option<String>,
    pub wrap: bool,
}

#[derive(Clone)]
pub struct PrInfo {
    pub number: u64,
    pub node_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub title: String,
    pub base_ref: String,
    pub head_ref: String,
    pub base_sha: String,
    pub head_sha: String,
    pub base_repo_owner: String,
    pub base_repo_name: String,
    pub head_repo_owner: Option<String>, // None if head repo was deleted (fork deleted)
    pub head_repo_name: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenPr {
    number: u64,
    title: String,
    author: Option<OpenPrAuthor>,
    head_ref_name: String,
    base_ref_name: String,
    is_draft: bool,
    review_decision: Option<String>,
    updated_at: String,
    additions: u64,
    deletions: u64,
    changed_files: u64,
}

#[derive(Clone, Deserialize)]
struct OpenPrAuthor {
    login: String,
}

impl OpenPr {
    fn label(&self) -> String {
        let author = self
            .author
            .as_ref()
            .map(|a| a.login.as_str())
            .unwrap_or("unknown");
        let state = if self.is_draft { "draft" } else { "ready" };
        let decision = self.review_decision.as_deref().unwrap_or("unreviewed");
        format!(
            "#{:<5} {:<8} {:<14} {} -> {}  {}  (+{} -{} / {} files)  @{}  {}",
            self.number,
            state,
            decision.to_lowercase(),
            self.head_ref_name,
            self.base_ref_name,
            self.title,
            self.additions,
            self.deletions,
            self.changed_files,
            author,
            self.updated_at
        )
    }
}

fn parse_pr_input(input: &str) -> Option<(Option<String>, Option<String>, u64)> {
    // Try to parse as a URL first
    if input.starts_with("http://") || input.starts_with("https://") {
        // Extract PR number and repo info from URL
        // Format: https://github.com/owner/repo/pull/123
        let parts: Vec<&str> = input.trim_end_matches('/').split('/').collect();
        if parts.len() >= 2 {
            if let Some(pos) = parts.iter().position(|&p| p == "pull") {
                if pos + 1 < parts.len() {
                    if let Ok(num) = parts[pos + 1].parse::<u64>() {
                        // Extract owner and repo
                        if pos >= 2 {
                            let owner = parts[pos - 2].to_string();
                            let repo = parts[pos - 1].to_string();
                            return Some((Some(owner), Some(repo), num));
                        }
                        return Some((None, None, num));
                    }
                }
            }
        }
        None
    } else {
        // Try to parse as a PR number
        input.parse::<u64>().ok().map(|num| (None, None, num))
    }
}

fn resolve_origin_repo() -> Result<String, String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;
    if !output.status.success() {
        return Err(
            "Could not determine repository. Set origin remote or use --origin owner/repo"
                .to_string(),
        );
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let url = url.strip_suffix(".git").unwrap_or(&url);
    let path = url
        .split("github.com")
        .nth(1)
        .ok_or_else(|| format!("Origin URL is not a GitHub URL: {}", url))?;
    let path = path.trim_start_matches(':').trim_start_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 2 {
        Ok(format!("{}/{}", parts[0], parts[1]))
    } else {
        Err(format!(
            "Could not parse owner/repo from origin URL: {}",
            url
        ))
    }
}

fn choose_open_pr(repo_override: Option<&str>) -> Result<String, String> {
    let repo_full = repo_override
        .map(str::to_string)
        .map(Ok)
        .unwrap_or_else(resolve_origin_repo)?;
    let mut spinner = Spinner::new(
        spinners::Dots,
        format!("Fetching open PRs for {}", repo_full),
        Color::Cyan,
    );

    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            "100",
            "--repo",
            &repo_full,
            "--json",
            "number,title,author,headRefName,baseRefName,isDraft,reviewDecision,updatedAt,additions,deletions,changedFiles",
        ])
        .output()
        .map_err(|e| format!("Failed to run gh pr list: {}", e))?;

    if !output.status.success() {
        spinner.fail("Could not fetch open PRs");
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh pr list failed: {}", stderr.trim()));
    }

    let prs: Vec<OpenPr> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Could not parse open PRs: {}", e))?;
    if prs.is_empty() {
        spinner.fail("No open PRs found");
        return Err(format!("No open PRs found for {}", repo_full));
    }

    spinner.success(&format!("Found {} open PRs", prs.len()));

    let labels: Vec<String> = prs.iter().map(OpenPr::label).collect();
    let selected = Select::new("Select a PR to review", labels)
        .with_help_message("Use arrows to scan PRs. Enter opens the diff reviewer.")
        .prompt()
        .map_err(|e| format!("PR selection cancelled: {}", e))?;
    let idx = prs
        .iter()
        .position(|pr| pr.label() == selected)
        .ok_or_else(|| "Selected PR was not found".to_string())?;

    Ok(prs[idx].number.to_string())
}

#[derive(Deserialize)]
struct GhPullRequest {
    number: u64,
    node_id: String,
    title: String,
    base: GhPrRef,
    head: GhPrRef,
}

#[derive(Deserialize)]
struct GhUser {
    login: String,
}

#[derive(Deserialize)]
struct GhPrRef {
    #[serde(rename = "ref")]
    ref_name: String,
    sha: String,
    repo: Option<GhRepo>,
}

#[derive(Deserialize)]
struct GhRepo {
    name: String,
    owner: GhUser,
}

fn fetch_pr_info(pr_input: &str, repo_override: Option<&str>) -> Result<PrInfo, String> {
    let (owner, repo, number) = parse_pr_input(pr_input).ok_or_else(|| {
        format!(
            "Invalid PR reference: {}. Use a PR number or URL.",
            pr_input
        )
    })?;

    let repo_full = match (&owner, &repo, repo_override) {
        (Some(o), Some(r), _) => format!("{}/{}", o, r),
        (_, _, Some(r)) => r.to_string(),
        _ => resolve_origin_repo()?,
    };

    let (repo_owner, repo_name) = {
        let parts: Vec<&str> = repo_full.split('/').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid repo format: {}", repo_full));
        }
        (
            owner.unwrap_or_else(|| parts[0].to_string()),
            repo.unwrap_or_else(|| parts[1].to_string()),
        )
    };

    let api_path = format!("repos/{}/{}/pulls/{}", repo_owner, repo_name, number);
    let output = Command::new("gh")
        .args(["api", &api_path])
        .output()
        .map_err(|e| format!("Failed to run gh api: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh api failed: {}", stderr.trim()));
    }

    let pr: GhPullRequest = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Could not parse PR metadata: {}", e))?;

    let base_repo_owner = pr
        .base
        .repo
        .as_ref()
        .map(|r| r.owner.login.clone())
        .unwrap_or_else(|| repo_owner.clone());
    let base_repo_name = pr
        .base
        .repo
        .as_ref()
        .map(|r| r.name.clone())
        .unwrap_or_else(|| repo_name.clone());
    let head_repo_owner = pr.head.repo.as_ref().map(|r| r.owner.login.clone());
    let head_repo_name = pr.head.repo.as_ref().map(|r| r.name.clone());

    Ok(PrInfo {
        number: pr.number,
        node_id: pr.node_id,
        repo_owner,
        repo_name,
        title: pr.title,
        base_ref: pr.base.ref_name,
        head_ref: pr.head.ref_name,
        base_sha: pr.base.sha,
        head_sha: pr.head.sha,
        base_repo_owner,
        base_repo_name,
        head_repo_owner,
        head_repo_name,
    })
}

#[derive(Serialize)]
struct ReviewRequest<'a> {
    commit_id: &'a str,
    event: &'static str,
    body: String,
    comments: Vec<ReviewComment<'a>>,
}

#[derive(Serialize)]
struct ReviewComment<'a> {
    path: &'a str,
    body: &'a str,
    line: usize,
    side: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_side: Option<&'static str>,
}

pub fn submit_pr_review(pr_info: &PrInfo, annotations: &[Annotation]) -> Result<usize, String> {
    if annotations.is_empty() {
        return Err("No annotations to submit".to_string());
    }

    let mut body_sections = Vec::new();
    let mut comments = Vec::new();

    for annotation in annotations {
        match &annotation.target {
            AnnotationTarget::File => {
                body_sections.push(format!(
                    "**{}**\n\n{}",
                    annotation.filename, annotation.content
                ));
            }
            AnnotationTarget::LineRange {
                panel,
                start_line,
                end_line,
            } => {
                let side = match panel {
                    DiffPanelFocus::Old => "LEFT",
                    _ => "RIGHT",
                };
                let (start_line, end_line) = if start_line <= end_line {
                    (*start_line, *end_line)
                } else {
                    (*end_line, *start_line)
                };
                comments.push(ReviewComment {
                    path: &annotation.filename,
                    body: &annotation.content,
                    line: end_line,
                    side,
                    start_line: (start_line != end_line).then_some(start_line),
                    start_side: (start_line != end_line).then_some(side),
                });
            }
        }
    }

    let body = if body_sections.is_empty() {
        "Review comments from lumen.".to_string()
    } else {
        body_sections.join("\n\n---\n\n")
    };
    let request = ReviewRequest {
        commit_id: &pr_info.head_sha,
        event: "COMMENT",
        body,
        comments,
    };
    let payload = serde_json::to_vec(&request)
        .map_err(|e| format!("Could not encode review payload: {}", e))?;

    let api_path = format!(
        "repos/{}/{}/pulls/{}/reviews",
        pr_info.repo_owner, pr_info.repo_name, pr_info.number
    );
    let mut child = Command::new("gh")
        .args(["api", "-X", "POST", &api_path, "--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run gh api: {}", e))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(&payload)
            .map_err(|e| format!("Failed to send review payload: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for gh api: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh api failed: {}", stderr.trim()));
    }

    Ok(annotations.len())
}

/// Fetch the list of files that are marked as viewed on GitHub
pub fn fetch_viewed_files(pr_info: &PrInfo) -> Result<HashSet<String>, String> {
    let query = format!(
        r#"query {{ repository(owner: "{}", name: "{}") {{ pullRequest(number: {}) {{ files(first: 100) {{ nodes {{ path viewerViewedState }} }} }} }} }}"#,
        pr_info.repo_owner, pr_info.repo_name, pr_info.number
    );

    let output = Command::new("gh")
        .args(["api", "graphql", "-f", &format!("query={}", query)])
        .output()
        .map_err(|e| format!("Failed to run gh api graphql: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh api graphql failed: {}", stderr.trim()));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);

    // Parse the response to find viewed files
    // Look for patterns like: "path":"filename","viewerViewedState":"VIEWED"
    let mut viewed_files = HashSet::new();

    // Simple parsing: find all path/viewerViewedState pairs
    let mut remaining = json_str.as_ref();
    while let Some(path_start) = remaining.find("\"path\":\"") {
        let path_value_start = path_start + 8;
        let after_path = &remaining[path_value_start..];
        if let Some(path_end) = after_path.find('"') {
            let path = &after_path[..path_end];

            // Look for viewerViewedState after this path
            let after_path_str = &after_path[path_end..];
            if let Some(state_start) = after_path_str.find("\"viewerViewedState\":\"") {
                let state_value_start = state_start + 21;
                let after_state = &after_path_str[state_value_start..];
                if let Some(state_end) = after_state.find('"') {
                    let state = &after_state[..state_end];
                    if state == "VIEWED" {
                        viewed_files.insert(path.to_string());
                    }
                }
            }

            remaining = &remaining[path_value_start + path_end..];
        } else {
            break;
        }
    }

    Ok(viewed_files)
}

/// Mark a file as viewed on GitHub PR (non-blocking, spawns a thread)
pub fn mark_file_as_viewed_async(pr_info: &PrInfo, file_path: &str) {
    let node_id = pr_info.node_id.clone();
    let path = file_path.to_string();

    thread::spawn(move || {
        let _ = mark_file_as_viewed_sync(&node_id, &path);
    });
}

/// Unmark a file as viewed on GitHub PR (non-blocking, spawns a thread)
pub fn unmark_file_as_viewed_async(pr_info: &PrInfo, file_path: &str) {
    let node_id = pr_info.node_id.clone();
    let path = file_path.to_string();

    thread::spawn(move || {
        let _ = unmark_file_as_viewed_sync(&node_id, &path);
    });
}

/// Mark a file as viewed on GitHub PR (blocking)
fn mark_file_as_viewed_sync(node_id: &str, file_path: &str) -> Result<(), String> {
    let mutation = format!(
        r#"mutation {{ markFileAsViewed(input: {{ pullRequestId: "{}", path: "{}" }}) {{ clientMutationId }} }}"#,
        node_id, file_path
    );

    let output = Command::new("gh")
        .args(["api", "graphql", "-f", &format!("query={}", mutation)])
        .output()
        .map_err(|e| format!("Failed to run gh api graphql: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }

    Ok(())
}

/// Unmark a file as viewed on GitHub PR (blocking)
fn unmark_file_as_viewed_sync(node_id: &str, file_path: &str) -> Result<(), String> {
    let mutation = format!(
        r#"mutation {{ unmarkFileAsViewed(input: {{ pullRequestId: "{}", path: "{}" }}) {{ clientMutationId }} }}"#,
        node_id, file_path
    );

    let output = Command::new("gh")
        .args(["api", "graphql", "-f", &format!("query={}", mutation)])
        .output()
        .map_err(|e| format!("Failed to run gh api graphql: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }

    Ok(())
}

pub fn run_diff_ui(options: DiffOptions, backend: &dyn VcsBackend) -> io::Result<()> {
    // Handle PR mode
    if let Some(ref pr_input) = options.pr {
        let pr_input = if pr_input.trim().is_empty() {
            match choose_open_pr(options.origin.as_deref()) {
                Ok(selected) => selected,
                Err(e) => {
                    eprintln!("\x1b[91merror:\x1b[0m {}", e);
                    process::exit(1);
                }
            }
        } else {
            pr_input.clone()
        };
        let spinner_msg = match parse_pr_input(&pr_input) {
            Some((Some(owner), Some(repo), number)) => {
                format!("Fetching PR {}/{}#{}", owner, repo, number)
            }
            Some((_, _, number)) => {
                format!("Fetching PR #{}", number)
            }
            None => "Fetching PR".to_string(),
        };
        let mut spinner = Spinner::new(spinners::Dots, spinner_msg, Color::Cyan);
        match fetch_pr_info(&pr_input, options.origin.as_deref()) {
            Ok(pr_info) => {
                spinner.success("Fetched PR metadata");
                return app::run_app_with_pr(options, pr_info, backend);
            }
            Err(e) => {
                spinner.fail(&e);
                process::exit(1);
            }
        }
    }

    // Also check if the reference looks like a PR (number or URL)
    if let Some(CommitReference::Single(ref input)) = options.reference {
        if input.contains("/pull/") || input.parse::<u64>().is_ok() {
            let spinner_msg = match parse_pr_input(input) {
                Some((Some(owner), Some(repo), number)) => {
                    format!("Fetching PR {}/{}#{}", owner, repo, number)
                }
                Some((_, _, number)) => {
                    format!("Fetching PR #{}", number)
                }
                None => "Fetching PR".to_string(),
            };
            let mut spinner = Spinner::new(spinners::Dots, spinner_msg, Color::Cyan);
            match fetch_pr_info(input, options.origin.as_deref()) {
                Ok(pr_info) => {
                    spinner.success("Fetched PR metadata");
                    return app::run_app_with_pr(options, pr_info, backend);
                }
                Err(e) => {
                    spinner.fail(&e);
                    process::exit(1);
                }
            }
        }
    }

    // Handle stacked mode for range references
    if options.stacked {
        if let Some(ref reference) = options.reference {
            let (from, to) = match reference {
                CommitReference::Range { from, to } => (from.clone(), to.clone()),
                CommitReference::TripleDots { from, to } => {
                    // Get merge-base for triple dots
                    let merge_base = backend
                        .get_merge_base(from, to)
                        .unwrap_or_else(|_| from.clone());
                    (merge_base, to.clone())
                }
                CommitReference::Single(_) | CommitReference::RangeToWorkingTree { .. } => {
                    eprintln!(
                        "\x1b[91merror:\x1b[0m --stacked requires a range (e.g., main..feature)"
                    );
                    process::exit(1);
                }
            };

            let commits = match backend.get_commits_in_range(&from, &to) {
                Ok(c) if c.is_empty() => {
                    eprintln!(
                        "\x1b[91merror:\x1b[0m No commits found in range {}..{}",
                        from, to
                    );
                    process::exit(1);
                }
                Ok(c) => c,
                Err(e) => {
                    eprintln!("\x1b[91merror:\x1b[0m {}", e);
                    process::exit(1);
                }
            };

            return app::run_app_stacked(options, commits, backend);
        } else {
            eprintln!("\x1b[91merror:\x1b[0m --stacked requires a range (e.g., main..feature)");
            process::exit(1);
        }
    }

    app::run_app(options, None, backend)
}

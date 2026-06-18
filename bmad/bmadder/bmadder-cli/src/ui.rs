use crate::story_io;
use bmadder_core::config::{Config, Phase};
use bmadder_core::story::{Story, StoryStatus};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const INDEX_HTML: &str = include_str!("../../../../ui/workflow-visualization-interface/workflow-visualization-interface/project/BMADder Console.dc.html");

#[derive(Debug, Serialize)]
struct UiStatus {
    project_root: String,
    counts: BTreeMap<String, usize>,
    paths: UiPaths,
    models: BTreeMap<String, String>,
    roles: BTreeMap<String, UiRole>,
    agent_hints: BTreeMap<String, String>,
    defaults: UiDefaults,
}

#[derive(Debug, Serialize)]
struct UiPaths {
    skills_dir: String,
    headless_dir: String,
    stories_dir: String,
    state_dir: String,
    prd_file: String,
    architecture_file: String,
    orchestrator_marker: String,
}

#[derive(Debug, Serialize)]
struct UiRole {
    personality: String,
    model: String,
    resolved_model: String,
    headless: String,
}

#[derive(Debug, Serialize)]
struct UiDefaults {
    max_dev_iterations: u32,
    max_sm_iterations: u32,
    max_qa_passes: u32,
    story_timeout_seconds: u64,
    gemini_cooldown_seconds: u64,
    gemini_initial_backoff: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiStory {
    id: String,
    num: String,
    title: String,
    status: String,
    epic: String,
    summary: String,
    files: Vec<String>,
    hint: String,
    model: String,
    ac_done: usize,
    ac_total: usize,
    path: String,
    priority: Option<String>,
    assigned_dev: Option<String>,
    po_alignment: Option<String>,
    qa_status: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct UiLogEntry {
    time: String,
    actor: String,
    story_id: String,
    event: String,
    detail: String,
    text: String,
    fail: bool,
}

#[derive(Debug, Deserialize)]
struct RunRequest {
    command: String,
    story: Option<String>,
    dry_run: Option<bool>,
    no_commit: Option<bool>,
    skip_po: Option<bool>,
    skip_sm: Option<bool>,
    from_existing: Option<bool>,
    start_from: Option<String>,
}

pub fn run_ui(
    config: &Config,
    config_path: &Path,
    host: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let address = format!("{}:{}", host, port);
    let server =
        Server::http(&address).map_err(|err| format!("failed to bind {address}: {err}"))?;
    println!("BMADder Console listening on http://{}", address);
    println!("Press Ctrl+C to stop.");

    for request in server.incoming_requests() {
        if let Err(err) = handle_request(request, config, config_path) {
            eprintln!("UI request error: {}", err);
        }
    }
    Ok(())
}

fn handle_request(
    mut request: Request,
    config: &Config,
    config_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let method = request.method().clone();
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/");

    match (method, path) {
        (Method::Get, "/") | (Method::Get, "/index.html") => {
            respond_html(request, INDEX_HTML.to_string())?;
        }
        (Method::Get, "/api/status") => respond_json(request, &status_payload(config)?)?,
        (Method::Get, "/api/stories") => respond_json(request, &stories_payload(config)?)?,
        (Method::Get, "/api/logs/activity") => {
            let limit = query_param(&url, "limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(200);
            respond_json(request, &activity_log_payload(config, limit)?)?;
        }
        (Method::Post, "/api/run") => {
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body)?;
            let run: RunRequest = serde_json::from_str(&body)?;
            let payload = spawn_runtime(config_path, run)?;
            respond_json(request, &payload)?;
        }
        (Method::Options, _) => respond_empty(request, 204)?,
        _ => respond_error(request, 404, "not found")?,
    }

    Ok(())
}

fn status_payload(config: &Config) -> Result<UiStatus, Box<dyn std::error::Error>> {
    let mut counts = BTreeMap::new();
    let statuses = StoryStatus::all();
    let mut total = 0;
    for status in statuses {
        let count = story_io::count_by_status(&config.paths.stories_dir, status);
        counts.insert(status.label().to_string(), count);
        total += count;
    }
    counts.insert("total".to_string(), total);

    let models = config
        .models
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let roles = config
        .roles
        .iter()
        .map(|(key, role)| {
            let resolved_model = config
                .models
                .get(&role.model)
                .cloned()
                .unwrap_or_else(|| role.model.clone());
            (
                key.clone(),
                UiRole {
                    personality: role.personality.clone(),
                    model: role.model.clone(),
                    resolved_model,
                    headless: role.headless.clone(),
                },
            )
        })
        .collect();
    let agent_hints = config
        .agent_hints
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    Ok(UiStatus {
        project_root: display_path(&config.project_root),
        counts,
        paths: UiPaths {
            skills_dir: display_path(&config.paths.skills_dir),
            headless_dir: display_path(&config.paths.headless_dir),
            stories_dir: display_path(&config.paths.stories_dir),
            state_dir: display_path(&config.paths.state_dir),
            prd_file: display_path(&config.paths.prd_file),
            architecture_file: display_path(&config.paths.architecture_file),
            orchestrator_marker: display_path(&config.paths.orchestrator_marker),
        },
        models,
        roles,
        agent_hints,
        defaults: UiDefaults {
            max_dev_iterations: config.defaults.max_dev_iterations,
            max_sm_iterations: config.defaults.max_sm_iterations,
            max_qa_passes: config.defaults.max_qa_passes,
            story_timeout_seconds: config.defaults.story_timeout_seconds,
            gemini_cooldown_seconds: config.defaults.gemini_cooldown_seconds,
            gemini_initial_backoff: config.defaults.gemini_initial_backoff,
        },
    })
}

fn stories_payload(config: &Config) -> Result<Vec<UiStory>, Box<dyn std::error::Error>> {
    let paths = story_io::list_stories(&config.paths.stories_dir)?;
    let mut stories = Vec::new();
    for path in paths {
        let story = story_io::parse_story_file(&path)?;
        stories.push(story_payload(config, &story));
    }
    Ok(stories)
}

fn story_payload(config: &Config, story: &Story) -> UiStory {
    let fm = &story.frontmatter;
    let (ac_done, ac_total) = acceptance_counts(&story.body);
    UiStory {
        id: fm.story_id.clone(),
        num: story_number(&fm.story_id),
        title: fm.title.clone(),
        status: fm.status.label().to_string(),
        epic: fm
            .epic_id
            .clone()
            .unwrap_or_else(|| "Unassigned epic".into()),
        summary: summary_from_body(&story.body),
        files: files_from_body(&story.body),
        hint: fm.agent_hint.clone().unwrap_or_else(|| "default".into()),
        model: config.resolve_model(Phase::Dev, Some(story)),
        ac_done,
        ac_total,
        path: display_path(&story.path),
        priority: fm.priority.clone(),
        assigned_dev: fm.assigned_dev.clone(),
        po_alignment: fm.po_alignment.clone(),
        qa_status: fm.qa_status.clone(),
        updated_at: fm.updated_at.clone(),
    }
}

fn activity_log_payload(
    config: &Config,
    limit: usize,
) -> Result<Vec<UiLogEntry>, Box<dyn std::error::Error>> {
    let path = config.activity_log_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let mut entries: Vec<UiLogEntry> = content.lines().filter_map(parse_activity_line).collect();
    if entries.len() > limit {
        entries = entries.split_off(entries.len() - limit);
    }
    Ok(entries)
}

fn parse_activity_line(line: &str) -> Option<UiLogEntry> {
    let parts: Vec<&str> = line.splitn(5, " | ").collect();
    if parts.len() != 5 {
        return None;
    }
    let actor = parts[1].trim().to_string();
    let story_id = parts[2].trim().to_string();
    let event = parts[3].trim().to_string();
    let detail = parts[4].trim().to_string();
    let fail = event.contains("FAIL") || detail.contains("FAIL") || detail.contains("REFIX");
    Some(UiLogEntry {
        time: parts[0].trim().to_string(),
        text: format!("{} {} · {}", actor, story_id, detail),
        actor,
        story_id,
        event,
        detail,
        fail,
    })
}

fn spawn_runtime(
    config_path: &Path,
    run: RunRequest,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let command = run.command.to_ascii_lowercase();
    let allowed = [
        "plan",
        "dev",
        "qa",
        "cycle",
        "iterative",
        "status",
        "validate",
    ];
    if !allowed.contains(&command.as_str()) {
        return Err(format!("unsupported command: {}", run.command).into());
    }

    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("--config").arg(config_path);
    if run.dry_run.unwrap_or(false) {
        cmd.arg("--dry-run");
    }
    if run.no_commit.unwrap_or(false) {
        cmd.arg("--no-commit");
    }
    if run.skip_po.unwrap_or(false) {
        cmd.arg("--skip-po");
    }
    if run.skip_sm.unwrap_or(false) {
        cmd.arg("--skip-sm");
    }
    if run.from_existing.unwrap_or(false) {
        cmd.arg("--from-existing");
    }
    if let Some(story) = &run.story {
        cmd.arg("--story").arg(story);
    }
    if let Some(start_from) = &run.start_from {
        cmd.arg("--start-from").arg(start_from);
    }
    cmd.arg(&command);

    let child = cmd.spawn()?;
    Ok(json!({
        "ok": true,
        "pid": child.id(),
        "command": command,
    }))
}

fn summary_from_body(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('#'))
        .filter(|line| !line.starts_with("---"))
        .filter(|line| !line.starts_with("- ["))
        .next()
        .map(|line| line.chars().take(180).collect())
        .unwrap_or_else(|| "No story body summary yet.".into())
}

fn files_from_body(body: &str) -> Vec<String> {
    let re = Regex::new(r"`([^`]+\.[A-Za-z0-9_+.-]+)`").unwrap();
    let mut files = Vec::new();
    for cap in re.captures_iter(body) {
        let file = cap[1].to_string();
        if (file.contains('/') || file.contains('.')) && !files.contains(&file) {
            files.push(file);
        }
        if files.len() >= 8 {
            break;
        }
    }
    files
}

fn acceptance_counts(body: &str) -> (usize, usize) {
    let mut done = 0;
    let mut total = 0;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- [ ]")
            || trimmed.starts_with("- [x]")
            || trimmed.starts_with("- [X]")
        {
            total += 1;
            if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") {
                done += 1;
            }
        }
    }
    (done, total)
}

fn story_number(story_id: &str) -> String {
    let digits: String = story_id.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        story_id.to_string()
    } else {
        let trimmed = digits.trim_start_matches('0');
        if trimmed.is_empty() {
            "0".into()
        } else {
            trimmed.to_string()
        }
    }
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == key {
            return Some(v.to_string());
        }
    }
    None
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn respond_html(request: Request, body: String) -> Result<(), Box<dyn std::error::Error>> {
    let response =
        Response::from_string(body).with_header(header("Content-Type", "text/html; charset=utf-8"));
    request.respond(response)?;
    Ok(())
}

fn respond_json<T: Serialize>(
    request: Request,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = serde_json::to_string_pretty(value)?;
    let response = Response::from_string(body)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"));
    request.respond(response)?;
    Ok(())
}

fn respond_empty(request: Request, status: u16) -> Result<(), Box<dyn std::error::Error>> {
    request.respond(Response::empty(StatusCode(status)))?;
    Ok(())
}

fn respond_error(
    request: Request,
    status: u16,
    message: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = Response::from_string(json!({ "error": message }).to_string())
        .with_status_code(StatusCode(status))
        .with_header(header("Content-Type", "application/json; charset=utf-8"));
    request.respond(response)?;
    Ok(())
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid static header")
}

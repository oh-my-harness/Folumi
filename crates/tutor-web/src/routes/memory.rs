use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use llm_harness_runtime_sandbox_os::OsEnv;
use llm_harness_types::ExecutionEnv;
use serde::{Deserialize, Serialize};
use tutor_agent::llm_provider::{LlmConfig, LlmProviderKind};
use tutor_agent::memory::MemoryOutputLanguage;

use crate::memory_store::{
    FileMemoryBackend, MemoryAssistAction, MemoryChange, MemoryChangeOp, MemoryChangeSet,
    MemoryFile, MemoryFinding, memory_entry_text_limit, parse_memory_entries,
};
use crate::memory_tool::{
    ListMemoryEntriesTool, ListMemoryEventsTool, MemoryEvidenceActivity, MemoryEvidenceTracker,
    ReadMemoryContextTool, ReadMemoryEntrySourcesTool, ReadMemoryEntryTool, ReadMemoryEventTool,
    ReadMemorySourceTool, SearchMemoryEntriesTool, SearchMemoryEventsTool,
};

#[derive(Deserialize)]
struct MemoryFileQuery {
    path: String,
}

#[derive(Deserialize)]
struct UpdateMemoryFileRequest {
    markdown: String,
}

#[derive(Deserialize)]
struct EventsQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct SourceQuery {
    reference: String,
}

#[derive(Deserialize)]
struct UndoMemoryRequest {
    target_path: String,
}

#[derive(Clone, Deserialize)]
struct MemoryLlmConfig {
    provider: String,
    model: String,
    api_key: Option<String>,
    base_url: Option<String>,
    chat_path: Option<String>,
    context_window_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct StartMemoryRunRequest {
    target_path: String,
    action: MemoryAssistAction,
    #[serde(default)]
    output_language: MemoryOutputLanguage,
    llm: MemoryLlmConfig,
}

#[derive(Deserialize)]
struct ApplyMemoryRunRequest {
    accepted_change_ids: Vec<String>,
}

#[derive(Deserialize)]
struct MemoryRunsQuery {
    active_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct MemoryRunFlowItem {
    stage: String,
    status: String,
    summary: String,
}

#[derive(Debug, Clone, Serialize)]
struct MemoryRunSnapshot {
    run_id: String,
    target_path: String,
    action: MemoryAssistAction,
    output_language: MemoryOutputLanguage,
    started_at: chrono::DateTime<chrono::Utc>,
    status: String,
    current_stage: String,
    flow: Vec<MemoryRunFlowItem>,
    change_set: Option<MemoryChangeSet>,
    error: Option<String>,
}

#[derive(Clone)]
pub(crate) struct MemoryState {
    store: Arc<FileMemoryBackend>,
    workflow_root: PathBuf,
    runs: Arc<tokio::sync::RwLock<HashMap<String, MemoryRunSnapshot>>>,
    tasks: Arc<tokio::sync::RwLock<HashMap<String, tokio::task::AbortHandle>>>,
}

async fn start_memory_run(
    State(state): State<MemoryState>,
    Json(req): Json<StartMemoryRunRequest>,
) -> impl IntoResponse {
    let llm = match build_llm_config(req.llm) {
        Ok(llm) => llm,
        Err(err) => return error_response(StatusCode::BAD_REQUEST, err),
    };
    let file = match state.store.read(&req.target_path) {
        Ok(file) => file,
        Err(err) => return error_response(StatusCode::BAD_REQUEST, err.to_string()),
    };
    let run_id = uuid::Uuid::new_v4().to_string();
    let snapshot = MemoryRunSnapshot {
        run_id: run_id.clone(),
        target_path: file.path.clone(),
        action: req.action,
        output_language: req.output_language,
        started_at: chrono::Utc::now(),
        status: "running".into(),
        current_stage: "queued".into(),
        flow: vec![MemoryRunFlowItem {
            stage: "queued".into(),
            status: "done".into(),
            summary: "Memory run queued".into(),
        }],
        change_set: None,
        error: None,
    };
    state
        .runs
        .write()
        .await
        .insert(run_id.clone(), snapshot.clone());

    let task_state = state.clone();
    let task_run_id = run_id.clone();
    let (start_tx, start_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = start_rx.await;
        update_run_stage(
            &task_state.runs,
            &task_run_id,
            "discovering_sources",
            "running",
            "Preparing the L1 evidence catalog",
        )
        .await;
        let (activity_tx, mut activity_rx) = tokio::sync::mpsc::unbounded_channel();
        let tracker = MemoryEvidenceTracker::with_activity_sender(activity_tx);
        let activity_runs = task_state.runs.clone();
        let activity_run_id = task_run_id.clone();
        let activity_task = tokio::spawn(async move {
            while let Some(activity) = activity_rx.recv().await {
                record_evidence_activity(&activity_runs, &activity_run_id, activity).await;
            }
        });

        let result = run_memory_change_set(MemoryChangeRun {
            store: task_state.store.clone(),
            workflow_root: &task_state.workflow_root,
            file,
            action: req.action,
            output_language: req.output_language,
            llm: &llm,
            tracker,
            run_id: task_run_id.clone(),
        })
        .await;
        let _ = activity_task.await;
        match result {
            Ok(change_set) => {
                update_run_stage(
                    &task_state.runs,
                    &task_run_id,
                    "analyzing_memory",
                    "done",
                    "Compared the evidence with the current memory document",
                )
                .await;
                update_run_stage(
                    &task_state.runs,
                    &task_run_id,
                    "proposing_changes",
                    "done",
                    "Built a structured memory change set",
                )
                .await;
                let summary = if change_set.changes.is_empty() {
                    "No changes were needed; review the result and finish".to_string()
                } else {
                    format!("{} changes ready for review", change_set.changes.len())
                };
                let mut runs = task_state.runs.write().await;
                if let Some(run) = runs.get_mut(&task_run_id) {
                    run.current_stage = "awaiting_review".into();
                    run.status = "awaiting_review".into();
                    run.flow.push(MemoryRunFlowItem {
                        stage: "validating_changes".into(),
                        status: "done".into(),
                        summary: "Change anchors and evidence references validated".into(),
                    });
                    run.flow.push(MemoryRunFlowItem {
                        stage: "awaiting_review".into(),
                        status: "waiting".into(),
                        summary,
                    });
                    run.change_set = Some(change_set);
                }
            }
            Err(err) => {
                let mut runs = task_state.runs.write().await;
                if let Some(run) = runs.get_mut(&task_run_id) {
                    run.current_stage = "failed".into();
                    run.status = "failed".into();
                    run.error = Some(err.clone());
                    run.flow.push(MemoryRunFlowItem {
                        stage: "failed".into(),
                        status: "error".into(),
                        summary: err,
                    });
                }
            }
        }
        task_state.tasks.write().await.remove(&task_run_id);
    });
    state
        .tasks
        .write()
        .await
        .insert(run_id.clone(), task.abort_handle());
    let _ = start_tx.send(());

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "run": snapshot })),
    )
        .into_response()
}

async fn get_memory_run(
    State(state): State<MemoryState>,
    AxumPath(run_id): AxumPath<String>,
) -> impl IntoResponse {
    match state.runs.read().await.get(&run_id).cloned() {
        Some(run) => (StatusCode::OK, Json(serde_json::json!({ "run": run }))).into_response(),
        None => error_response(StatusCode::NOT_FOUND, "memory run was not found".into()),
    }
}

async fn list_memory_runs(
    State(state): State<MemoryState>,
    Query(query): Query<MemoryRunsQuery>,
) -> impl IntoResponse {
    let active_only = query.active_only.unwrap_or(false);
    let mut runs = state
        .runs
        .read()
        .await
        .values()
        .filter(|run| !active_only || matches!(run.status.as_str(), "running" | "awaiting_review"))
        .cloned()
        .collect::<Vec<_>>();
    runs.sort_by_key(|run| std::cmp::Reverse(run.started_at));
    (StatusCode::OK, Json(serde_json::json!({ "runs": runs }))).into_response()
}

async fn cancel_memory_run(
    State(state): State<MemoryState>,
    AxumPath(run_id): AxumPath<String>,
) -> impl IntoResponse {
    let Some(task) = state.tasks.write().await.remove(&run_id) else {
        return match state.runs.read().await.get(&run_id) {
            Some(_) => error_response(StatusCode::CONFLICT, "memory run is not active".into()),
            None => error_response(StatusCode::NOT_FOUND, "memory run was not found".into()),
        };
    };
    task.abort();
    let mut runs = state.runs.write().await;
    let Some(run) = runs.get_mut(&run_id) else {
        return error_response(StatusCode::NOT_FOUND, "memory run was not found".into());
    };
    run.current_stage = "cancelled".into();
    run.status = "cancelled".into();
    run.flow.push(MemoryRunFlowItem {
        stage: "cancelled".into(),
        status: "done".into(),
        summary: "Memory run cancelled by the user".into(),
    });
    (
        StatusCode::OK,
        Json(serde_json::json!({ "run": run.clone() })),
    )
        .into_response()
}

async fn apply_memory_run(
    State(state): State<MemoryState>,
    AxumPath(run_id): AxumPath<String>,
    Json(req): Json<ApplyMemoryRunRequest>,
) -> impl IntoResponse {
    let change_set = {
        let runs = state.runs.read().await;
        let Some(run) = runs.get(&run_id) else {
            return error_response(StatusCode::NOT_FOUND, "memory run was not found".into());
        };
        let Some(change_set) = run.change_set.clone() else {
            return error_response(
                StatusCode::CONFLICT,
                "memory run is not ready for review".into(),
            );
        };
        change_set
    };
    update_run_stage(
        &state.runs,
        &run_id,
        "applying",
        "running",
        "Applying accepted changes",
    )
    .await;
    match state.store.apply_memory_changes(
        &change_set.target_path,
        &change_set.base_revision,
        &change_set.changes,
        &req.accepted_change_ids,
    ) {
        Ok(file) => {
            update_run_stage(
                &state.runs,
                &run_id,
                "completed",
                "done",
                "Accepted changes applied",
            )
            .await;
            if let Some(run) = state.runs.write().await.get_mut(&run_id) {
                run.status = "completed".into();
            }
            (StatusCode::OK, Json(serde_json::json!({ "file": file }))).into_response()
        }
        Err(err) => {
            if let Some(run) = state.runs.write().await.get_mut(&run_id) {
                run.current_stage = "awaiting_review".into();
                run.status = "awaiting_review".into();
                run.error = Some(err.to_string());
            }
            error_response(StatusCode::CONFLICT, err.to_string())
        }
    }
}

async fn update_run_stage(
    runs: &tokio::sync::RwLock<HashMap<String, MemoryRunSnapshot>>,
    run_id: &str,
    stage: &str,
    status: &str,
    summary: &str,
) {
    let mut runs = runs.write().await;
    if let Some(run) = runs.get_mut(run_id) {
        run.current_stage = stage.into();
        run.flow.push(MemoryRunFlowItem {
            stage: stage.into(),
            status: status.into(),
            summary: summary.into(),
        });
    }
}

async fn record_evidence_activity(
    runs: &tokio::sync::RwLock<HashMap<String, MemoryRunSnapshot>>,
    run_id: &str,
    activity: MemoryEvidenceActivity,
) {
    update_run_stage(runs, run_id, &activity.stage, "done", &activity.summary).await;
}

struct MemoryChangeRun<'a> {
    store: Arc<FileMemoryBackend>,
    workflow_root: &'a Path,
    file: MemoryFile,
    action: MemoryAssistAction,
    output_language: MemoryOutputLanguage,
    llm: &'a LlmConfig,
    tracker: MemoryEvidenceTracker,
    run_id: String,
}

async fn run_memory_change_set(run: MemoryChangeRun<'_>) -> Result<MemoryChangeSet, String> {
    let MemoryChangeRun {
        store,
        workflow_root,
        file,
        action,
        output_language,
        llm,
        tracker,
        run_id,
    } = run;
    let context = store
        .agent_context(&file.path, &file.markdown)
        .map_err(|err| err.to_string())?;
    let workflow_input = tutor_agent::memory::MemoryWorkflowInput {
        target_path: file.path.clone(),
        action: workflow_action(action),
        output_language,
        current_markdown: file.markdown.clone(),
        consolidation_input_json: serde_json::to_string_pretty(&context)
            .map_err(|err| err.to_string())?,
    };
    let run = run_memory_runtime_workflow_with_tools(
        llm,
        workflow_root,
        &workflow_input,
        memory_evidence_tools(store.clone(), tracker.clone(), &file.path),
    )
    .await
    .map_err(|err| err.to_string())?;
    let unread_refs = unread_workflow_refs(&run.output, &tracker);
    let oversized_changes = oversized_workflow_changes(&run.output, &file.path);
    let output = if unread_refs.is_empty() && oversized_changes.is_empty() {
        run.output
    } else {
        tracker.record_stage(
            "validating_changes",
            format!(
                "Found {} unread citations and {} oversized changes; requesting one repair pass",
                unread_refs.len(),
                oversized_changes.len()
            ),
        );
        let repair_input =
            workflow_repair_input(&workflow_input, &unread_refs, &oversized_changes)?;
        run_memory_runtime_workflow_with_tools(
            llm,
            workflow_root,
            &repair_input,
            memory_evidence_tools(store, tracker.clone(), &file.path),
        )
        .await
        .map_err(|err| err.to_string())?
        .output
    };
    workflow_output_to_change_set(run_id, &file, output, &tracker)
}

fn oversized_workflow_changes(
    output: &tutor_agent::memory::MemoryWorkflowOutput,
    target_path: &str,
) -> Vec<serde_json::Value> {
    let limit = memory_entry_text_limit(target_path);
    output
        .changes
        .iter()
        .filter_map(|change| {
            let count = change.text.as_deref()?.chars().count();
            (count > limit).then(|| {
                serde_json::json!({
                    "changeId": change.id,
                    "characters": count,
                    "limit": limit,
                })
            })
        })
        .collect()
}

fn memory_evidence_tools(
    store: Arc<FileMemoryBackend>,
    tracker: MemoryEvidenceTracker,
    target_path: &str,
) -> Vec<Arc<dyn llm_harness_types::Tool>> {
    let mut tools: Vec<Arc<dyn llm_harness_types::Tool>> = Vec::new();
    if target_path.starts_with("L2/") || target_path == "L3/recent.md" {
        tools.extend([
            Arc::new(ListMemoryEventsTool::new(store.clone(), tracker.clone())) as Arc<_>,
            Arc::new(SearchMemoryEventsTool::new(store.clone(), tracker.clone())) as Arc<_>,
            Arc::new(ReadMemoryEventTool::new(store.clone(), tracker.clone())) as Arc<_>,
            Arc::new(ReadMemoryContextTool::new(store.clone(), tracker.clone())) as Arc<_>,
            Arc::new(ReadMemorySourceTool::new(store.clone(), tracker.clone())) as Arc<_>,
        ]);
    }
    if target_path.starts_with("L3/") {
        let paths = l3_source_paths(target_path);
        tools.extend([
            Arc::new(ListMemoryEntriesTool::new(
                store.clone(),
                tracker.clone(),
                paths.clone(),
            )) as Arc<_>,
            Arc::new(SearchMemoryEntriesTool::new(
                store.clone(),
                tracker.clone(),
                paths.clone(),
            )) as Arc<_>,
            Arc::new(ReadMemoryEntryTool::new(
                store.clone(),
                tracker.clone(),
                paths.clone(),
            )) as Arc<_>,
            Arc::new(ReadMemoryEntrySourcesTool::new(store, tracker, paths)) as Arc<_>,
        ]);
    }
    tools
}

fn l3_source_paths(target_path: &str) -> Vec<String> {
    let paths = match target_path {
        "L3/preferences.md" => vec!["L2/chat.md", "L2/notebook.md"],
        "L3/profile.md" | "L3/scope.md" | "L3/recent.md" | "L3/teaching_strategy.md" => vec![
            "L2/chat.md",
            "L2/quiz.md",
            "L2/notebook.md",
            "L2/knowledge.md",
        ],
        _ => Vec::new(),
    };
    paths.into_iter().map(str::to_string).collect()
}

fn unread_workflow_refs(
    output: &tutor_agent::memory::MemoryWorkflowOutput,
    tracker: &MemoryEvidenceTracker,
) -> Vec<String> {
    output
        .changes
        .iter()
        .flat_map(|change| change.refs.iter())
        .chain(
            output
                .findings
                .iter()
                .flat_map(|finding| finding.refs.iter()),
        )
        .filter(|reference| tracker.canonical_reference(reference).is_none())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
fn evidence_repair_input(
    input: &tutor_agent::memory::MemoryWorkflowInput,
    unread_refs: &[String],
) -> Result<tutor_agent::memory::MemoryWorkflowInput, String> {
    workflow_repair_input(input, unread_refs, &[])
}

fn workflow_repair_input(
    input: &tutor_agent::memory::MemoryWorkflowInput,
    unread_refs: &[String],
    oversized_changes: &[serde_json::Value],
) -> Result<tutor_agent::memory::MemoryWorkflowInput, String> {
    let mut context = serde_json::from_str::<serde_json::Value>(&input.consolidation_input_json)
        .map_err(|err| format!("invalid memory workflow context: {err}"))?;
    let evidence_action = if input.target_path.starts_with("L3/") {
        "Call read_memory_entry for every retained L2 reference below. Use read_memory_entry_sources only when source verification is needed, then resubmit the full result. Remove any claim whose evidence you do not read or that the full entry does not support."
    } else {
        "Call read_memory_event or read_memory_source for every retained reference below, inspect the complete event, then resubmit the full result. Remove any claim whose evidence you do not read or that the full event does not support."
    };
    let mut required_actions = Vec::new();
    if !unread_refs.is_empty() {
        required_actions.push(evidence_action);
    }
    if !oversized_changes.is_empty() {
        required_actions.push(
            "Shorten every oversized change or split it into multiple coherent evidence-bound changes. Do not truncate claims mechanically, merge unrelated claims, or omit their evidence references.",
        );
    }
    context["validationFeedback"] = serde_json::json!({
        "reason": "The previous draft failed the Memory change-set validation contract.",
        "unreadRefs": unread_refs,
        "oversizedChanges": oversized_changes,
        "requiredAction": required_actions.join(" "),
        "repairAttempt": 1,
    });
    let mut repair_input = input.clone();
    repair_input.consolidation_input_json =
        serde_json::to_string_pretty(&context).map_err(|err| err.to_string())?;
    Ok(repair_input)
}

fn workflow_output_to_change_set(
    run_id: String,
    file: &MemoryFile,
    output: tutor_agent::memory::MemoryWorkflowOutput,
    tracker: &MemoryEvidenceTracker,
) -> Result<MemoryChangeSet, String> {
    let entries = parse_memory_entries(&file.markdown);
    let mut changes = Vec::new();
    for change in output.changes {
        let refs = canonicalize_run_refs(&change.refs, tracker, true)?;
        validate_target_refs(&file.path, &refs)?;
        let text = change.text.map(|value| normalize_change_text(&value));
        if let Some(text) = text.as_deref() {
            let count = text.chars().count();
            let limit = memory_entry_text_limit(&file.path);
            if count > limit {
                return Err(format!(
                    "memory change `{}` has {count} characters, exceeding the {limit}-character limit for {}; shorten it or split it into multiple changes",
                    change.id, file.path
                ));
            }
        }
        let op = match change.op {
            tutor_agent::memory::MemoryWorkflowChangeOp::Insert => MemoryChangeOp::Insert,
            tutor_agent::memory::MemoryWorkflowChangeOp::Replace => MemoryChangeOp::Replace,
            tutor_agent::memory::MemoryWorkflowChangeOp::Delete => MemoryChangeOp::Delete,
        };
        let before_text = change.entry_id.as_deref().and_then(|entry_id| {
            entries
                .iter()
                .find(|entry| entry.marker == entry_id)
                .map(|entry| entry.text.clone())
        });
        if op != MemoryChangeOp::Insert && before_text.is_none() {
            return Err(format!(
                "memory change targets unknown entry `{}`",
                change.entry_id.as_deref().unwrap_or_default()
            ));
        }
        changes.push(MemoryChange {
            id: change.id,
            op,
            section: change.section,
            entry_id: change.entry_id,
            after_entry_id: change.after_entry_id,
            text,
            refs,
            reason: change.reason,
            before_text,
        });
    }
    let mut findings = Vec::new();
    for finding in output.findings {
        let refs = canonicalize_run_refs(&finding.refs, tracker, false)?;
        validate_target_refs(&file.path, &refs)?;
        findings.push(MemoryFinding {
            id: finding.id,
            entry_id: finding.entry_id,
            severity: finding.severity,
            kind: finding.kind,
            message: finding.message,
            refs,
        });
    }
    Ok(MemoryChangeSet {
        run_id,
        target_path: file.path.clone(),
        base_revision: file.revision.clone(),
        summary: output.summary,
        findings,
        changes,
    })
}

fn validate_target_refs(target_path: &str, refs: &[String]) -> Result<(), String> {
    if target_path.starts_with("L3/")
        && target_path != "L3/recent.md"
        && let Some(reference) = refs
            .iter()
            .find(|reference| !reference.starts_with("memory:L2/"))
    {
        return Err(format!(
            "ordinary L3 memory change must cite read L2 evidence, not `{reference}`"
        ));
    }
    Ok(())
}

fn canonicalize_run_refs(
    refs: &[String],
    tracker: &MemoryEvidenceTracker,
    required: bool,
) -> Result<Vec<String>, String> {
    if required && refs.is_empty() {
        return Err("memory change did not cite read evidence".into());
    }
    let mut canonical_refs = std::collections::BTreeSet::new();
    for reference in refs {
        let canonical = tracker
            .canonical_reference(reference)
            .ok_or_else(|| format!("memory change cites unread evidence `{reference}`"))?;
        canonical_refs.insert(canonical);
    }
    Ok(canonical_refs.into_iter().collect())
}

fn normalize_change_text(text: &str) -> String {
    text.trim()
        .strip_prefix("- ")
        .unwrap_or(text.trim())
        .split("<!--")
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn workflow_action(action: MemoryAssistAction) -> tutor_agent::memory::MemoryWorkflowAction {
    match action {
        MemoryAssistAction::Update => tutor_agent::memory::MemoryWorkflowAction::Update,
        MemoryAssistAction::Check => tutor_agent::memory::MemoryWorkflowAction::Check,
        MemoryAssistAction::Dedupe => tutor_agent::memory::MemoryWorkflowAction::Dedupe,
    }
}

async fn list_files(State(state): State<MemoryState>) -> impl IntoResponse {
    match state.store.list() {
        Ok(files) => (StatusCode::OK, Json(serde_json::json!({ "files": files }))).into_response(),
        Err(err) => error_response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn get_file(
    State(state): State<MemoryState>,
    Query(query): Query<MemoryFileQuery>,
) -> impl IntoResponse {
    match state.store.read(&query.path) {
        Ok(file) => (StatusCode::OK, Json(serde_json::json!({ "file": file }))).into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn update_file(
    State(state): State<MemoryState>,
    Query(query): Query<MemoryFileQuery>,
    Json(req): Json<UpdateMemoryFileRequest>,
) -> impl IntoResponse {
    match state.store.write(&query.path, req.markdown) {
        Ok(file) => (StatusCode::OK, Json(serde_json::json!({ "file": file }))).into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn list_events(
    State(state): State<MemoryState>,
    Query(query): Query<EventsQuery>,
) -> impl IntoResponse {
    match state
        .store
        .recent_events(query.limit.unwrap_or(50).clamp(1, 200))
    {
        Ok(events) => (
            StatusCode::OK,
            Json(serde_json::json!({ "events": events })),
        )
            .into_response(),
        Err(err) => error_response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn get_source(
    State(state): State<MemoryState>,
    Query(query): Query<SourceQuery>,
) -> impl IntoResponse {
    match state.store.resolve_source_ref(&query.reference) {
        Ok(source) => (
            StatusCode::OK,
            Json(serde_json::json!({ "source": source })),
        )
            .into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn undo_memory(
    State(state): State<MemoryState>,
    Json(req): Json<UndoMemoryRequest>,
) -> impl IntoResponse {
    match state.store.undo_latest_write(&req.target_path) {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({ "result": result })),
        )
            .into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

fn build_llm_config(config: MemoryLlmConfig) -> Result<LlmConfig, String> {
    let provider = match config.provider.trim().to_ascii_lowercase().as_str() {
        "anthropic" => LlmProviderKind::Anthropic,
        "deepseek" => LlmProviderKind::DeepSeek,
        "openai" | "openai-compatible" => LlmProviderKind::OpenAI,
        _ => return Err("unsupported LLM provider".into()),
    };
    let model = config.model.trim().to_string();
    if model.is_empty() {
        return Err("memory workflow requires a model".into());
    }
    let api_key = config
        .api_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "memory workflow requires an API key".to_string())?;
    Ok(LlmConfig::from_parts(
        provider,
        model,
        api_key,
        config
            .base_url
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        config
            .chat_path
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        config.context_window_tokens.filter(|value| *value > 0),
    ))
}

async fn run_memory_runtime_workflow_with_tools(
    llm: &LlmConfig,
    workflow_root: &Path,
    input: &tutor_agent::memory::MemoryWorkflowInput,
    tools: Vec<Arc<dyn llm_harness_types::Tool>>,
) -> tutor_agent::Result<tutor_agent::memory::MemoryWorkflowRun> {
    let cwd = std::env::current_dir()
        .map_err(|err| tutor_agent::TutorError::Internal(err.to_string()))?;
    let env = Arc::new(OsEnv::new(cwd)) as Arc<dyn ExecutionEnv>;
    let client = llm.build_client();
    let engine_config = tutor_agent::runtime_engine::build_workflow_engine_config(
        client,
        llm.model.clone(),
        env,
        workflow_root.join("memory"),
    );
    tutor_agent::memory::run_memory_workflow_with_tools(input, engine_config, tools).await
}

pub fn memory_router(store: Arc<FileMemoryBackend>, workflow_root: impl Into<PathBuf>) -> Router {
    let state = MemoryState {
        store,
        workflow_root: workflow_root.into(),
        runs: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        tasks: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
    };
    Router::new()
        .route("/api/memory/files", get(list_files))
        .route("/api/memory/file", get(get_file).patch(update_file))
        .route("/api/memory/events", get(list_events))
        .route("/api/memory/source", get(get_source))
        .route("/api/memory/undo", axum::routing::post(undo_memory))
        .route(
            "/api/memory/runs",
            get(list_memory_runs).post(start_memory_run),
        )
        .route(
            "/api/memory/runs/{run_id}",
            get(get_memory_run).delete(cancel_memory_run),
        )
        .route(
            "/api/memory/runs/{run_id}/apply",
            axum::routing::post(apply_memory_run),
        )
        .with_state(state)
}

fn error_response(status: StatusCode, message: String) -> axum::response::Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request};
    use tower::ServiceExt;

    #[test]
    fn memory_run_request_captures_output_language_with_legacy_default() {
        let request = serde_json::from_value::<StartMemoryRunRequest>(serde_json::json!({
            "target_path": "L2/chat.md",
            "action": "update",
            "output_language": "zh-CN",
            "llm": {
                "provider": "openai",
                "model": "test-model"
            }
        }))
        .unwrap();
        assert_eq!(request.output_language, MemoryOutputLanguage::ZhCn);

        let legacy = serde_json::from_value::<StartMemoryRunRequest>(serde_json::json!({
            "target_path": "L2/chat.md",
            "action": "update",
            "llm": {
                "provider": "openai",
                "model": "test-model"
            }
        }))
        .unwrap();
        assert_eq!(legacy.output_language, MemoryOutputLanguage::EnUs);
    }

    #[test]
    fn canonicalizes_read_product_aliases_and_rejects_unread_aliases() {
        let tracker = MemoryEvidenceTracker::default();
        let alias = "notebook:4747cc47-597a-410b-a073-5881480bb4c6";
        let canonical = "notebook:7b2cd73e-2f84-49fb-8600-03d67fe088d4";
        tracker.record_resolution(
            "reading_evidence",
            "read_memory_source",
            "Resolved notebook evidence".into(),
            alias,
            canonical,
        );

        assert_eq!(
            canonicalize_run_refs(&[alias.into()], &tracker, true).unwrap(),
            vec![canonical.to_string()]
        );
        let error = canonicalize_run_refs(&["notebook:unread".into()], &tracker, true).unwrap_err();
        assert!(error.contains("cites unread evidence `notebook:unread`"));
    }

    #[test]
    fn routes_memory_tools_by_target_layer() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        let tracker = MemoryEvidenceTracker::default();

        let l2 = memory_evidence_tools(store.clone(), tracker.clone(), "L2/chat.md")
            .iter()
            .map(|tool| tool.name().to_string())
            .collect::<Vec<_>>();
        assert!(l2.iter().any(|name| name == "read_memory_event"));
        assert!(!l2.iter().any(|name| name == "read_memory_entry"));

        let profile = memory_evidence_tools(store.clone(), tracker.clone(), "L3/profile.md")
            .iter()
            .map(|tool| tool.name().to_string())
            .collect::<Vec<_>>();
        assert!(profile.iter().any(|name| name == "read_memory_entry"));
        assert!(!profile.iter().any(|name| name == "read_memory_event"));

        let recent = memory_evidence_tools(store, tracker, "L3/recent.md")
            .iter()
            .map(|tool| tool.name().to_string())
            .collect::<Vec<_>>();
        assert!(recent.iter().any(|name| name == "read_memory_entry"));
        assert!(recent.iter().any(|name| name == "read_memory_event"));
    }

    #[test]
    fn ordinary_l3_rejects_direct_l1_references() {
        assert!(validate_target_refs("L3/profile.md", &["chat:event-1".into()]).is_err());
        assert!(
            validate_target_refs("L3/profile.md", &["memory:L2/chat.md#m_example".into()]).is_ok()
        );
        assert!(validate_target_refs("L3/recent.md", &["chat:event-1".into()]).is_ok());
    }

    #[test]
    fn l3_evidence_repair_requests_the_available_l2_read_tool() {
        let input = tutor_agent::memory::MemoryWorkflowInput {
            target_path: "L3/profile.md".into(),
            action: tutor_agent::memory::MemoryWorkflowAction::Update,
            output_language: MemoryOutputLanguage::EnUs,
            current_markdown: "# Student profile".into(),
            consolidation_input_json: "{}".into(),
        };

        let repair =
            evidence_repair_input(&input, &["memory:L2/chat.md#m_candidate".into()]).unwrap();

        assert!(
            repair
                .consolidation_input_json
                .contains("read_memory_entry")
        );
        assert!(
            !repair
                .consolidation_input_json
                .contains("read_memory_event")
        );
    }

    #[test]
    fn oversized_l3_change_requests_one_split_repair_pass() {
        let input = tutor_agent::memory::MemoryWorkflowInput {
            target_path: "L3/profile.md".into(),
            action: tutor_agent::memory::MemoryWorkflowAction::Update,
            output_language: MemoryOutputLanguage::EnUs,
            current_markdown: "# Student profile".into(),
            consolidation_input_json: "{}".into(),
        };
        let output = tutor_agent::memory::MemoryWorkflowOutput {
            summary: "Profile synthesis".into(),
            findings: vec![],
            changes: vec![tutor_agent::memory::MemoryWorkflowChange {
                id: "change_long".into(),
                op: tutor_agent::memory::MemoryWorkflowChangeOp::Insert,
                section: Some("Strengths".into()),
                entry_id: None,
                after_entry_id: None,
                text: Some("x".repeat(1_201)),
                refs: vec!["memory:L2/chat.md#m_source".into()],
                reason: "Evidence-backed synthesis".into(),
            }],
        };

        let oversized = oversized_workflow_changes(&output, &input.target_path);
        let repair = workflow_repair_input(&input, &[], &oversized).unwrap();

        assert_eq!(oversized[0]["changeId"], "change_long");
        assert_eq!(oversized[0]["limit"], 1_200);
        assert!(
            repair
                .consolidation_input_json
                .contains("split it into multiple")
        );
    }

    #[test]
    fn collects_unread_workflow_refs_once_for_evidence_repair() {
        let tracker = MemoryEvidenceTracker::default();
        tracker.record_resolution(
            "reading_evidence",
            "read_memory_event",
            "Read quiz evidence".into(),
            "quiz:read",
            "quiz:read",
        );
        let output = serde_json::from_value::<tutor_agent::memory::MemoryWorkflowOutput>(
            serde_json::json!({
                "summary": "draft",
                "changes": [{
                    "id": "change_1",
                    "op": "insert",
                    "section": "Topics",
                    "entry_id": null,
                    "after_entry_id": null,
                    "text": "RAG",
                    "refs": ["quiz:read", "quiz:unread"],
                    "reason": "test"
                }],
                "findings": [{
                    "id": "finding_1",
                    "entry_id": null,
                    "severity": "warning",
                    "kind": "evidence",
                    "message": "needs repair",
                    "refs": ["quiz:unread"]
                }]
            }),
        )
        .unwrap();

        assert_eq!(unread_workflow_refs(&output, &tracker), vec!["quiz:unread"]);
    }

    #[test]
    fn repair_input_requires_missing_refs_to_be_read_or_removed() {
        let input = tutor_agent::memory::MemoryWorkflowInput {
            target_path: "L2/quiz.md".into(),
            action: tutor_agent::memory::MemoryWorkflowAction::Update,
            output_language: MemoryOutputLanguage::ZhCn,
            current_markdown: "# Quiz memory".into(),
            consolidation_input_json: serde_json::json!({ "target": { "path": "L2/quiz.md" } })
                .to_string(),
        };

        let repaired = evidence_repair_input(&input, &["quiz:unread".into()]).unwrap();
        let context =
            serde_json::from_str::<serde_json::Value>(&repaired.consolidation_input_json).unwrap();

        assert_eq!(context["validationFeedback"]["repairAttempt"], 1);
        assert_eq!(
            context["validationFeedback"]["unreadRefs"],
            serde_json::json!(["quiz:unread"])
        );
        assert!(
            context["validationFeedback"]["requiredAction"]
                .as_str()
                .unwrap()
                .contains("read_memory_event")
        );
    }

    #[tokio::test]
    async fn cancellation_aborts_and_records_the_memory_run_state() {
        let dir = tempfile::tempdir().unwrap();
        let run_id = "run-to-cancel".to_string();
        let state = MemoryState {
            store: Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory"))),
            workflow_root: dir.path().join("workflow-sessions"),
            runs: Arc::new(tokio::sync::RwLock::new(HashMap::from([(
                run_id.clone(),
                MemoryRunSnapshot {
                    run_id: run_id.clone(),
                    target_path: "L2/chat.md".into(),
                    action: MemoryAssistAction::Update,
                    output_language: MemoryOutputLanguage::EnUs,
                    started_at: chrono::Utc::now(),
                    status: "running".into(),
                    current_stage: "discovering_sources".into(),
                    flow: vec![],
                    change_set: None,
                    error: None,
                },
            )]))),
            tasks: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        };
        let task = tokio::spawn(std::future::pending::<()>());
        state
            .tasks
            .write()
            .await
            .insert(run_id.clone(), task.abort_handle());

        let response = cancel_memory_run(State(state.clone()), AxumPath(run_id.clone()))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let run = state.runs.read().await.get(&run_id).cloned().unwrap();
        assert_eq!(run.status, "cancelled");
        assert_eq!(run.current_stage, "cancelled");
        assert_eq!(run.flow.last().unwrap().stage, "cancelled");
        assert!(state.tasks.read().await.is_empty());
    }

    #[tokio::test]
    async fn active_memory_runs_are_listed_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let now = chrono::Utc::now();
        let snapshot = |run_id: &str, status: &str, started_at| MemoryRunSnapshot {
            run_id: run_id.into(),
            target_path: "L2/chat.md".into(),
            action: MemoryAssistAction::Update,
            output_language: MemoryOutputLanguage::EnUs,
            started_at,
            status: status.into(),
            current_stage: status.into(),
            flow: vec![],
            change_set: None,
            error: None,
        };
        let state = MemoryState {
            store: Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory"))),
            workflow_root: dir.path().join("workflow-sessions"),
            runs: Arc::new(tokio::sync::RwLock::new(HashMap::from([
                (
                    "older-running".into(),
                    snapshot(
                        "older-running",
                        "running",
                        now - chrono::Duration::seconds(5),
                    ),
                ),
                (
                    "newer-review".into(),
                    snapshot("newer-review", "awaiting_review", now),
                ),
                (
                    "completed".into(),
                    snapshot("completed", "completed", now + chrono::Duration::seconds(5)),
                ),
            ]))),
            tasks: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        };

        let response = list_memory_runs(
            State(state),
            Query(MemoryRunsQuery {
                active_only: Some(true),
            }),
        )
        .await
        .into_response();
        let body = response_json(response).await;
        let runs = body["runs"].as_array().unwrap();

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0]["run_id"], "newer-review");
        assert_eq!(runs[1]["run_id"], "older-running");
    }

    #[tokio::test]
    async fn lists_and_updates_memory_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        let app = memory_router(store, dir.path().join("workflow-sessions"));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/memory/files")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert!(body["files"].as_array().unwrap().len() >= 3);

        let response = app
            .oneshot(json_request(
                Method::PATCH,
                "/api/memory/file?path=L3%2Fprofile.md",
                serde_json::json!({ "markdown": "# Student profile\n\n- Needs review. <!--m_01-->" }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert!(
            body["file"]["markdown"]
                .as_str()
                .unwrap()
                .contains("Needs review")
        );
    }

    #[tokio::test]
    async fn undo_restores_latest_memory_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        store
            .write("L2/chat.md", "# Chat memory\n\n- Original.".into())
            .unwrap();
        let app = memory_router(store, dir.path().join("workflow-sessions"));

        let response = app
            .clone()
            .oneshot(json_request(
                Method::PATCH,
                "/api/memory/file?path=L2%2Fchat.md",
                serde_json::json!({ "markdown": "# Chat memory\n\n- Changed." }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(json_request(
                Method::POST,
                "/api/memory/undo",
                serde_json::json!({ "target_path": "L2/chat.md" }),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert!(
            body["result"]["file"]["markdown"]
                .as_str()
                .unwrap()
                .contains("Original")
        );
    }

    #[tokio::test]
    async fn resolves_memory_source_refs() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        store
            .record_event(
                crate::memory_store::MemoryEventCategory::Quiz,
                "answered",
                "Answered OPC question correctly",
                Some("quiz-1".into()),
                serde_json::json!({ "question_id": "q1" }),
            )
            .unwrap();
        let app = memory_router(store, dir.path().join("workflow-sessions"));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/memory/source?reference=quiz%3Aquiz-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["source"]["reference"], "quiz:quiz-1");
        assert_eq!(
            body["source"]["event"]["summary"],
            "Answered OPC question correctly"
        );
    }

    #[tokio::test]
    async fn retired_memory_generation_routes_are_not_mounted() {
        let dir = tempfile::tempdir().unwrap();
        let app = memory_router(
            Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory"))),
            dir.path().join("workflow-sessions"),
        );

        for uri in [
            "/api/memory/assist",
            "/api/memory/consolidate/preview",
            "/api/memory/consolidate/apply",
        ] {
            let response = app
                .clone()
                .oneshot(json_request(Method::POST, uri, serde_json::json!({})))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        }
    }

    fn json_request(method: Method, uri: &str, value: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    }

    async fn response_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}

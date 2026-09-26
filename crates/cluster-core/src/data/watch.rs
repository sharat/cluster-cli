//! Long-lived `kubectl get --watch` streams that push object changes between
//! polls. Each watch is a `kubectl get` (so the read-only whitelist still
//! applies) that restarts with backoff when kubectl exits — the API server
//! closes watches periodically. Changes missed while a watch is down are
//! healed by the fetcher's next full poll, which replaces the store.

use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::debug;

use crate::data::collector::{self, ensure_readonly_kubectl_args, expand_all_namespaces};

const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// A watch that stayed up this long failed for a new reason; retry promptly.
const HEALTHY_RUN: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchKind {
    Nodes,
    Pods,
    Events,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchAction {
    Upsert,
    Delete,
}

#[derive(Debug)]
pub struct WatchUpdate {
    /// Watches from before a namespace switch keep draining for a moment;
    /// the fetcher drops updates whose generation is not current.
    pub generation: u64,
    pub kind: WatchKind,
    pub action: WatchAction,
    pub object: Value,
}

/// Starts one watch per [`WatchKind`] on `set`. Dropping or aborting the set
/// kills the kubectl processes. `context` pins the watches with `--context`
/// so a `kubectl config use-context` elsewhere cannot redirect a long-lived
/// stream to another cluster; `None` falls back to the task's context
/// override, then kubectl's current context.
pub fn spawn_watches(
    set: &mut JoinSet<()>,
    context: Option<&str>,
    namespace: &str,
    generation: u64,
    tx: &mpsc::Sender<WatchUpdate>,
) {
    let context = context
        .map(str::to_string)
        .or_else(collector::context_override);
    for kind in [WatchKind::Nodes, WatchKind::Pods, WatchKind::Events] {
        let args = watch_args(kind, namespace);
        set.spawn(watch_loop(
            kind,
            context.clone(),
            args,
            generation,
            tx.clone(),
        ));
    }
}

fn watch_args(kind: WatchKind, namespace: &str) -> Vec<String> {
    let mut args = vec!["get".to_string()];
    match kind {
        WatchKind::Nodes => args.push("nodes".to_string()),
        WatchKind::Pods => args.extend(["pods", "-n", namespace].map(str::to_string)),
        WatchKind::Events => args.extend(["events", "-n", namespace].map(str::to_string)),
    }
    // The fetcher lists before watching, so skip the initial list here.
    args.extend(["-o", "json", "--watch-only", "--output-watch-events"].map(str::to_string));
    args
}

// Logged at debug only: the TUI's stderr subscriber would draw over the
// alternate screen, and polls already surface list failures in the UI.
async fn watch_loop(
    kind: WatchKind,
    context: Option<String>,
    args: Vec<String>,
    generation: u64,
    tx: mpsc::Sender<WatchUpdate>,
) {
    let mut backoff = MIN_BACKOFF;
    loop {
        let started = tokio::time::Instant::now();
        match run_watch(kind, context.as_deref(), &args, generation, &tx).await {
            Ok(()) => debug!(?kind, "kubectl watch ended; restarting"),
            Err(err) if is_permanent_watch_error(&format!("{err:#}")) => {
                // e.g. a namespace-scoped user watching nodes. Retrying cannot
                // help; the poll keeps reporting the list failure.
                debug!(?kind, "kubectl watch not permitted; stopping: {err:#}");
                return;
            }
            Err(err) => debug!(?kind, "kubectl watch failed: {err:#}"),
        }
        if tx.is_closed() {
            return;
        }
        if started.elapsed() >= HEALTHY_RUN {
            backoff = MIN_BACKOFF;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

fn is_permanent_watch_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("forbidden")
        || message.contains("unauthorized")
        || message.contains("doesn't have a resource type")
}

async fn run_watch(
    kind: WatchKind,
    context: Option<&str>,
    args: &[String],
    generation: u64,
    tx: &mpsc::Sender<WatchUpdate>,
) -> Result<()> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let args = expand_all_namespaces(&args);
    ensure_readonly_kubectl_args("kubectl", &args)?;

    let mut command = Command::new("kubectl");
    if let Some(context) = context {
        command.args(["--context", context]);
    }
    let mut child = command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("failed to start kubectl watch")?;
    let mut stdout = BufReader::new(child.stdout.take().context("kubectl stdout missing")?);
    let mut stderr = child.stderr.take().context("kubectl stderr missing")?;
    let stderr_task = tokio::spawn(async move {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text).await;
        text
    });

    let mut buf = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let read = stdout.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
        for value in drain_json_values(&mut buf)? {
            let Some((action, object)) = parse_watch_event(value)? else {
                continue;
            };
            let update = WatchUpdate {
                generation,
                kind,
                action,
                object,
            };
            if tx.send(update).await.is_err() {
                return Ok(());
            }
        }
    }

    let status = child.wait().await?;
    let stderr = stderr_task.await.unwrap_or_default();
    if !status.success() {
        bail!("kubectl exited with {status}: {}", stderr.trim());
    }
    Ok(())
}

/// Removes every complete JSON value from the front of `buf`, leaving a
/// trailing partial value for the next read. `kubectl -o json --watch`
/// prints pretty-printed objects back to back, not one per line.
fn drain_json_values(buf: &mut Vec<u8>) -> Result<Vec<Value>> {
    let mut values = Vec::new();
    let mut stream = serde_json::Deserializer::from_slice(buf).into_iter::<Value>();
    let mut consumed = 0;
    loop {
        match stream.next() {
            Some(Ok(value)) => {
                values.push(value);
                consumed = stream.byte_offset();
            }
            Some(Err(err)) if err.is_eof() => break,
            Some(Err(err)) => return Err(err).context("malformed kubectl watch output"),
            None => {
                consumed = stream.byte_offset();
                break;
            }
        }
    }
    buf.drain(..consumed);
    Ok(values)
}

/// Maps a `{"type": ..., "object": ...}` watch event to a store change.
/// An `ERROR` event (e.g. `410 Gone` for an expired resource version) ends
/// the watch so it restarts from the current state.
fn parse_watch_event(mut value: Value) -> Result<Option<(WatchAction, Value)>> {
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let object = value.get_mut("object").map(Value::take).unwrap_or_default();
    Ok(match event_type.as_str() {
        "ADDED" | "MODIFIED" => Some((WatchAction::Upsert, object)),
        "DELETED" => Some((WatchAction::Delete, object)),
        "ERROR" => bail!(
            "watch error: {}",
            object
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drains_back_to_back_values_across_chunk_boundaries() {
        let first =
            "{\n  \"type\": \"ADDED\",\n  \"object\": {\"metadata\": {\"name\": \"a\"}}\n}\n";
        let second = "{\"type\": \"DELETED\", \"object\": {\"metadata\": {\"name\": \"b\"}}}";
        let stream = format!("{first}{second}");
        let (head, tail) = stream.split_at(first.len() + 10);

        let mut buf = head.as_bytes().to_vec();
        let values = drain_json_values(&mut buf).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0]["object"]["metadata"]["name"], "a");

        buf.extend_from_slice(tail.as_bytes());
        let values = drain_json_values(&mut buf).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0]["type"], "DELETED");
        assert!(buf.iter().all(u8::is_ascii_whitespace));
    }

    #[test]
    fn malformed_output_is_an_error() {
        let mut buf = b"{\"type\": ]".to_vec();
        assert!(drain_json_values(&mut buf).is_err());
    }

    #[test]
    fn maps_watch_event_types() {
        let object = json!({"metadata": {"name": "a"}});
        let event = |kind: &str| json!({"type": kind, "object": object.clone()});

        let (action, parsed) = parse_watch_event(event("MODIFIED")).unwrap().unwrap();
        assert_eq!(action, WatchAction::Upsert);
        assert_eq!(parsed, object);
        assert_eq!(
            parse_watch_event(event("DELETED")).unwrap().unwrap().0,
            WatchAction::Delete
        );
        assert!(parse_watch_event(event("BOOKMARK")).unwrap().is_none());
        assert!(
            parse_watch_event(json!({"type": "ERROR", "object": {"message": "Gone"}})).is_err()
        );
    }

    #[test]
    fn rbac_denials_stop_the_watch() {
        assert!(is_permanent_watch_error(
            "kubectl exited with exit status: 1: Error from server (Forbidden): nodes is forbidden"
        ));
        assert!(is_permanent_watch_error(
            "error: You must be logged in (Unauthorized)"
        ));
        assert!(!is_permanent_watch_error(
            "kubectl exited with exit status: 1: Unable to connect to the server: dial tcp: i/o timeout"
        ));
    }

    #[test]
    fn watch_args_are_read_only_and_expand_all_namespaces() {
        let args = watch_args(WatchKind::Pods, "*");
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let expanded = expand_all_namespaces(&args);
        assert_eq!(
            expanded,
            vec![
                "get",
                "pods",
                "--all-namespaces",
                "-o",
                "json",
                "--watch-only",
                "--output-watch-events"
            ]
        );
        assert!(ensure_readonly_kubectl_args("kubectl", &expanded).is_ok());
    }
}

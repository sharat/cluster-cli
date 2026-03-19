use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppState, AppView, Overlay, Panel, PodDetailSection};
use crate::events::{DataEvent, FetchCommand};

// Kubernetes namespace names are limited to 63 characters (RFC 1123)
const MAX_NAMESPACE_LEN: usize = 63;

pub enum AppCommand {
    Quit,
    Fetch(FetchCommand),
}

pub fn handle_key(app: &mut AppState, key: KeyEvent) -> Option<AppCommand> {
    let view = app.view.clone();
    match view {
        AppView::Dashboard => handle_dashboard_key(app, key),
        AppView::PodDetail { .. } => handle_pod_detail_key(app, key),
        AppView::NodeDetail { .. } => handle_node_detail_key(app, key),
    }
}

fn handle_dashboard_key(app: &mut AppState, key: KeyEvent) -> Option<AppCommand> {
    match app.overlay.clone() {
        Overlay::WorkloadPopup => {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('w') => {
                    app.overlay = Overlay::None;
                    app.clear_incident_focus();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    let len = app
                        .snapshot
                        .as_ref()
                        .map(|s| s.workloads.len())
                        .unwrap_or(0);
                    if app.workload_cursor + 1 < len {
                        app.workload_cursor += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if app.workload_cursor > 0 {
                        app.workload_cursor -= 1;
                    }
                }
                _ => {}
            }
            return None;
        }

        Overlay::NamespaceList => {
            match key.code {
                KeyCode::Esc => {
                    app.overlay = Overlay::None;
                    app.ns_list.clear();
                    app.clear_incident_focus();
                }
                KeyCode::Enter => {
                    if let Some(ns) = app.ns_list.get(app.ns_list_cursor).cloned() {
                        app.config.namespace = ns;
                        app.pod_cursor = 0;
                        app.node_cursor = 0;
                        app.workload_cursor = 0;
                        app.event_cursor = 0;
                        app.is_loading = true;
                        app.overlay = Overlay::None;
                        app.ns_list.clear();
                        app.clear_incident_focus();
                        return Some(AppCommand::Fetch(FetchCommand::RefreshAll {
                            namespace: app.config.namespace.clone(),
                        }));
                    }
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if app.ns_list_cursor + 1 < app.ns_list.len() {
                        app.ns_list_cursor += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if app.ns_list_cursor > 0 {
                        app.ns_list_cursor -= 1;
                    }
                }
                _ => {}
            }
            return None;
        }

        Overlay::NamespaceInput => {
            match key.code {
                KeyCode::Esc => {
                    app.overlay = Overlay::None;
                    app.ns_input.clear();
                    app.clear_incident_focus();
                }
                KeyCode::Enter => {
                    let ns = app.ns_input.trim().to_string();
                    if !ns.is_empty() {
                        app.config.namespace = ns;
                        app.pod_cursor = 0;
                        app.node_cursor = 0;
                        app.workload_cursor = 0;
                        app.event_cursor = 0;
                        app.is_loading = true;
                        app.clear_incident_focus();
                    }
                    app.overlay = Overlay::None;
                    app.ns_input.clear();
                    if app.is_loading {
                        return Some(AppCommand::Fetch(FetchCommand::RefreshAll {
                            namespace: app.config.namespace.clone(),
                        }));
                    }
                }
                KeyCode::Char(c) if app.ns_input.len() < MAX_NAMESPACE_LEN => {
                    app.ns_input.push(c);
                }
                KeyCode::Backspace => {
                    app.ns_input.pop();
                }
                _ => {}
            }
            return None;
        }

        Overlay::RefreshInput => {
            match key.code {
                KeyCode::Esc => {
                    app.overlay = Overlay::None;
                    app.refresh_input.clear();
                }
                KeyCode::Enter => {
                    let parsed = app
                        .refresh_input
                        .trim()
                        .parse::<u64>()
                        .ok()
                        .filter(|secs| *secs > 0);
                    app.overlay = Overlay::None;
                    app.refresh_input.clear();

                    if let Some(interval_secs) = parsed {
                        app.config.refresh_interval_secs = interval_secs;
                        app.is_loading = true;
                        return Some(AppCommand::Fetch(FetchCommand::UpdateRefreshInterval {
                            namespace: app.config.namespace.clone(),
                            interval_secs,
                        }));
                    }
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    app.refresh_input.push(c);
                }
                KeyCode::Backspace => {
                    app.refresh_input.pop();
                }
                _ => {}
            }
            return None;
        }

        Overlay::ExportInput => {
            match key.code {
                KeyCode::Esc => {
                    app.overlay = Overlay::None;
                    app.export_input.clear();
                }
                KeyCode::Enter => {
                    let path = app.export_input.trim().to_string();
                    let namespace = app.config.namespace.clone();
                    let cluster_name = app.snapshot.as_ref().and_then(|s| s.context_name.clone());
                    app.overlay = Overlay::None;
                    app.export_input.clear();
                    if !path.is_empty() {
                        return Some(AppCommand::Fetch(FetchCommand::ExportPods {
                            cluster_name,
                            namespace,
                            path,
                        }));
                    }
                }
                KeyCode::Char(c) => {
                    app.export_input.push(c);
                }
                KeyCode::Backspace => {
                    app.export_input.pop();
                }
                _ => {}
            }
            return None;
        }

        Overlay::PodFilter => {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    app.overlay = Overlay::None;
                    let count = app.filtered_pod_count();
                    app.pod_cursor = if count == 0 {
                        0
                    } else {
                        app.pod_cursor.min(count - 1)
                    };
                }
                KeyCode::Char(c) => {
                    app.pod_filter.push(c);
                    app.pod_cursor = 0;
                }
                KeyCode::Backspace => {
                    app.pod_filter.pop();
                    app.pod_cursor = 0;
                }
                _ => {}
            }
            return None;
        }

        Overlay::None => {} // fall through to default key handling
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return Some(AppCommand::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Some(AppCommand::Quit);
        }
        KeyCode::Tab => {
            app.focused_panel = app.focused_panel.next();
        }
        KeyCode::Char('1') => {
            app.focused_panel = Panel::Nodes;
        }
        KeyCode::Char('2') => {
            app.focused_panel = Panel::Events;
        }
        KeyCode::Char('3') => {
            app.focused_panel = Panel::Pods;
        }
        KeyCode::Char('N') => {
            app.ns_input = app.config.namespace.clone();
            app.overlay = Overlay::NamespaceInput;
        }
        KeyCode::Char('n') => {
            app.overlay = Overlay::NamespaceList;
            app.ns_list_cursor = 0;
            return Some(AppCommand::Fetch(FetchCommand::FetchNamespaces));
        }
        KeyCode::Char('r') => {
            app.is_loading = true;
            return Some(AppCommand::Fetch(FetchCommand::RefreshAll {
                namespace: app.config.namespace.clone(),
            }));
        }
        KeyCode::Char('R') => {
            app.refresh_input = app.config.refresh_interval_secs.to_string();
            app.overlay = Overlay::RefreshInput;
        }
        KeyCode::Char('w') => {
            app.overlay = if app.overlay == Overlay::WorkloadPopup {
                Overlay::None
            } else {
                Overlay::WorkloadPopup
            };
        }
        KeyCode::Char('s') => {
            app.cycle_pod_sort_mode();
        }
        KeyCode::Char('E') => {
            let cluster_name = app.snapshot.as_ref().and_then(|s| s.context_name.clone());
            let ns = &app.config.namespace;
            let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let filename = match &cluster_name {
                Some(cluster) => format!("{cluster}-{ns}-{timestamp}.csv"),
                None => format!("{ns}-{timestamp}.csv"),
            };
            app.export_input = filename;
            app.overlay = Overlay::ExportInput;
        }
        KeyCode::Esc => {
            if app.incident_focus.is_some() {
                app.clear_incident_focus();
            } else if app.connection_issue.is_some() {
                // Allow quitting with Esc when there's a connection error
                return Some(AppCommand::Quit);
            }
        }
        KeyCode::Char('/') => {
            app.overlay = Overlay::PodFilter;
        }
        KeyCode::Char('j') | KeyCode::Down => match app.focused_panel {
            Panel::Nodes => {
                let len = app.snapshot.as_ref().map(|s| s.nodes.len()).unwrap_or(0);
                if app.node_cursor + 1 < len {
                    app.node_cursor += 1;
                }
            }
            Panel::Pods => {
                let count = app.filtered_pod_count();
                if app.pod_cursor + 1 < count {
                    app.pod_cursor += 1;
                }
            }
            Panel::Events => {
                let len = app
                    .snapshot
                    .as_ref()
                    .map(|s| s.incident_buckets.len())
                    .unwrap_or(0);
                if app.event_cursor + 1 < len {
                    app.event_cursor += 1;
                }
            }
        },
        KeyCode::Char('k') | KeyCode::Up => match app.focused_panel {
            Panel::Nodes => {
                if app.node_cursor > 0 {
                    app.node_cursor -= 1;
                }
            }
            Panel::Pods => {
                if app.pod_cursor > 0 {
                    app.pod_cursor -= 1;
                }
            }
            Panel::Events => {
                if app.event_cursor > 0 {
                    app.event_cursor -= 1;
                }
            }
        },
        KeyCode::Enter => {
            if app.focused_panel == Panel::Pods {
                let pod_info = {
                    let pods = app.filtered_pods();
                    pods.get(app.pod_cursor)
                        .map(|p| (p.name.clone(), p.namespace.clone()))
                };
                if let Some((pod_name, ns)) = pod_info {
                    app.enter_pod_detail(pod_name.clone());
                    return Some(AppCommand::Fetch(FetchCommand::StartLogStream {
                        pod: pod_name,
                        namespace: ns,
                    }));
                }
            } else if app.focused_panel == Panel::Nodes {
                let node_info = app
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.nodes.get(app.node_cursor).map(|n| n.name.clone()));
                if let Some(node_name) = node_info {
                    app.view = AppView::NodeDetail { node_name };
                    app.detail_scroll = 0;
                }
            } else if app.focused_panel == Panel::Events {
                return handle_incident_enter(app);
            }
        }
        _ => {}
    }

    None
}

fn handle_incident_enter(app: &mut AppState) -> Option<AppCommand> {
    let bucket = app.selected_incident()?.clone();
    let mut pod_names = std::collections::BTreeSet::new();
    let mut node_names = std::collections::BTreeSet::new();
    let mut workload_keys = std::collections::BTreeSet::new();

    for target in &bucket.targets {
        if let Some(pod_name) = target.pod_name() {
            pod_names.insert(pod_name.to_string());
        }
        if let Some(node_name) = target.node_name() {
            node_names.insert(node_name.to_string());
        }
        if let Some((kind, name)) = target.workload_key() {
            workload_keys.insert((kind.to_string(), name.to_string()));
        }
    }

    if pod_names.len() == 1 && node_names.is_empty() && workload_keys.is_empty() {
        if let Some(pod_name) = pod_names.iter().next().cloned() {
            let namespace = pod_namespace(app, &pod_name);
            app.enter_pod_detail(pod_name.clone());
            app.focused_panel = Panel::Pods;
            return Some(AppCommand::Fetch(FetchCommand::StartLogStream {
                pod: pod_name,
                namespace,
            }));
        }
    }

    if node_names.len() == 1 && pod_names.is_empty() && workload_keys.is_empty() {
        if let Some(node_name) = node_names.iter().next().cloned() {
            app.view = AppView::NodeDetail { node_name };
            app.detail_scroll = 0;
            return None;
        }
    }

    if workload_keys.len() == 1 && pod_names.is_empty() && node_names.is_empty() {
        if let Some((kind, name)) = workload_keys.iter().next().cloned() {
            if let Some(index) = resolve_workload_index(app, &kind, &name) {
                app.workload_cursor = index;
            }
            app.overlay = Overlay::WorkloadPopup;
            return None;
        }
    }

    if !pod_names.is_empty() || !workload_keys.is_empty() {
        app.apply_incident_focus(&bucket);
        app.focused_panel = Panel::Pods;
        return None;
    }

    if let Some(node_name) = node_names.iter().next().cloned() {
        app.view = AppView::NodeDetail { node_name };
        app.detail_scroll = 0;
    }

    None
}

fn pod_namespace(app: &AppState, pod_name: &str) -> String {
    app.snapshot
        .as_ref()
        .and_then(|snapshot| {
            snapshot
                .pods
                .iter()
                .find(|pod| pod.name == pod_name)
                .map(|pod| pod.namespace.clone())
        })
        .unwrap_or_else(|| app.config.namespace.clone())
}

fn resolve_workload_index(app: &AppState, kind: &str, name: &str) -> Option<usize> {
    let snapshot = app.snapshot.as_ref()?;
    snapshot.workloads.iter().position(|workload| {
        (workload.kind.as_str() == kind && workload.name == name)
            || workload
                .related_event_targets
                .iter()
                .any(|(target_kind, target_name)| target_kind == kind && target_name == name)
    })
}

fn handle_pod_detail_key(app: &mut AppState, key: KeyEvent) -> Option<AppCommand> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.view = AppView::Dashboard;
            return Some(AppCommand::Fetch(FetchCommand::StopLogStream));
        }
        KeyCode::Tab => {
            app.pod_detail_section = app.pod_detail_section.next();
            app.detail_scroll = 0;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.detail_scroll = app.detail_scroll.saturating_add(1);
            if app.pod_detail_section == PodDetailSection::Logs {
                app.log_follow = false;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.detail_scroll = app.detail_scroll.saturating_sub(1);
            if app.pod_detail_section == PodDetailSection::Logs {
                app.log_follow = false;
            }
        }
        KeyCode::Char('f') => {
            app.log_follow = !app.log_follow;
            if app.log_follow {
                app.detail_scroll = app.log_buffer.len().saturating_sub(1);
            }
        }
        _ => {}
    }
    None
}

fn handle_node_detail_key(app: &mut AppState, key: KeyEvent) -> Option<AppCommand> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.view = AppView::Dashboard;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.detail_scroll = app.detail_scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.detail_scroll = app.detail_scroll.saturating_sub(1);
        }
        _ => {}
    }
    None
}

pub fn handle_data_event(app: &mut AppState, event: DataEvent) {
    match event {
        DataEvent::Refreshed(snapshot) => {
            if let Some(msg) = snapshot.error.clone() {
                app.status_message = Some((msg, std::time::Instant::now()));
            }
            app.update_pod_history(&snapshot.pods);
            app.snapshot = Some(snapshot);
            let node_len = app.snapshot.as_ref().map(|s| s.nodes.len()).unwrap_or(0);
            if node_len == 0 {
                app.node_cursor = 0;
            } else {
                app.node_cursor = app.node_cursor.min(node_len - 1);
            }

            let workload_len = app
                .snapshot
                .as_ref()
                .map(|s| s.workloads.len())
                .unwrap_or(0);
            if workload_len == 0 {
                app.workload_cursor = 0;
            } else {
                app.workload_cursor = app.workload_cursor.min(workload_len - 1);
            }

            let event_len = app
                .snapshot
                .as_ref()
                .map(|s| s.incident_buckets.len())
                .unwrap_or(0);
            if event_len == 0 {
                app.event_cursor = 0;
            } else {
                app.event_cursor = app.event_cursor.min(event_len - 1);
            }

            let pod_count = app.filtered_pod_count();
            if pod_count == 0 {
                app.pod_cursor = 0;
            } else {
                app.pod_cursor = app.pod_cursor.min(pod_count - 1);
            }
            app.is_loading = false;
        }
        DataEvent::ConnectionState(issue) => {
            app.connection_issue = issue;
            if app.connection_issue.is_some() {
                app.is_loading = false;
            }
        }
        DataEvent::LogLine(line) => {
            app.log_buffer.push_back(line);
            while app.log_buffer.len() > 1000 {
                app.log_buffer.pop_front();
            }
            if app.log_follow {
                app.detail_scroll = app.log_buffer.len().saturating_sub(1);
            }
        }
        DataEvent::Error(msg) => {
            app.status_message = Some((msg, std::time::Instant::now()));
            app.is_loading = false;
        }
        DataEvent::Namespaces(namespaces) => {
            app.ns_list = namespaces;
            if app.overlay == Overlay::NamespaceList {
                let current_idx = app
                    .ns_list
                    .iter()
                    .position(|ns| ns == &app.config.namespace);
                if let Some(idx) = current_idx {
                    app.ns_list_cursor = idx;
                }
            }
        }
        DataEvent::ExportResult { message } => {
            app.status_message = Some((message, std::time::Instant::now()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{handle_data_event, handle_key, AppCommand};
    use crate::app::{AppState, AppView, Overlay, Panel, PodSortMode};
    use crate::config::Config;
    use crate::data::models::{
        ClusterEvent, ClusterSnapshot, ConditionStatus, ContainerInfo, EventType, HealthScore,
        HealthStatus, IncidentBucket, IncidentSeverity, IncidentTarget, NodeConditions, NodeMetric,
        PodInfo, WorkloadKind, WorkloadSummary,
    };
    use crate::events::{DataEvent, FetchCommand};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::time::Instant;

    #[test]
    fn namespace_switch_keeps_existing_snapshot_visible_until_refresh() {
        let mut app = AppState::new(Config::default());
        app.snapshot = Some(snapshot_with_events());
        app.overlay = Overlay::NamespaceList;
        app.ns_list = vec!["default".to_string(), "payments".to_string()];
        app.ns_list_cursor = 1;

        let command = handle_key(&mut app, enter_key());

        assert!(matches!(
            command,
            Some(AppCommand::Fetch(FetchCommand::RefreshAll { namespace }))
            if namespace == "payments"
        ));
        assert!(app.snapshot.is_some());
        assert!(app.is_loading);
        assert_eq!(app.config.namespace, "payments");
    }

    #[test]
    fn refreshed_snapshot_error_is_promoted_to_status_message() {
        let mut app = AppState::new(Config::default());
        let mut snapshot = snapshot_with_events();
        snapshot.error = Some("Events: forbidden".to_string());

        handle_data_event(&mut app, DataEvent::Refreshed(snapshot));

        assert_eq!(
            app.status_message.as_ref().map(|(msg, _)| msg.as_str()),
            Some("Events: forbidden")
        );
    }

    #[test]
    fn w_toggles_workload_popup_on_dashboard() {
        let mut app = AppState::new(Config::default());

        let _ = handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
        );
        assert!(matches!(app.overlay, Overlay::WorkloadPopup));

        let _ = handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
        );
        assert!(matches!(app.overlay, Overlay::None));
    }

    #[test]
    fn s_cycles_pod_sort_mode_and_resets_cursor() {
        let mut app = AppState::new(Config::default());
        app.pod_cursor = 3;

        let _ = handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );
        assert_eq!(app.pod_sort_mode, PodSortMode::Restarts);
        assert_eq!(app.pod_cursor, 0);

        let _ = handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );
        assert_eq!(app.pod_sort_mode, PodSortMode::Cpu);
    }

    #[test]
    fn e_opens_export_prompt_with_cluster_and_namespace_in_default_name() {
        let mut app = AppState::new(Config {
            namespace: "payments".to_string(),
            cluster_name: None,
            resource_group: None,
            refresh_interval_secs: 60,
            node_pool_filter: None,
        });
        let mut snapshot = snapshot_with_events();
        snapshot.context_name = Some("prod-cluster".to_string());
        app.snapshot = Some(snapshot);

        let _ = handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('E'), KeyModifiers::NONE),
        );

        assert!(matches!(app.overlay, Overlay::ExportInput));
        assert!(app.export_input.ends_with(".csv"));
        assert!(app.export_input.contains("prod-cluster"));
        assert!(app.export_input.contains("payments"));
    }

    #[test]
    fn export_prompt_enter_emits_export_command_with_custom_path() {
        let mut app = AppState::new(Config {
            namespace: "payments".to_string(),
            cluster_name: None,
            resource_group: None,
            refresh_interval_secs: 60,
            node_pool_filter: None,
        });
        let mut snapshot = snapshot_with_events();
        snapshot.context_name = None;
        app.overlay = Overlay::ExportInput;
        app.export_input = "custom-export.csv".to_string();
        app.snapshot = Some(snapshot);

        let command = handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(matches!(app.overlay, Overlay::None));
        assert!(app.export_input.is_empty());
        assert!(matches!(
            command,
            Some(AppCommand::Fetch(FetchCommand::ExportPods {
                cluster_name: None,
                namespace,
                path,
            }))
            if namespace == "payments" && path == "custom-export.csv"
        ));
    }

    #[test]
    fn enter_on_selected_pod_incident_opens_pod_detail_and_logs() {
        let mut app = AppState::new(Config::default());
        app.focused_panel = Panel::Events;
        app.snapshot = Some(snapshot_with_incident_targets(
            vec![IncidentTarget::Pod {
                pod_name: "api-0".to_string(),
            }],
            vec![pod("api-0", "default")],
            vec![],
            vec![],
            vec![],
        ));

        let command = handle_key(&mut app, enter_key());

        assert!(matches!(app.view, AppView::PodDetail { ref pod_name } if pod_name == "api-0"));
        assert!(matches!(
            command,
            Some(AppCommand::Fetch(FetchCommand::StartLogStream { pod, namespace }))
            if pod == "api-0" && namespace == "default"
        ));
    }

    #[test]
    fn enter_on_selected_node_incident_opens_node_detail() {
        let mut app = AppState::new(Config::default());
        app.focused_panel = Panel::Events;
        app.snapshot = Some(snapshot_with_incident_targets(
            vec![IncidentTarget::Node {
                node_name: "node-a".to_string(),
            }],
            vec![],
            vec![node("node-a", false)],
            vec![],
            vec![],
        ));

        let command = handle_key(&mut app, enter_key());

        assert!(matches!(app.view, AppView::NodeDetail { ref node_name } if node_name == "node-a"));
        assert!(command.is_none());
    }

    #[test]
    fn enter_on_selected_workload_incident_opens_matching_workload_popup() {
        let mut app = AppState::new(Config::default());
        app.focused_panel = Panel::Events;
        app.snapshot = Some(snapshot_with_incident_targets(
            vec![IncidentTarget::Workload {
                kind: "ReplicaSet".to_string(),
                name: "api-7f9c".to_string(),
            }],
            vec![],
            vec![],
            vec![workload(
                WorkloadKind::Deployment,
                "api",
                vec![
                    ("Deployment".to_string(), "api".to_string()),
                    ("ReplicaSet".to_string(), "api-7f9c".to_string()),
                ],
            )],
            vec![],
        ));

        let command = handle_key(&mut app, enter_key());

        assert!(matches!(app.overlay, Overlay::WorkloadPopup));
        assert_eq!(app.workload_cursor, 0);
        assert!(command.is_none());
    }

    #[test]
    fn enter_on_multi_target_incident_applies_incident_focus_to_pods() {
        let mut app = AppState::new(Config::default());
        app.focused_panel = Panel::Events;
        app.snapshot = Some(snapshot_with_incident_targets(
            vec![
                IncidentTarget::Pod {
                    pod_name: "api-0".to_string(),
                },
                IncidentTarget::Pod {
                    pod_name: "worker-0".to_string(),
                },
            ],
            vec![
                pod("api-0", "default"),
                pod("worker-0", "default"),
                pod("other", "default"),
            ],
            vec![],
            vec![],
            vec![],
        ));

        let command = handle_key(&mut app, enter_key());

        assert!(command.is_none());
        assert_eq!(app.focused_panel, Panel::Pods);
        let focus = app
            .incident_focus
            .as_ref()
            .expect("incident focus should be set");
        assert_eq!(focus.reason, "CrashLoopBackOff");
        assert!(focus.pod_names.contains("api-0"));
        assert!(focus.pod_names.contains("worker-0"));
        assert_eq!(app.filtered_pod_count(), 2);
    }

    fn enter_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
    }

    fn snapshot_with_events() -> ClusterSnapshot {
        snapshot_with_incident_targets(
            vec![IncidentTarget::Pod {
                pod_name: "api-0".to_string(),
            }],
            vec![],
            vec![],
            vec![],
            vec![ClusterEvent {
                kind: "Pod".to_string(),
                name: "api-0".to_string(),
                reason: "Started".to_string(),
                message: "Container started".to_string(),
                event_type: EventType::Normal,
                count: 1,
                timestamp: "2026-03-09T10:00:00Z".to_string(),
            }],
        )
    }

    fn snapshot_with_incident_targets(
        incident_targets: Vec<IncidentTarget>,
        pods: Vec<PodInfo>,
        nodes: Vec<NodeMetric>,
        workloads: Vec<WorkloadSummary>,
        events: Vec<ClusterEvent>,
    ) -> ClusterSnapshot {
        ClusterSnapshot {
            nodes,
            workloads,
            pods,
            events,
            incident_buckets: vec![IncidentBucket {
                reason: "CrashLoopBackOff".to_string(),
                severity: IncidentSeverity::Critical,
                occurrences: 1,
                targets: incident_targets,
                affected_resources: vec!["Pod/api-0".to_string()],
                latest_timestamp: "2026-03-09T10:00:00Z".to_string(),
                sample_message: Some("Container started".to_string()),
            }],
            health: HealthScore {
                score: 100,
                grade: 'A',
                critical_nodes: 0,
                critical_pods: 0,
                total_restarts: 0,
            },
            fetched_at: Instant::now(),
            error: None,
            context_name: Some("test".to_string()),
        }
    }

    fn pod(name: &str, namespace: &str) -> PodInfo {
        PodInfo {
            uid: format!("{namespace}-{name}-uid"),
            name: name.to_string(),
            namespace: namespace.to_string(),
            phase: "Running".to_string(),
            restarts: 0,
            age: "1m".to_string(),
            cpu_millicores: 0,
            cpu_request_millicores: 0,
            cpu_limit_millicores: 0,
            memory_mb: 0,
            memory_request_mb: 0,
            memory_limit_mb: 0,
            memory_request_pct: 0,
            memory_pct: 0,
            cpu_request_pct: 0,
            cpu_pct: 0,
            status: HealthStatus::Healthy,
            ready_containers: 1,
            total_containers: 1,
            is_ready: true,
            crash_looping: false,
            oom_killed: false,
            node_name: None,
            containers: vec![ContainerInfo {
                name: "app".to_string(),
                ready: true,
                restart_count: 0,
                state: "Running".to_string(),
                last_termination_reason: None,
                last_exit_code: None,
            }],
        }
    }

    fn node(name: &str, ready: bool) -> NodeMetric {
        NodeMetric {
            name: name.to_string(),
            cpu_millicores: 0,
            memory_mb: 0,
            memory_total_mb: 0,
            memory_pct: 0,
            cpu_pct: 0,
            status: HealthStatus::Healthy,
            cpu_capacity: 0,
            memory_capacity_mb: 0,
            unhealthy_conditions: 0,
            ready,
            conditions: NodeConditions {
                ready: if ready {
                    ConditionStatus::True
                } else {
                    ConditionStatus::False
                },
                memory_pressure: ConditionStatus::False,
                disk_pressure: ConditionStatus::False,
                pid_pressure: ConditionStatus::False,
                network_unavailable: ConditionStatus::False,
            },
            cordoned: false,
            draining: false,
            node_info: None,
        }
    }

    fn workload(
        kind: WorkloadKind,
        name: &str,
        related_event_targets: Vec<(String, String)>,
    ) -> WorkloadSummary {
        WorkloadSummary {
            kind,
            name: name.to_string(),
            namespace: "default".to_string(),
            desired_replicas: 1,
            ready_replicas: 1,
            available_replicas: 1,
            updated_replicas: Some(1),
            current_replicas: None,
            unavailable_pods: 0,
            rollout_status: "steady".to_string(),
            status: HealthStatus::Healthy,
            recent_events: vec![],
            related_event_targets,
        }
    }
}

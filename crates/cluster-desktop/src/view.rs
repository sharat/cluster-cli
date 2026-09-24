//! The desktop window: fleet overview, cluster detail, pod detail, and the
//! cluster and namespace pickers from the design canvas.

use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use cluster_core::data::models::{
    ClusterSnapshot, ConnectionIssue, EventType, IncidentBucket, IncidentSeverity,
    NamespaceSummary, NodeMetric, PodInfo, ALL_NAMESPACES, GRADE_A_THRESHOLD, GRADE_B_THRESHOLD,
    GRADE_C_THRESHOLD, GRADE_D_THRESHOLD,
};
use cluster_core::events::{DataEvent, FetchCommand};
use gpui::{
    canvas, div, point, prelude::*, px, uniform_list, AnyElement, ClickEvent, Context, Div,
    FocusHandle, FontWeight, KeyDownEvent, MouseButton, PathBuilder, Pixels, Rgba, ScrollHandle,
    SharedString, Stateful, Window,
};
use tokio::sync::mpsc;

use crate::backend::{Backend, TaggedEvent};
use crate::theme::{self, MONO, SANS};

/// Score samples kept per cluster for the trend chart (in memory, since launch).
const MAX_HISTORY: usize = 240;
const FLEET_INCIDENT_ROWS: usize = 8;
/// Log lines kept for the open pod (kubectl starts with `--tail=100`).
const MAX_LOG_LINES: usize = 1000;
/// Rows shown before the pods/nodes tables scroll (they are virtualized, so
/// only visible rows are built, however large the cluster).
const MAX_VISIBLE_ROWS: usize = 12;
const ROW_HEIGHT: f32 = 44.;
/// Minimum gap between notifications for one cluster, unless it gets worse.
const ALERT_COOLDOWN: Duration = Duration::from_secs(10 * 60);
/// Consecutive refreshes with no data before a cluster counts as unreachable.
const UNREACHABLE_AFTER: u32 = 2;

pub struct ClusterState {
    context: String,
    commands: mpsc::Sender<FetchCommand>,
    snapshot: Option<ClusterSnapshot>,
    issue: Option<ConnectionIssue>,
    error: Option<String>,
    history: VecDeque<u8>,
    /// `None` = the context's default namespace.
    namespace: Option<String>,
    namespaces: Option<Vec<NamespaceSummary>>,
    /// Refreshes in a row that produced no snapshot.
    failed_refreshes: u32,
    last_alert: Option<AlertMark>,
}

/// A notification ready to send, with what it says about severity so the
/// rate limiter can let escalations through.
struct Alert {
    summary: String,
    body: String,
    grade: char,
    critical: bool,
}

struct AlertMark {
    at: Instant,
    grade: char,
    critical: bool,
}

impl Alert {
    /// One per cluster per cooldown, unless the grade is worse than the last
    /// alert's or the first critical incident appears.
    fn should_send(&self, last: Option<&AlertMark>) -> bool {
        match last {
            None => true,
            Some(mark) => {
                mark.at.elapsed() >= ALERT_COOLDOWN
                    || (self.grade.is_ascii_alphabetic() && self.grade > mark.grade)
                    || (self.critical && !mark.critical)
            }
        }
    }
}

impl ClusterState {
    /// `?` until the first snapshot, or while the cluster is unreachable.
    fn grade(&self) -> char {
        match (&self.snapshot, &self.issue) {
            (Some(snapshot), None) => snapshot.health.grade,
            _ => '?',
        }
    }

    fn score_label(&self) -> String {
        match (&self.snapshot, &self.issue) {
            (Some(snapshot), None) => snapshot.health.score.to_string(),
            _ => "—".to_string(),
        }
    }

    fn namespace_label(&self) -> String {
        if self.namespace.as_deref() == Some(ALL_NAMESPACES) {
            return "all namespaces".to_string();
        }
        self.namespace
            .clone()
            .or_else(|| {
                self.snapshot
                    .as_ref()
                    .and_then(|s| s.pods.first().map(|p| p.namespace.clone()))
            })
            .unwrap_or_else(|| "context default".to_string())
    }

    fn status_line(&self) -> String {
        if let Some(issue) = &self.issue {
            return format!("Unreachable: {}", issue.detail);
        }
        match &self.snapshot {
            None => "Connecting…".to_string(),
            Some(snapshot) => match snapshot.incident_buckets.first() {
                Some(top) => format!("{} · {}", top.reason, target_label(top)),
                None => "No active incidents".to_string(),
            },
        }
    }

    fn send(&self, command: FetchCommand) {
        let _ = self.commands.try_send(command);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Fleet,
    Cluster(usize),
    /// The pod is `DesktopApp::pod`; the index is its cluster.
    Pod(usize),
}

/// The pod on screen, tracked by UID across refreshes, plus its log stream.
struct PodView {
    uid: String,
    name: String,
    namespace: String,
    container: Option<String>,
    previous: bool,
    stream_id: u64,
    logs: VecDeque<String>,
    log_error: Option<String>,
    log_scroll: ScrollHandle,
}

#[derive(Clone, Copy, PartialEq)]
enum Picker {
    None,
    Cluster,
    Namespace,
}

pub struct DesktopApp {
    backend: Backend,
    focus_handle: FocusHandle,
    clusters: Vec<ClusterState>,
    load_error: Option<String>,
    screen: Screen,
    picker: Picker,
    pod: Option<PodView>,
    next_stream_id: u64,
    alerts_enabled: bool,
}

impl DesktopApp {
    pub fn new(
        backend: Backend,
        mut events: mpsc::UnboundedReceiver<TaggedEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);

        let (clusters, load_error) = match backend.context_names() {
            Ok(names) if names.is_empty() => {
                (Vec::new(), Some("No contexts in your kubeconfig.".into()))
            }
            Ok(names) => (
                names
                    .into_iter()
                    .enumerate()
                    .map(|(index, context)| ClusterState {
                        commands: backend.watch(index, context.clone()),
                        context,
                        snapshot: None,
                        issue: None,
                        error: None,
                        history: VecDeque::new(),
                        namespace: None,
                        namespaces: None,
                        failed_refreshes: 0,
                        last_alert: None,
                    })
                    .collect(),
                None,
            ),
            Err(err) => (Vec::new(), Some(err)),
        };

        cx.spawn(async move |this, cx| {
            while let Some((index, event)) = events.recv().await {
                let applied = this.update(cx, |app, cx| {
                    app.apply(index, event);
                    cx.notify();
                });
                if applied.is_err() {
                    break;
                }
            }
        })
        .detach();

        // Keeps "refreshed Ns ago" labels current.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        })
        .detach();

        Self {
            backend,
            focus_handle,
            clusters,
            load_error,
            screen: Screen::Fleet,
            picker: Picker::None,
            pod: None,
            next_stream_id: 0,
            alerts_enabled: true,
        }
    }

    fn apply(&mut self, index: usize, event: DataEvent) {
        let open_pod = match (self.screen, self.pod.as_mut()) {
            (Screen::Pod(cluster), Some(pod)) if cluster == index => Some(pod),
            _ => None,
        };
        match (&event, open_pod) {
            (DataEvent::LogLine { stream_id, line }, Some(pod)) if *stream_id == pod.stream_id => {
                if pod.logs.len() == MAX_LOG_LINES {
                    pod.logs.pop_front();
                }
                pod.logs.push_back(line.clone());
                pod.log_scroll.scroll_to_bottom();
                return;
            }
            (DataEvent::LogStreamError { stream_id, message }, Some(pod))
                if *stream_id == pod.stream_id =>
            {
                pod.log_error = Some(message.clone());
                return;
            }
            _ => {}
        }

        let Some(cluster) = self.clusters.get_mut(index) else {
            return;
        };
        // A connection issue also precedes partial snapshots (e.g. nodes
        // forbidden), so only repeated refreshes without data mean unreachable.
        let alert = match &event {
            DataEvent::Refreshed(snapshot) => {
                cluster.failed_refreshes = 0;
                cluster
                    .snapshot
                    .as_ref()
                    .and_then(|previous| snapshot_alert(&cluster.context, previous, snapshot))
            }
            DataEvent::ConnectionState(Some(issue)) => {
                cluster.failed_refreshes += 1;
                (cluster.failed_refreshes == UNREACHABLE_AFTER && cluster.snapshot.is_some()).then(
                    || Alert {
                        summary: format!("{} is unreachable", cluster.context),
                        body: issue.detail.clone(),
                        grade: '?',
                        critical: true,
                    },
                )
            }
            _ => None,
        };
        if let Some(alert) = alert {
            if self.alerts_enabled && alert.should_send(cluster.last_alert.as_ref()) {
                cluster.last_alert = Some(AlertMark {
                    at: Instant::now(),
                    grade: alert.grade,
                    critical: alert.critical,
                });
                self.backend.notify(alert.summary, alert.body);
            }
        }

        match event {
            DataEvent::Refreshed(snapshot) => {
                if cluster.history.len() == MAX_HISTORY {
                    cluster.history.pop_front();
                }
                cluster.history.push_back(snapshot.health.score);
                cluster.error = snapshot.error.clone();
                cluster.issue = None;
                cluster.snapshot = Some(snapshot);
            }
            DataEvent::ConnectionState(issue) => cluster.issue = issue,
            DataEvent::Error(message) => cluster.error = Some(message),
            DataEvent::Namespaces(namespaces) => cluster.namespaces = Some(namespaces),
            DataEvent::LogLine { .. }
            | DataEvent::LogStreamError { .. }
            | DataEvent::ExportResult { .. } => {}
        }
    }

    /// Every screen change goes through here so a pod's log stream stops
    /// when its screen is left.
    fn set_screen(&mut self, screen: Screen) {
        if let Screen::Pod(index) = self.screen {
            if screen != self.screen {
                self.clusters[index].send(FetchCommand::StopLogStream);
                self.pod = None;
            }
        }
        self.screen = screen;
        self.picker = Picker::None;
    }

    fn open_cluster(&mut self, index: usize, cx: &mut Context<Self>) {
        self.set_screen(Screen::Cluster(index));
        cx.notify();
    }

    fn open_pod(&mut self, index: usize, pod: &PodInfo, cx: &mut Context<Self>) {
        self.set_screen(Screen::Pod(index));
        self.pod = Some(PodView {
            uid: pod.uid.clone(),
            name: pod.name.clone(),
            namespace: pod.namespace.clone(),
            container: pod.containers.first().map(|c| c.name.clone()),
            previous: false,
            stream_id: 0,
            logs: VecDeque::new(),
            log_error: None,
            log_scroll: ScrollHandle::new(),
        });
        self.restart_logs();
        cx.notify();
    }

    fn open_pod_named(&mut self, index: usize, name: &str, cx: &mut Context<Self>) {
        let pod = self.clusters[index]
            .snapshot
            .as_ref()
            .and_then(|s| s.pods.iter().find(|p| p.name == name))
            .cloned();
        if let Some(pod) = pod {
            self.open_pod(index, &pod, cx);
        }
    }

    /// (Re)starts `kubectl logs` for the open pod's container and mode.
    fn restart_logs(&mut self) {
        let (Screen::Pod(index), Some(pod)) = (self.screen, self.pod.as_mut()) else {
            return;
        };
        self.next_stream_id += 1;
        pod.stream_id = self.next_stream_id;
        pod.logs.clear();
        pod.log_error = None;
        self.clusters[index].send(FetchCommand::StartLogStream {
            stream_id: pod.stream_id,
            pod: pod.name.clone(),
            namespace: pod.namespace.clone(),
            container: pod.container.clone(),
            previous: pod.previous,
        });
    }

    fn select_container(&mut self, container: String, cx: &mut Context<Self>) {
        if let Some(pod) = self.pod.as_mut() {
            pod.container = Some(container);
        }
        self.restart_logs();
        cx.notify();
    }

    fn set_previous_logs(&mut self, previous: bool, cx: &mut Context<Self>) {
        if let Some(pod) = self.pod.as_mut() {
            pod.previous = previous;
        }
        self.restart_logs();
        cx.notify();
    }

    fn open_namespace_picker(&mut self, index: usize, cx: &mut Context<Self>) {
        self.clusters[index].send(FetchCommand::FetchNamespaces);
        self.picker = Picker::Namespace;
        cx.notify();
    }

    fn select_namespace(
        &mut self,
        index: usize,
        namespace: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let cluster = &mut self.clusters[index];
        cluster.namespace = namespace.clone();
        cluster.snapshot = None;
        cluster.history.clear();
        cluster.send(FetchCommand::RefreshAll {
            namespace: namespace.unwrap_or_default(),
        });
        self.picker = Picker::None;
        cx.notify();
    }

    fn refresh_all(&mut self, cx: &mut Context<Self>) {
        for cluster in &self.clusters {
            cluster.send(FetchCommand::RefreshAll {
                namespace: cluster.namespace.clone().unwrap_or_default(),
            });
        }
        cx.notify();
    }

    /// `1`–`9` open a cluster, `n` the namespace picker, Ctrl/⌘-K the cluster
    /// switcher, `r` refreshes, `m` mutes notifications, `p` toggles a pod's
    /// previous logs, and Esc
    /// closes a picker or goes back one level.
    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.secondary() && keystroke.key == "k" {
            self.picker = Picker::Cluster;
        } else if keystroke.modifiers.modified() {
            return;
        } else {
            match keystroke.key.as_str() {
                "escape" if self.picker != Picker::None => self.picker = Picker::None,
                "escape" => match self.screen {
                    Screen::Pod(index) => self.set_screen(Screen::Cluster(index)),
                    _ => self.set_screen(Screen::Fleet),
                },
                "n" => match self.screen {
                    Screen::Cluster(index) => return self.open_namespace_picker(index, cx),
                    _ => return,
                },
                "p" if matches!(self.screen, Screen::Pod(_)) => {
                    let previous = self.pod.as_ref().is_some_and(|p| p.previous);
                    return self.set_previous_logs(!previous, cx);
                }
                "r" => self.refresh_all(cx),
                "m" => self.alerts_enabled = !self.alerts_enabled,
                key => match key.parse::<usize>() {
                    Ok(digit @ 1..=9) if digit <= self.clusters.len() => {
                        return self.open_cluster(digit - 1, cx)
                    }
                    _ => return,
                },
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn last_refresh_secs(&self) -> Option<u64> {
        self.clusters
            .iter()
            .filter_map(|c| c.snapshot.as_ref())
            .map(|s| s.fetched_at.elapsed().as_secs())
            .min()
    }
}

impl Render for DesktopApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main: AnyElement = match self.screen {
            Screen::Fleet => self.render_fleet(cx).into_any_element(),
            Screen::Cluster(index) => self.render_cluster(index, cx).into_any_element(),
            Screen::Pod(index) => self.render_pod(index, cx).into_any_element(),
        };

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .relative()
            .size_full()
            .flex()
            .bg(theme::bg())
            .text_color(theme::text())
            .font_family(SANS)
            .text_size(px(13.))
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .id("main")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .px(px(36.))
                    .py(px(28.))
                    .child(main),
            )
            .when(self.picker != Picker::None, |root| {
                root.child(self.render_picker(cx))
            })
    }
}

// ---------------------------------------------------------------------------
// Sidebar

impl DesktopApp {
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fleet_selected = self.screen == Screen::Fleet;
        let incident_total: usize = self
            .clusters
            .iter()
            .filter_map(|c| c.snapshot.as_ref())
            .map(|s| s.incident_buckets.len())
            .sum();

        div()
            .w(px(232.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(24.))
            .px(px(14.))
            .py(px(20.))
            .bg(theme::sidebar())
            .border_r_1()
            .border_color(theme::border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(8.))
                    .child(logo_icon(26.))
                    .child(
                        div()
                            .ml(px(10.))
                            .font_family(MONO)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(17.))
                            .child("cluster"),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(theme::muted())
                            .border_1()
                            .border_color(rgb_border())
                            .rounded(px(4.))
                            .px(px(6.))
                            .py(px(2.))
                            .child("desktop"),
                    ),
            )
            .child(
                div().flex().flex_col().gap(px(2.)).child(
                    nav_item("nav-fleet", "Fleet", fleet_selected)
                        .child(
                            div()
                                .ml_auto()
                                .font_family(MONO)
                                .text_size(px(12.))
                                .text_color(if incident_total > 0 {
                                    theme::warning()
                                } else {
                                    theme::faint()
                                })
                                .child(incident_total.to_string()),
                        )
                        .on_click(cx.listener(|app, _: &ClickEvent, _, cx| {
                            app.set_screen(Screen::Fleet);
                            cx.notify();
                        })),
                ),
            )
            .child(section_label("Contexts").px(px(10.)).mb(px(-18.)))
            // The context list takes the free height and scrolls, keeping the
            // alerts toggle and read-only badge pinned to the bottom.
            .child(
                div()
                    .id("contexts")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .children(self.clusters.iter().enumerate().map(|(index, cluster)| {
                        let (fg, _) = theme::grade_colors(cluster.grade());
                        let selected = matches!(
                            self.screen,
                            Screen::Cluster(i) | Screen::Pod(i) if i == index
                        );
                        div()
                            .id(("ctx", index))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .h(px(34.))
                            .px(px(10.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .font_family(MONO)
                            .text_color(theme::text_secondary())
                            .when(selected, |d| d.bg(theme::selected()))
                            .hover(|s| s.bg(theme::selected()))
                            .child(div().size(px(8.)).rounded_full().bg(fg))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(cluster.context.clone()),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(fg)
                                    .child(cluster.grade().to_string()),
                            )
                            .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| {
                                app.open_cluster(index, cx)
                            }))
                    })),
            )
            .child(
                div()
                    .id("alerts-toggle")
                    .flex()
                    .items_center()
                    .h(px(36.))
                    .px(px(10.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::selected()))
                    .text_size(px(13.))
                    .child(div().text_color(theme::muted()).child("Notifications"))
                    .child(
                        div()
                            .ml_auto()
                            .px(px(8.))
                            .py(px(2.))
                            .rounded(px(10.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .when(self.alerts_enabled, |d| {
                                d.bg(theme::grade_colors('A').1)
                                    .text_color(theme::grade_colors('A').0)
                                    .child("On")
                            })
                            .when(!self.alerts_enabled, |d| {
                                d.bg(theme::selected())
                                    .text_color(theme::faint())
                                    .child("Muted")
                            }),
                    )
                    .on_click(cx.listener(|app, _: &ClickEvent, _, cx| {
                        app.alerts_enabled = !app.alerts_enabled;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .p(px(10.))
                    .border_1()
                    .border_color(theme::border())
                    .rounded(px(8.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(lock_icon(16., theme::grade_colors('A').0))
                    .child("Read-only · get / top / logs"),
            )
    }
}

// ---------------------------------------------------------------------------
// Fleet overview

impl DesktopApp {
    fn render_fleet(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let refreshed = match self.last_refresh_secs() {
            Some(secs) => format!("refreshed {secs}s ago"),
            None => "waiting for first refresh".to_string(),
        };

        let mut incidents: Vec<(&str, &IncidentBucket)> = self
            .clusters
            .iter()
            .filter(|c| c.issue.is_none())
            .filter_map(|c| c.snapshot.as_ref().map(|s| (c.context.as_str(), s)))
            .flat_map(|(ctx, s)| s.incident_buckets.iter().map(move |b| (ctx, b)))
            .collect();
        incidents.sort_by(|(_, a), (_, b)| {
            b.severity
                .rank()
                .cmp(&a.severity.rank())
                .then(b.occurrences.cmp(&a.occurrences))
        });
        incidents.truncate(FLEET_INCIDENT_ROWS);

        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(page_title(
                        "Fleet",
                        format!("{} contexts · {refreshed}", self.clusters.len()),
                    ))
                    .child(div().ml_auto().child(
                        primary_button("refresh-all", "Refresh all").on_click(
                            cx.listener(|app, _: &ClickEvent, _, cx| app.refresh_all(cx)),
                        ),
                    )),
            )
            .when_some(self.load_error.clone(), |d, err| {
                d.child(notice(format!("Could not read kubeconfig contexts: {err}")))
            })
            .child(
                div().grid().grid_cols(3).gap(px(16.)).children(
                    self.clusters
                        .iter()
                        .enumerate()
                        .map(|(index, cluster)| self.render_cluster_card(index, cluster, cx)),
                ),
            )
            .child(
                panel()
                    .child(panel_header(
                        "Active incidents",
                        "ranked by severity, across all contexts",
                    ))
                    .child(table_header(&[
                        ("Severity", Some(110.)),
                        ("Reason", Some(200.)),
                        ("Cluster", Some(170.)),
                        ("Target", None),
                        ("Count", Some(90.)),
                    ]))
                    .when(incidents.is_empty(), |d| {
                        d.child(
                            div()
                                .px(px(20.))
                                .py(px(16.))
                                .text_color(theme::faint())
                                .child("No active incidents."),
                        )
                    })
                    .children(incidents.into_iter().map(|(ctx, bucket)| {
                        table_row()
                            .child(fixed(110., severity_badge(bucket.severity)))
                            .child(fixed(200., mono(bucket.reason.clone())))
                            .child(fixed(
                                170.,
                                mono(ctx.to_string()).text_color(theme::text_secondary()),
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_family(MONO)
                                    .text_color(theme::muted())
                                    .child(target_label(bucket)),
                            )
                            .child(fixed(
                                90.,
                                div()
                                    .text_color(theme::text_secondary())
                                    .child(bucket.occurrences.to_string()),
                            ))
                    })),
            )
    }

    fn render_cluster_card(
        &self,
        index: usize,
        cluster: &ClusterState,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let grade = cluster.grade();
        let (fg, _) = theme::grade_colors(grade);
        let troubled = matches!(grade, 'C' | 'D' | 'F');
        let (nodes, pods, incidents) = match (&cluster.snapshot, &cluster.issue) {
            (Some(s), None) => (
                s.nodes.len().to_string(),
                s.pods.len().to_string(),
                s.incident_buckets.len(),
            ),
            _ => ("—".into(), "—".into(), 0),
        };

        div()
            .id(("card", index))
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(20.))
            .py(px(18.))
            .bg(theme::surface())
            .border_1()
            .border_color(if troubled {
                gpui::rgb(0x3a3325)
            } else {
                theme::border()
            })
            .rounded(px(12.))
            .cursor_pointer()
            .hover(|s| {
                s.border_color(theme::border_strong())
                    .bg(theme::surface_raised())
            })
            .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| app.open_cluster(index, cx)))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .min_w_0()
                            .flex_1()
                            .child(
                                mono(cluster.context.clone())
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate(),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::faint())
                                    .child(format!("ns: {}", cluster.namespace_label()))
                                    .when(
                                        cluster
                                            .snapshot
                                            .as_ref()
                                            .is_some_and(|s| !s.coverage.is_complete()),
                                        |d| {
                                            d.child(
                                                div()
                                                    .text_color(theme::warning())
                                                    .child("partial data"),
                                            )
                                        },
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(mono(cluster.score_label()).text_color(theme::muted()))
                            .child(grade_chip(grade, 40., 22.)),
                    ),
            )
            .child(sparkline(
                cluster.history.iter().copied().collect(),
                fg,
                36.,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(18.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(stat(nodes, "nodes", theme::text()))
                    .child(stat(pods, "pods", theme::text()))
                    .child(stat(
                        incidents.to_string(),
                        "incidents",
                        if incidents > 0 {
                            theme::warning()
                        } else {
                            theme::text()
                        },
                    )),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(theme::border())
                    .pt(px(10.))
                    .text_size(px(12.))
                    .text_color(if incidents > 0 || cluster.issue.is_some() {
                        theme::text_secondary()
                    } else {
                        theme::faint()
                    })
                    .truncate()
                    .child(cluster.status_line()),
            )
    }
}

// ---------------------------------------------------------------------------
// Cluster detail

impl DesktopApp {
    fn render_cluster(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let cluster = &self.clusters[index];
        let grade = cluster.grade();
        let (fg, _) = theme::grade_colors(grade);
        let trend = match (cluster.history.front(), cluster.history.back()) {
            (Some(first), Some(last)) if cluster.history.len() > 1 => {
                let delta = i32::from(*last) - i32::from(*first);
                match delta.cmp(&0) {
                    std::cmp::Ordering::Less => format!("down {} since launch", -delta),
                    std::cmp::Ordering::Greater => format!("up {delta} since launch"),
                    std::cmp::Ordering::Equal => "steady since launch".to_string(),
                }
            }
            _ => "collecting samples".to_string(),
        };

        // Explicit margins: GPUI 0.2.2 drops `gap` in this row.
        let header = div()
            .flex()
            .items_center()
            .child(
                icon_button("back", "‹").on_click(cx.listener(|app, _: &ClickEvent, _, cx| {
                    app.set_screen(Screen::Fleet);
                    cx.notify();
                })),
            )
            .child(grade_chip(grade, 56., 30.).ml(px(20.)))
            .child(
                div()
                    .ml(px(20.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_color(theme::faint())
                            .child(format!("Fleet / {}", cluster.context)),
                    )
                    .child(
                        div()
                            .id("switch-cluster")
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .font_family(MONO)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(24.))
                            .hover(|s| s.text_color(theme::accent()))
                            .child(cluster.context.clone())
                            .child(
                                div()
                                    .text_color(theme::faint())
                                    .text_size(px(16.))
                                    .child("▾"),
                            )
                            .on_click(cx.listener(|app, _: &ClickEvent, _, cx| {
                                app.picker = Picker::Cluster;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .ml(px(20.))
                    .pl(px(20.))
                    .border_l_1()
                    .border_color(theme::border())
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .font_family(MONO)
                            .child(div().text_size(px(22.)).child(cluster.score_label()))
                            .child(div().text_color(theme::faint()).child(" / 100")),
                    )
                    .child(div().text_size(px(12.)).text_color(fg).child(trend)),
            )
            .child(
                div().ml_auto().child(
                    div()
                        .id("namespace-picker")
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .h(px(40.))
                        .px(px(14.))
                        .bg(theme::surface())
                        .border_1()
                        .border_color(gpui::rgb(0x3a4a66))
                        .rounded(px(8.))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::surface_raised()))
                        .child(div().text_color(theme::muted()).child("Namespace"))
                        .child(mono(cluster.namespace_label()).font_weight(FontWeight::MEDIUM))
                        .child(div().text_color(theme::faint()).child("▾"))
                        .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| {
                            app.open_namespace_picker(index, cx)
                        })),
                ),
            );

        let body: AnyElement = match (&cluster.snapshot, &cluster.issue) {
            (_, Some(issue)) => notice(format!("Unreachable: {}", issue.detail)).into_any_element(),
            (None, None) => notice("Connecting…".to_string()).into_any_element(),
            (Some(snapshot), None) => self
                .render_cluster_body(index, cluster, snapshot, cx)
                .into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(header)
            .when_some(cluster.error.clone(), |d, err| d.child(notice(err)))
            .child(body)
    }

    fn render_cluster_body(
        &self,
        index: usize,
        cluster: &ClusterState,
        snapshot: &ClusterSnapshot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let health = &snapshot.health;
        let grade = health.grade;

        // Trend and the factors behind the grade share one panel.
        let health_panel = panel()
            .flex_1()
            .min_w_0()
            .child(panel_header(
                "Health score",
                &format!(
                    "why it's a{} {grade} · samples since launch",
                    article(grade)
                ),
            ))
            .child(
                div()
                    .px(px(20.))
                    .child(score_chart(cluster.history.iter().copied().collect(), 170.)),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(4)
                    .gap(px(12.))
                    .px(px(20.))
                    .pt(px(16.))
                    .child(factor_tile(
                        "Critical nodes",
                        "memory ≥85% or not ready",
                        health.critical_nodes,
                    ))
                    .child(factor_tile(
                        "Critical pods",
                        "failed, unready, crash-looping or OOM",
                        health.critical_pods,
                    ))
                    .child(factor_tile(
                        "Restarts",
                        "−2 each, capped",
                        health.total_restarts,
                    ))
                    .child(factor_tile(
                        "Incidents",
                        "listed alongside",
                        snapshot.incident_buckets.len() as u32,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .mx(px(20.))
                    .mt(px(16.))
                    .py(px(14.))
                    .border_t_1()
                    .border_color(theme::border())
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Score"))
                    .child(
                        div()
                            .ml(px(12.))
                            .text_size(px(12.))
                            .text_color(theme::faint())
                            .child(grade_band_hint(health.score)),
                    )
                    .child(
                        mono(format!("{} · {grade}", health.score))
                            .ml_auto()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::grade_colors(grade).0),
                    ),
            );

        let incidents_panel = panel()
            .w(px(460.))
            .flex_shrink_0()
            .child(panel_header(
                "Incidents",
                &snapshot.incident_buckets.len().to_string(),
            ))
            .when(snapshot.incident_buckets.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(20.))
                        .py(px(16.))
                        .text_color(theme::faint())
                        .child("No active incidents."),
                )
            })
            .children(
                snapshot
                    .incident_buckets
                    .iter()
                    .enumerate()
                    .map(|(i, bucket)| {
                        let pod_name = bucket
                            .targets
                            .iter()
                            .find_map(|t| t.pod_name().map(str::to_string));
                        let row = div()
                            .id(("incident", i))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .px(px(20.))
                            .py(px(12.))
                            .border_t_1()
                            .border_color(theme::divider())
                            .when(i == 0, |d| d.bg(theme::surface_raised()))
                            .when_some(pod_name, |d, name| {
                                d.cursor_pointer()
                                    .hover(|s| s.bg(theme::selected()))
                                    .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| {
                                        app.open_pod_named(index, &name, cx)
                                    }))
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(severity_badge(bucket.severity))
                                    .child(
                                        mono(bucket.reason.clone())
                                            .ml(px(10.))
                                            .font_weight(FontWeight::MEDIUM),
                                    )
                                    .child(
                                        div()
                                            .ml_auto()
                                            .text_size(px(12.))
                                            .text_color(theme::muted())
                                            .child(format!(
                                                "×{} · {}",
                                                bucket.occurrences, bucket.latest_timestamp
                                            )),
                                    ),
                            )
                            .child(
                                mono(target_label(bucket))
                                    .text_color(theme::text_secondary())
                                    .truncate(),
                            );
                        match (&bucket.sample_message, i) {
                            (Some(message), 0) => row.child(
                                div()
                                    .p(px(10.))
                                    .bg(theme::bg())
                                    .border_1()
                                    .border_color(theme::border())
                                    .rounded(px(6.))
                                    .font_family(MONO)
                                    .text_size(px(12.))
                                    .text_color(theme::muted())
                                    .line_clamp(4)
                                    .child(message.clone()),
                            ),
                            _ => row,
                        }
                    }),
            );

        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .when_some(coverage_notice(snapshot), |d, text| d.child(notice(text)))
            .child(
                div()
                    .flex()
                    .gap(px(20.))
                    .child(health_panel)
                    .child(incidents_panel),
            )
            .child(self.render_nodes_panel(index, snapshot, cx))
            .child(self.render_pods_panel(index, snapshot, cx))
    }

    fn render_nodes_panel(
        &self,
        index: usize,
        snapshot: &ClusterSnapshot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let metrics = snapshot.coverage.metrics_available;
        let mut pods_per_node: HashMap<String, usize> = HashMap::new();
        for pod in &snapshot.pods {
            if let Some(node) = &pod.node_name {
                *pods_per_node.entry(node.clone()).or_default() += 1;
            }
        }
        let pods_per_node = Rc::new(pods_per_node);
        let count = snapshot.nodes.len();

        panel()
            .child(panel_header(
                "Nodes",
                &format!("{count} · pressure line at 85%"),
            ))
            .child(table_header(&[
                ("Node", None),
                ("Status", Some(130.)),
                ("CPU", Some(240.)),
                ("Memory", Some(240.)),
                ("Pods", Some(56.)),
            ]))
            .when(!snapshot.coverage.nodes_visible, |d| {
                d.child(empty_row(
                    "Listing nodes isn't permitted for this context (RBAC), so node health isn't part of the score.",
                ))
            })
            .when(snapshot.coverage.nodes_visible && count == 0, |d| {
                d.child(empty_row("No nodes match the current node pool filter."))
            })
            .when(count > 0, |d| {
                d.child(
                    uniform_list(
                        ("nodes", index),
                        count,
                        cx.processor(move |app, range: Range<usize>, _, _| {
                            let Some(snapshot) = app.clusters[index].snapshot.as_ref() else {
                                return Vec::new();
                            };
                            snapshot.nodes[range.start.min(snapshot.nodes.len())
                                ..range.end.min(snapshot.nodes.len())]
                                .iter()
                                .map(|node| {
                                    let pods = pods_per_node.get(&node.name).copied().unwrap_or(0);
                                    node_row(node, pods, metrics)
                                })
                                .collect()
                        }),
                    )
                    .h(px(visible_rows_height(count))),
                )
            })
    }

    fn render_pods_panel(
        &self,
        index: usize,
        snapshot: &ClusterSnapshot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pods = &snapshot.pods;
        let mut order: Vec<usize> = (0..pods.len()).collect();
        order.sort_by(|&a, &b| {
            let (a, b) = (&pods[a], &pods[b]);
            pod_status(b)
                .2
                .cmp(&pod_status(a).2)
                .then(b.restarts.cmp(&a.restarts))
                .then(a.namespace.cmp(&b.namespace))
                .then(a.name.cmp(&b.name))
        });
        let order = Rc::new(order);
        let count = order.len();
        let all_namespaces = self.clusters[index].namespace.as_deref() == Some(ALL_NAMESPACES);
        let metrics = snapshot.coverage.metrics_available;

        let mut columns: Vec<(&'static str, Option<f32>)> = vec![("Pod", None)];
        if all_namespaces {
            columns.push(("Namespace", Some(150.)));
        }
        columns.extend([
            ("Status", Some(150.)),
            ("Ready", Some(60.)),
            ("Restarts", Some(70.)),
            ("CPU", Some(150.)),
            ("Memory", Some(150.)),
            ("Node", Some(160.)),
            ("Age", Some(56.)),
        ]);

        panel()
            .child(panel_header(
                "Pods",
                &format!("{count} · worst first · click one for details"),
            ))
            .child(table_header(&columns))
            .when(count == 0, |d| {
                d.child(empty_row("No pods in this namespace."))
            })
            .when(count > 0, |d| {
                d.child(
                    uniform_list(
                        ("pods", index),
                        count,
                        cx.processor(move |app, range: Range<usize>, _, cx| {
                            let Some(snapshot) = app.clusters[index].snapshot.as_ref() else {
                                return Vec::new();
                            };
                            let rows: Vec<(usize, PodInfo)> = range
                                .filter_map(|row| {
                                    let pod = order.get(row).and_then(|&i| snapshot.pods.get(i))?;
                                    Some((row, pod.clone()))
                                })
                                .collect();
                            rows.into_iter()
                                .map(|(row, pod)| {
                                    pod_row(index, row, pod, all_namespaces, metrics, cx)
                                })
                                .collect()
                        }),
                    )
                    .h(px(visible_rows_height(count))),
                )
            })
    }
}

// ---------------------------------------------------------------------------
// Pod detail

impl DesktopApp {
    fn render_pod(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let cluster = &self.clusters[index];
        let Some(view) = self.pod.as_ref() else {
            return div().child(notice("No pod selected.".to_string()));
        };
        let snapshot = cluster.snapshot.as_ref();
        let pod = snapshot.and_then(|s| s.pods.iter().find(|p| p.uid == view.uid));

        let header = div()
            .flex()
            .items_center()
            .child(icon_button("pod-back", "‹").on_click(cx.listener(
                move |app, _: &ClickEvent, _, cx| {
                    app.set_screen(Screen::Cluster(index));
                    cx.notify();
                },
            )))
            .child(
                div()
                    .ml(px(20.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_color(theme::faint())
                            .child(format!("Fleet / {} / {}", cluster.context, view.namespace)),
                    )
                    .child(
                        mono(view.name.clone())
                            .text_size(px(22.))
                            .font_weight(FontWeight::SEMIBOLD),
                    ),
            )
            .when_some(pod, |d, pod| {
                let (status, color, _) = pod_status(pod);
                d.child(
                    div()
                        .ml_auto()
                        .px(px(14.))
                        .py(px(8.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(color)
                        .text_color(color)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(status),
                )
            });

        let Some(pod) = pod else {
            return div()
                .flex()
                .flex_col()
                .gap(px(20.))
                .child(header)
                .child(notice(
                    "This pod is no longer in the latest snapshot (deleted or rescheduled)."
                        .to_string(),
                ));
        };
        let snapshot = snapshot.expect("pod came from the snapshot");
        let metrics = snapshot.coverage.metrics_available;
        let ready = pod.containers.iter().filter(|c| c.ready).count();

        let overview = panel()
            .flex_1()
            .min_w_0()
            .child(panel_header("Overview", ""))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .px(px(20.))
                    .pb(px(18.))
                    .child(kv("Phase", pod.phase.clone()))
                    .child(kv(
                        "Ready",
                        format!("{ready}/{} containers", pod.containers.len()),
                    ))
                    .child(kv("Restarts", pod.restarts.to_string()))
                    .child(kv(
                        "Node",
                        pod.node_name
                            .clone()
                            .unwrap_or_else(|| "unscheduled".into()),
                    ))
                    .child(kv("Namespace", pod.namespace.clone()))
                    .child(kv("Age", pod.age.clone()))
                    .child(kv("UID", pod.uid.clone()))
                    .child(kv(
                        "Flags",
                        pod_flags(pod).unwrap_or_else(|| "none".to_string()),
                    )),
            );

        let resources = panel()
            .flex_1()
            .min_w_0()
            .child(panel_header("Resources", "usage vs limit"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .px(px(20.))
                    .pb(px(18.))
                    .child(resource_block(
                        metrics,
                        "CPU",
                        pod.cpu_pct,
                        format!("{}m used", pod.cpu_millicores),
                        format!(
                            "request {} · limit {}",
                            millicores(pod.cpu_request_millicores),
                            millicores(pod.cpu_limit_millicores)
                        ),
                    ))
                    .child(resource_block(
                        metrics,
                        "Memory",
                        pod.memory_pct,
                        format!("{} MiB used", pod.memory_mb),
                        format!(
                            "request {} · limit {}",
                            mebibytes(pod.memory_request_mb),
                            mebibytes(pod.memory_limit_mb)
                        ),
                    )),
            );

        let selected_container = view.container.clone();
        let containers = panel()
            .child(panel_header("Containers", "click one to show its logs"))
            .child(table_header(&[
                ("Container", None),
                ("Ready", Some(70.)),
                ("Restarts", Some(80.)),
                ("State", Some(200.)),
                ("Last termination", Some(200.)),
                ("Exit", Some(60.)),
            ]))
            .children(pod.containers.iter().enumerate().map(|(i, c)| {
                let name = c.name.clone();
                let selected = selected_container.as_deref() == Some(c.name.as_str());
                table_row()
                    .id(("container", i))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(theme::selected()))
                    .hover(|s| s.bg(theme::selected()))
                    .child(
                        mono(c.name.clone())
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .when(selected, |d| d.text_color(theme::accent())),
                    )
                    .child(fixed(
                        70.,
                        div()
                            .text_color(if c.ready {
                                theme::muted()
                            } else {
                                theme::warning()
                            })
                            .child(if c.ready { "yes" } else { "no" }),
                    ))
                    .child(fixed(
                        80.,
                        mono(c.restart_count.to_string()).text_color(if c.restart_count > 0 {
                            theme::warning()
                        } else {
                            theme::muted()
                        }),
                    ))
                    .child(fixed(200., div().truncate().child(c.state.clone())))
                    .child(fixed(
                        200.,
                        div().truncate().text_color(theme::muted()).child(
                            c.last_termination_reason
                                .clone()
                                .unwrap_or_else(|| "—".into()),
                        ),
                    ))
                    .child(fixed(
                        60.,
                        mono(
                            c.last_exit_code
                                .map(|code| code.to_string())
                                .unwrap_or_else(|| "—".into()),
                        )
                        .text_color(theme::muted()),
                    ))
                    .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| {
                        app.select_container(name.clone(), cx)
                    }))
            }));

        let events: Vec<_> = snapshot
            .events
            .iter()
            .filter(|e| e.name == pod.name)
            .collect();
        let events_panel = panel()
            .flex_1()
            .min_w_0()
            .child(panel_header("Events", &events.len().to_string()))
            .when(events.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(20.))
                        .pb(px(16.))
                        .text_color(theme::faint())
                        .child("No recent events for this pod."),
                )
            })
            .children(events.into_iter().map(|e| {
                let color = if e.event_type == EventType::Warning {
                    theme::warning()
                } else {
                    theme::muted()
                };
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .px(px(20.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(theme::divider())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(mono(e.reason.clone()).text_color(color))
                            .child(
                                div()
                                    .ml_auto()
                                    .text_size(px(12.))
                                    .text_color(theme::faint())
                                    .child(format!("×{} · {}", e.count, e.timestamp)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::text_secondary())
                            .child(e.message.clone()),
                    )
            }));

        let incidents: Vec<_> = snapshot
            .incident_buckets
            .iter()
            .filter(|b| {
                b.targets
                    .iter()
                    .any(|t| t.pod_name() == Some(pod.name.as_str()))
            })
            .collect();
        let incidents_panel = panel()
            .w(px(420.))
            .flex_shrink_0()
            .child(panel_header("Incidents", &incidents.len().to_string()))
            .when(incidents.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(20.))
                        .pb(px(16.))
                        .text_color(theme::faint())
                        .child("Not part of any incident."),
                )
            })
            .children(incidents.into_iter().map(|b| {
                div()
                    .flex()
                    .items_center()
                    .px(px(20.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(theme::divider())
                    .child(severity_badge(b.severity))
                    .child(mono(b.reason.clone()).ml(px(10.)))
                    .child(
                        div()
                            .ml_auto()
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(format!("×{}", b.occurrences)),
                    )
            }));

        let previous = view.previous;
        let logs_panel = panel()
            .child(
                div()
                    .flex()
                    .items_center()
                    .px(px(20.))
                    .py(px(12.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Logs"),
                    )
                    .child(
                        mono(view.container.clone().unwrap_or_else(|| "default".into()))
                            .ml(px(12.))
                            .text_size(px(12.))
                            .text_color(theme::muted()),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .p(px(3.))
                            .bg(theme::bg())
                            .border_1()
                            .border_color(theme::border())
                            .rounded(px(8.))
                            .child(
                                segment("logs-live", "Live", !previous).on_click(cx.listener(
                                    |app, _: &ClickEvent, _, cx| app.set_previous_logs(false, cx),
                                )),
                            )
                            .child(segment("logs-previous", "Previous", previous).on_click(
                                cx.listener(|app, _: &ClickEvent, _, cx| {
                                    app.set_previous_logs(true, cx)
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .id("log-lines")
                    .track_scroll(&view.log_scroll)
                    .overflow_y_scroll()
                    .h(px(360.))
                    .mx(px(12.))
                    .mb(px(12.))
                    .p(px(12.))
                    .bg(theme::bg())
                    .border_1()
                    .border_color(theme::border())
                    .rounded(px(8.))
                    .font_family(MONO)
                    .text_size(px(12.))
                    .text_color(theme::text_secondary())
                    .when_some(view.log_error.clone(), |d, err| {
                        d.child(div().text_color(theme::critical()).child(err))
                    })
                    .when(view.logs.is_empty() && view.log_error.is_none(), |d| {
                        d.child(div().text_color(theme::faint()).child(if previous {
                            "Waiting for previous container logs…"
                        } else {
                            "Waiting for log lines…"
                        }))
                    })
                    .children(view.logs.iter().map(|line| div().child(line.clone()))),
            );

        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(header)
            .child(div().flex().gap(px(20.)).child(overview).child(resources))
            .child(containers)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(20.))
                    .child(events_panel)
                    .child(incidents_panel),
            )
            .child(logs_panel)
    }
}

// ---------------------------------------------------------------------------
// Pickers

impl DesktopApp {
    fn render_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let content: AnyElement = match (self.picker, self.screen) {
            (Picker::Namespace, Screen::Cluster(index)) => {
                self.render_namespace_picker(index, cx).into_any_element()
            }
            _ => self.render_cluster_picker(cx).into_any_element(),
        };

        div()
            .id("picker-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_start()
            .justify_center()
            .pt(px(96.))
            .bg(theme::backdrop())
            .on_click(cx.listener(|app, _: &ClickEvent, _, cx| {
                app.picker = Picker::None;
                cx.notify();
            }))
            .child(
                div()
                    .id("picker")
                    .w(px(480.))
                    .max_h(px(620.))
                    .flex()
                    .flex_col()
                    .bg(theme::surface())
                    .border_1()
                    .border_color(theme::border_strong())
                    .rounded(px(14.))
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(content),
            )
    }

    fn render_cluster_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = match self.screen {
            Screen::Cluster(index) | Screen::Pod(index) => Some(index),
            Screen::Fleet => None,
        };
        div()
            .flex()
            .flex_col()
            .child(picker_header(
                "Switch cluster",
                "Contexts from your kubeconfig",
            ))
            .child(
                div()
                    .id("cluster-list")
                    .flex()
                    .flex_col()
                    .p(px(6.))
                    .overflow_y_scroll()
                    .children(self.clusters.iter().enumerate().map(|(index, cluster)| {
                        let is_current = current == Some(index);
                        div()
                            .id(("pick-cluster", index))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .h(px(48.))
                            .px(px(10.))
                            .rounded(px(8.))
                            .cursor_pointer()
                            .when(is_current, |d| d.bg(theme::selected()))
                            .hover(|s| s.bg(theme::selected()))
                            .child(grade_chip(cluster.grade(), 30., 14.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .min_w_0()
                                    .flex_1()
                                    .child(
                                        mono(cluster.context.clone())
                                            .font_weight(FontWeight::MEDIUM),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(theme::faint())
                                            .truncate()
                                            .child(cluster.status_line()),
                                    ),
                            )
                            .when(is_current, |d| {
                                d.child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(theme::accent())
                                        .child("current"),
                                )
                            })
                            .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| {
                                app.open_cluster(index, cx)
                            }))
                    })),
            )
            .child(picker_footer(&[
                ("1–9", "jump"),
                ("Ctrl/⌘ K", "switch"),
                ("Esc", "close"),
            ]))
    }

    fn render_namespace_picker(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let cluster = &self.clusters[index];
        let selected = cluster.namespace.clone();

        let mut list = div()
            .id("namespace-list")
            .flex()
            .flex_col()
            .p(px(6.))
            .overflow_y_scroll()
            .child(
                namespace_row(
                    ("ns-default", 0),
                    "Context default".to_string(),
                    None,
                    selected.is_none(),
                    false,
                )
                .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| {
                    app.select_namespace(index, None, cx)
                })),
            )
            .child(
                namespace_row(
                    ("ns-all", 0),
                    "All namespaces".to_string(),
                    None,
                    selected.as_deref() == Some(ALL_NAMESPACES),
                    false,
                )
                .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| {
                    app.select_namespace(index, Some(ALL_NAMESPACES.to_string()), cx)
                })),
            );

        list = match &cluster.namespaces {
            None => list.child(
                div()
                    .px(px(10.))
                    .py(px(12.))
                    .text_color(theme::faint())
                    .child("Loading namespaces…"),
            ),
            Some(namespaces) => list.children(namespaces.iter().enumerate().map(|(i, ns)| {
                let name = ns.name.clone();
                let is_selected = selected.as_deref() == Some(ns.name.as_str());
                namespace_row(
                    ("ns", i),
                    ns.name.clone(),
                    Some(ns.pod_count),
                    is_selected,
                    true,
                )
                .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| {
                    app.select_namespace(index, Some(name.clone()), cx)
                }))
            })),
        };

        div()
            .flex()
            .flex_col()
            .child(picker_header(
                "Namespace",
                &format!("Namespaces in {}", cluster.context),
            ))
            .child(list)
            .child(picker_footer(&[("n", "opens this"), ("Esc", "close")]))
    }
}

// ---------------------------------------------------------------------------
// Building blocks

fn rgb_border() -> Rgba {
    gpui::rgb(0x323846)
}

fn mono(text: impl Into<SharedString>) -> Div {
    div().font_family(MONO).child(text.into())
}

fn article(grade: char) -> &'static str {
    if matches!(grade, 'A' | 'F') {
        "n"
    } else {
        ""
    }
}

fn target_label(bucket: &IncidentBucket) -> String {
    match bucket.targets.as_slice() {
        [] => bucket
            .affected_resources
            .first()
            .cloned()
            .unwrap_or_default(),
        [only] => only.display_label(),
        [first, rest @ ..] => format!("{} +{} more", first.display_label(), rest.len()),
    }
}

fn grade_band_hint(score: u8) -> String {
    let next = [
        (GRADE_A_THRESHOLD, 'A'),
        (GRADE_B_THRESHOLD, 'B'),
        (GRADE_C_THRESHOLD, 'C'),
        (GRADE_D_THRESHOLD, 'D'),
    ]
    .into_iter()
    .rev()
    .find(|(threshold, _)| score < *threshold);
    match next {
        Some((threshold, grade)) => format!("{} more points to reach {grade}.", threshold - score),
        None => "Top grade.".to_string(),
    }
}

fn nav_item(id: &'static str, label: &'static str, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(10.))
        .h(px(40.))
        .px(px(10.))
        .rounded(px(8.))
        .cursor_pointer()
        .text_size(px(14.))
        .when(selected, |d| {
            d.bg(theme::selected()).font_weight(FontWeight::MEDIUM)
        })
        .when(!selected, |d| d.text_color(theme::muted()))
        .hover(|s| s.bg(theme::selected()))
        .child(label)
}

fn section_label(text: &'static str) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::faint())
        .child(text.to_uppercase())
}

fn page_title(title: &'static str, subtitle: String) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(26.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(div().text_color(theme::muted()).child(subtitle))
}

fn primary_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(40.))
        .px(px(14.))
        .bg(theme::accent_fill())
        .rounded(px(8.))
        .text_color(gpui::white())
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|s| s.opacity(0.9))
        .child(label)
}

fn icon_button(id: &'static str, glyph: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(40.))
        .border_1()
        .border_color(theme::border_strong())
        .rounded(px(8.))
        .text_size(px(20.))
        .text_color(theme::text_secondary())
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface()))
        .child(glyph)
}

fn notice(message: String) -> Div {
    div()
        .px(px(16.))
        .py(px(12.))
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border())
        .rounded(px(10.))
        .text_color(theme::text_secondary())
        .child(message)
}

fn panel() -> Div {
    div()
        .flex()
        .flex_col()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border())
        .rounded(px(12.))
        .overflow_hidden()
}

fn panel_header(title: &str, detail: &str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(20.))
        .py(px(14.))
        .child(
            div()
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme::faint())
                .child(detail.to_string()),
        )
}

fn picker_header(title: &str, detail: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(px(16.))
        .py(px(14.))
        .border_b_1()
        .border_color(theme::border())
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(detail.to_string()),
        )
}

fn picker_footer(hints: &[(&'static str, &'static str)]) -> Div {
    div()
        .flex()
        .gap(px(16.))
        .px(px(16.))
        .py(px(10.))
        .border_t_1()
        .border_color(theme::border())
        .text_size(px(12.))
        .text_color(theme::faint())
        .children(hints.iter().map(|(key, action)| {
            div()
                .flex()
                .gap(px(6.))
                .child(
                    mono(*key)
                        .text_color(theme::muted())
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(*action)
        }))
}

fn table_header(columns: &[(&'static str, Option<f32>)]) -> Div {
    let mut row = div()
        .flex()
        .gap(px(16.))
        .px(px(20.))
        .py(px(10.))
        .border_t_1()
        .border_color(theme::border())
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::faint());
    for (label, width) in columns {
        let cell = div().truncate().child(label.to_uppercase());
        row = row.child(match width {
            Some(w) => cell.w(px(*w)).flex_shrink_0(),
            None => cell.flex_1().min_w_0(),
        });
    }
    row
}

fn table_row() -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(16.))
        .h(px(44.))
        .px(px(20.))
        .border_t_1()
        .border_color(theme::divider())
}

fn fixed(width: f32, child: impl IntoElement) -> Div {
    div().w(px(width)).flex_shrink_0().min_w_0().child(child)
}

fn stat(value: String, label: &'static str, color: Rgba) -> Div {
    div()
        .flex()
        .gap(px(4.))
        .child(
            mono(value)
                .text_color(color)
                .font_weight(FontWeight::MEDIUM),
        )
        .child(label)
}

fn grade_chip(grade: char, size: f32, text_size: f32) -> Div {
    let (fg, bg) = theme::grade_colors(grade);
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded(px(size / 4.))
        .bg(bg)
        .text_color(fg)
        .font_family(MONO)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(text_size))
        .child(grade.to_string())
}

fn severity_badge(severity: IncidentSeverity) -> Div {
    let (label, color) = match severity {
        IncidentSeverity::Critical => ("Critical", theme::critical()),
        IncidentSeverity::Warning => ("Warning", theme::warning()),
        IncidentSeverity::Elevated => ("Elevated", theme::muted()),
    };
    div()
        .flex()
        .items_center()
        .gap(px(7.))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(div().size(px(8.)).rounded(px(2.)).bg(color))
        .child(label)
}

/// What changed between two snapshots that is worth a notification: new
/// critical/warning incidents and a worse grade. `None` if nothing did.
fn snapshot_alert(
    context: &str,
    previous: &ClusterSnapshot,
    current: &ClusterSnapshot,
) -> Option<Alert> {
    let key = |b: &IncidentBucket| (b.reason.clone(), target_label(b));
    let known: std::collections::HashSet<_> = previous.incident_buckets.iter().map(key).collect();
    let fresh: Vec<&IncidentBucket> = current
        .incident_buckets
        .iter()
        .filter(|b| b.severity != IncidentSeverity::Elevated && !known.contains(&key(b)))
        .collect();
    // Grades are letters, so a later letter is a worse grade.
    let dropped = current.health.grade > previous.health.grade;
    if fresh.is_empty() && !dropped {
        return None;
    }

    let summary = if dropped {
        format!(
            "{context} dropped from {} to {}",
            previous.health.grade, current.health.grade
        )
    } else if fresh.len() == 1 {
        format!("{context}: new incident")
    } else {
        format!("{context}: {} new incidents", fresh.len())
    };
    let mut lines: Vec<String> = fresh
        .iter()
        .take(3)
        .map(|b| format!("{} · {}", b.reason, target_label(b)))
        .collect();
    if fresh.len() > 3 {
        lines.push(format!("+{} more", fresh.len() - 3));
    }
    if lines.is_empty() {
        lines.push(format!(
            "Score {} → {}",
            previous.health.score, current.health.score
        ));
    }
    Some(Alert {
        summary,
        body: lines.join("\n"),
        grade: current.health.grade,
        critical: fresh
            .iter()
            .any(|b| b.severity == IncidentSeverity::Critical),
    })
}

fn visible_rows_height(count: usize) -> f32 {
    count.min(MAX_VISIBLE_ROWS) as f32 * ROW_HEIGHT
}

fn empty_row(text: &'static str) -> Div {
    div()
        .px(px(20.))
        .py(px(14.))
        .border_t_1()
        .border_color(theme::divider())
        .text_color(theme::faint())
        .child(text)
}

/// What the snapshot could not see, in one sentence, or `None` if complete.
fn coverage_notice(snapshot: &ClusterSnapshot) -> Option<String> {
    let coverage = snapshot.coverage;
    let mut gaps = Vec::new();
    if !coverage.metrics_available {
        gaps.push("CPU/memory usage is unavailable (no metrics-server)");
    }
    if !coverage.nodes_visible {
        gaps.push("nodes can't be listed with your permissions, so node health isn't scored");
    }
    (!gaps.is_empty()).then(|| format!("Partial data: {}.", gaps.join("; ")))
}

fn node_row(node: &NodeMetric, pods: usize, metrics: bool) -> Div {
    let (status, status_color) = node_status(node);
    table_row()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(MONO)
                .text_size(px(12.))
                .text_color(theme::text_secondary())
                .child(node.name.clone()),
        )
        .child(fixed(
            130.,
            div()
                .text_size(px(12.))
                .text_color(status_color)
                .child(status),
        ))
        .child(fixed(240., usage_cell(node.cpu_pct, metrics)))
        .child(fixed(240., usage_cell(node.memory_pct, metrics)))
        .child(fixed(
            56.,
            mono(pods.to_string()).text_color(theme::muted()),
        ))
}

fn pod_row(
    index: usize,
    row: usize,
    pod: PodInfo,
    all_namespaces: bool,
    metrics: bool,
    cx: &mut Context<DesktopApp>,
) -> Stateful<Div> {
    let (status, color, _) = pod_status(&pod);
    let ready = pod.containers.iter().filter(|c| c.ready).count();
    table_row()
        .id(("pod", row))
        .cursor_pointer()
        .hover(|s| s.bg(theme::selected()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(MONO)
                .text_size(px(12.))
                .child(pod.name.clone()),
        )
        .when(all_namespaces, |d| {
            d.child(fixed(
                150.,
                mono(pod.namespace.clone())
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .truncate(),
            ))
        })
        .child(fixed(
            150.,
            div().text_size(px(12.)).text_color(color).child(status),
        ))
        .child(fixed(
            60.,
            mono(format!("{ready}/{}", pod.containers.len()))
                .text_size(px(12.))
                .text_color(theme::muted()),
        ))
        .child(fixed(
            70.,
            mono(pod.restarts.to_string())
                .text_size(px(12.))
                .text_color(if pod.restarts > 0 {
                    theme::warning()
                } else {
                    theme::muted()
                }),
        ))
        .child(fixed(150., usage_cell(pod.cpu_pct, metrics)))
        .child(fixed(150., usage_cell(pod.memory_pct, metrics)))
        .child(fixed(
            160.,
            mono(pod.node_name.clone().unwrap_or_else(|| "—".into()))
                .text_size(px(12.))
                .text_color(theme::muted())
                .truncate(),
        ))
        .child(fixed(
            56.,
            div()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(pod.age.clone()),
        ))
        .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| app.open_pod(index, &pod, cx)))
}

/// Usage bar, or "n/a" when the cluster has no metrics-server.
fn usage_cell(pct: u8, metrics: bool) -> Div {
    if metrics {
        usage_bar(pct)
    } else {
        div()
            .text_size(px(12.))
            .text_color(theme::faint())
            .child("n/a")
    }
}

/// Status label, colour and a rank for sorting (higher = worse).
fn pod_status(pod: &PodInfo) -> (String, Rgba, u8) {
    if pod.is_completed() {
        ("Completed".into(), theme::faint(), 0)
    } else if pod.is_evicted() {
        ("Evicted".into(), theme::faint(), 0)
    } else if pod.crash_looping {
        ("CrashLoopBackOff".into(), theme::critical(), 4)
    } else if pod.oom_killed {
        ("OOMKilled".into(), theme::critical(), 4)
    } else if pod.phase == "Failed" || pod.phase == "Unknown" {
        (pod.phase.clone(), theme::critical(), 3)
    } else if pod.phase == "Pending" {
        ("Pending".into(), theme::warning(), 2)
    } else if !pod.is_ready {
        ("NotReady".into(), theme::warning(), 2)
    } else {
        (pod.phase.clone(), theme::muted(), 1)
    }
}

fn pod_flags(pod: &PodInfo) -> Option<String> {
    let flags: Vec<&str> = [
        (pod.crash_looping, "crash-looping"),
        (pod.oom_killed, "OOM-killed"),
        (!pod.is_ready, "not ready"),
    ]
    .into_iter()
    .filter_map(|(on, label)| on.then_some(label))
    .collect();
    (!flags.is_empty()).then(|| flags.join(", "))
}

fn kv(label: &'static str, value: String) -> Div {
    div()
        .flex()
        .child(
            div()
                .w(px(110.))
                .flex_shrink_0()
                .text_color(theme::faint())
                .child(label),
        )
        .child(mono(value).min_w_0().truncate())
}

fn millicores(value: u64) -> String {
    if value == 0 {
        "none".into()
    } else {
        format!("{value}m")
    }
}

fn mebibytes(value: u64) -> String {
    if value == 0 {
        "none".into()
    } else {
        format!("{value} MiB")
    }
}

fn resource_block(
    metrics: bool,
    label: &'static str,
    pct: u8,
    used: String,
    bounds: String,
) -> Div {
    let used = if metrics {
        used
    } else {
        "usage n/a".to_string()
    };
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .child(div().font_weight(FontWeight::MEDIUM).child(label))
                .child(mono(used).ml_auto().text_color(theme::text_secondary())),
        )
        .child(usage_cell(pct, metrics))
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme::faint())
                .child(bounds),
        )
}

fn segment(id: &'static str, label: &'static str, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(12.))
        .py(px(4.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(12.))
        .when(active, |d| {
            d.bg(theme::selected()).text_color(theme::text())
        })
        .when(!active, |d| d.text_color(theme::muted()))
        .child(label)
}

fn factor_tile(label: &'static str, hint: &'static str, count: u32) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .p(px(12.))
        .bg(theme::surface_raised())
        .rounded(px(8.))
        .child(
            mono(count.to_string())
                .text_size(px(20.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if count > 0 {
                    theme::warning()
                } else {
                    theme::muted()
                }),
        )
        .child(div().font_weight(FontWeight::MEDIUM).child(label))
        .child(
            div()
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(hint),
        )
}

/// Worst condition first: not ready, pressure, cordoned, else ready.
fn node_status(node: &cluster_core::data::models::NodeMetric) -> (&'static str, Rgba) {
    if !node.ready {
        ("NotReady", theme::critical())
    } else if node.memory_pct >= cluster_core::data::models::RESOURCE_PRESSURE_PCT {
        ("MemoryPressure", theme::critical())
    } else if node.draining {
        ("Draining", theme::warning())
    } else if node.cordoned {
        ("Cordoned", theme::warning())
    } else {
        ("Ready", theme::muted())
    }
}

fn usage_bar(pct: u8) -> Div {
    let color = theme::heat(pct);
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .flex_1()
                .h(px(6.))
                .rounded(px(3.))
                .bg(theme::bg())
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .w(gpui::relative(f32::from(pct.min(100)) / 100.))
                        .bg(color),
                ),
        )
        .child(
            mono(format!("{pct}%"))
                .w(px(34.))
                .text_size(px(12.))
                .text_color(if pct >= 85 {
                    theme::critical()
                } else {
                    theme::muted()
                }),
        )
}

fn namespace_row(
    id: (&'static str, usize),
    name: String,
    pods: Option<usize>,
    selected: bool,
    monospace: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(12.))
        .h(px(40.))
        .px(px(10.))
        .rounded(px(8.))
        .cursor_pointer()
        .when(selected, |d| d.bg(theme::selected()))
        .hover(|s| s.bg(theme::selected()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .when(monospace, |d| d.font_family(MONO))
                .child(name),
        )
        .when_some(pods, |d, pods| {
            d.child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(format!("{pods} pods")),
            )
        })
        .when(selected, |d| {
            d.child(div().text_color(theme::accent()).child("✓"))
        })
}

/// The hexagon mark from the design canvas.
fn logo_icon(size: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let unit = bounds.size.width / 24.;
            let at = |x: f32, y: f32| point(bounds.origin.x + unit * x, bounds.origin.y + unit * y);
            let mut hex = PathBuilder::stroke(px(1.8));
            hex.move_to(at(12., 2.));
            for (x, y) in [
                (21., 7.),
                (21., 17.),
                (12., 22.),
                (3., 17.),
                (3., 7.),
                (12., 2.),
            ] {
                hex.line_to(at(x, y));
            }
            if let Ok(path) = hex.build() {
                window.paint_path(path, theme::accent());
            }
            let mut dot = PathBuilder::stroke(px(1.8));
            dot.move_to(at(15., 12.));
            dot.arc_to(
                point(unit * 3., unit * 3.),
                px(0.),
                false,
                true,
                at(9., 12.),
            );
            dot.arc_to(
                point(unit * 3., unit * 3.),
                px(0.),
                false,
                true,
                at(15., 12.),
            );
            if let Ok(path) = dot.build() {
                window.paint_path(path, theme::accent());
            }
        },
    )
    .size(px(size))
    .flex_shrink_0()
}

/// Padlock for the read-only badge.
fn lock_icon(size: f32, color: Rgba) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let unit = bounds.size.width / 24.;
            let at = |x: f32, y: f32| point(bounds.origin.x + unit * x, bounds.origin.y + unit * y);
            let mut body = PathBuilder::stroke(px(1.8));
            body.move_to(at(4., 11.));
            for (x, y) in [(20., 11.), (20., 21.), (4., 21.), (4., 11.)] {
                body.line_to(at(x, y));
            }
            if let Ok(path) = body.build() {
                window.paint_path(path, color);
            }
            let mut shackle = PathBuilder::stroke(px(1.8));
            shackle.move_to(at(7., 11.));
            shackle.line_to(at(7., 7.));
            shackle.arc_to(
                point(unit * 5., unit * 5.),
                px(0.),
                false,
                true,
                at(17., 7.),
            );
            shackle.line_to(at(17., 11.));
            if let Ok(path) = shackle.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(size))
    .flex_shrink_0()
}

/// Line of score samples scaled to 0–100; a single sample draws flat.
fn sparkline(values: Vec<u8>, color: Rgba, height: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let values = match values.len() {
                0 => return,
                1 => vec![values[0], values[0]],
                _ => values,
            };
            if let Some(path) = polyline(
                &values,
                bounds.origin,
                bounds.size.width,
                bounds.size.height,
            ) {
                window.paint_path(path, color);
            }
        },
    )
    .w_full()
    .h(px(height))
}

/// Trend chart with dashed-free grade threshold lines at 90/75/60/45.
fn score_chart(values: Vec<u8>, height: f32) -> impl IntoElement {
    div()
        .flex()
        .gap(px(10.))
        .child(
            div()
                .relative()
                .w(px(20.))
                .h(px(height))
                .font_family(MONO)
                .text_size(px(11.))
                .text_color(theme::faint())
                .children(
                    [
                        (GRADE_A_THRESHOLD, "A"),
                        (GRADE_B_THRESHOLD, "B"),
                        (GRADE_C_THRESHOLD, "C"),
                        (GRADE_D_THRESHOLD, "D"),
                    ]
                    .into_iter()
                    .map(move |(threshold, label)| {
                        div()
                            .absolute()
                            .right_0()
                            .top(px(height * (1. - f32::from(threshold) / 100.) - 8.))
                            .child(label)
                    }),
                ),
        )
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    for threshold in [
                        GRADE_A_THRESHOLD,
                        GRADE_B_THRESHOLD,
                        GRADE_C_THRESHOLD,
                        GRADE_D_THRESHOLD,
                    ] {
                        let y = bounds.origin.y
                            + bounds.size.height * (1. - f32::from(threshold) / 100.);
                        let mut line = PathBuilder::stroke(px(1.));
                        line.move_to(point(bounds.origin.x, y));
                        line.line_to(point(bounds.origin.x + bounds.size.width, y));
                        if let Ok(path) = line.build() {
                            window.paint_path(path, theme::border_strong());
                        }
                    }
                    let values = match values.len() {
                        0 => return,
                        1 => vec![values[0], values[0]],
                        _ => values,
                    };
                    if let Some(path) = polyline(
                        &values,
                        bounds.origin,
                        bounds.size.width,
                        bounds.size.height,
                    ) {
                        window.paint_path(path, theme::accent());
                    }
                },
            )
            .flex_1()
            .h(px(height)),
        )
}

fn polyline(
    values: &[u8],
    origin: gpui::Point<Pixels>,
    width: Pixels,
    height: Pixels,
) -> Option<gpui::Path<Pixels>> {
    let last = (values.len() - 1) as f32;
    let mut builder = PathBuilder::stroke(px(1.8));
    for (i, value) in values.iter().enumerate() {
        let x = origin.x + width * (i as f32 / last);
        let y = origin.y + height * (1. - f32::from((*value).min(100)) / 100.);
        if i == 0 {
            builder.move_to(point(x, y));
        } else {
            builder.line_to(point(x, y));
        }
    }
    builder.build().ok()
}

//! The desktop window: fleet overview, cluster detail, and the cluster and
//! namespace pickers from the design canvas.

use std::collections::VecDeque;
use std::time::Duration;

use cluster_core::data::models::{
    ClusterSnapshot, ConnectionIssue, IncidentBucket, IncidentSeverity, NamespaceSummary,
    GRADE_A_THRESHOLD, GRADE_B_THRESHOLD, GRADE_C_THRESHOLD, GRADE_D_THRESHOLD,
};
use cluster_core::events::{DataEvent, FetchCommand};
use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, ClickEvent, Context, Div, FocusHandle,
    FontWeight, KeyDownEvent, MouseButton, PathBuilder, Pixels, Rgba, SharedString, Stateful,
    Window,
};
use tokio::sync::mpsc;

use crate::backend::{Backend, TaggedEvent};
use crate::theme::{self, MONO, SANS};

/// Score samples kept per cluster for the trend chart (in memory, since launch).
const MAX_HISTORY: usize = 240;
const FLEET_INCIDENT_ROWS: usize = 8;

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
}

#[derive(Clone, Copy, PartialEq)]
enum Picker {
    None,
    Cluster,
    Namespace,
}

pub struct DesktopApp {
    _backend: Backend,
    focus_handle: FocusHandle,
    clusters: Vec<ClusterState>,
    load_error: Option<String>,
    screen: Screen,
    picker: Picker,
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
            _backend: backend,
            focus_handle,
            clusters,
            load_error,
            screen: Screen::Fleet,
            picker: Picker::None,
        }
    }

    fn apply(&mut self, index: usize, event: DataEvent) {
        let Some(cluster) = self.clusters.get_mut(index) else {
            return;
        };
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

    fn open_cluster(&mut self, index: usize, cx: &mut Context<Self>) {
        self.screen = Screen::Cluster(index);
        self.picker = Picker::None;
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
    /// switcher, `r` refreshes, Esc closes a picker or goes back to the fleet.
    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.secondary() && keystroke.key == "k" {
            self.picker = Picker::Cluster;
        } else if keystroke.modifiers.modified() {
            return;
        } else {
            match keystroke.key.as_str() {
                "escape" if self.picker != Picker::None => self.picker = Picker::None,
                "escape" => self.screen = Screen::Fleet,
                "n" => match self.screen {
                    Screen::Cluster(index) => return self.open_namespace_picker(index, cx),
                    Screen::Fleet => return,
                },
                "r" => self.refresh_all(cx),
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
                    .child(
                        div()
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
                            app.screen = Screen::Fleet;
                            cx.notify();
                        })),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(section_label("Contexts").px(px(10.)).pb(px(2.)))
                    .children(self.clusters.iter().enumerate().map(|(index, cluster)| {
                        let (fg, _) = theme::grade_colors(cluster.grade());
                        let selected = self.screen == Screen::Cluster(index);
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
                    .mt_auto()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .p(px(10.))
                    .border_1()
                    .border_color(theme::border())
                    .rounded(px(8.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(
                        div()
                            .size(px(8.))
                            .rounded_full()
                            .bg(theme::grade_colors('A').0),
                    )
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
                                    .child(format!("ns: {}", cluster.namespace_label())),
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
                    app.screen = Screen::Fleet;
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
                .render_cluster_body(cluster, snapshot)
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
        cluster: &ClusterState,
        snapshot: &ClusterSnapshot,
    ) -> impl IntoElement {
        let health = &snapshot.health;
        let grade = health.grade;

        let trend_panel = panel()
            .flex_1()
            .min_w_0()
            .child(panel_header("Health score", "samples since launch"))
            .child(
                div()
                    .px(px(20.))
                    .pb(px(18.))
                    .child(score_chart(cluster.history.iter().copied().collect())),
            );

        let why_panel = panel()
            .w(px(420.))
            .flex_shrink_0()
            .child(panel_header(
                &format!("Why it's a{} {grade}", article(grade)),
                "",
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .px(px(20.))
                    .pb(px(18.))
                    .child(factor_row(
                        "Critical nodes",
                        "memory ≥85% or not ready",
                        health.critical_nodes,
                    ))
                    .child(factor_row(
                        "Critical pods",
                        "failed, unready, crash-looping or OOM",
                        health.critical_pods,
                    ))
                    .child(factor_row(
                        "Container restarts",
                        "−2 each, capped",
                        health.total_restarts,
                    ))
                    .child(factor_row(
                        "Incident buckets",
                        "ranked below",
                        snapshot.incident_buckets.len() as u32,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .pt(px(10.))
                            .border_t_1()
                            .border_color(theme::border())
                            .child(div().font_weight(FontWeight::SEMIBOLD).child("Score"))
                            .child(
                                mono(format!("{} · {grade}", health.score))
                                    .ml_auto()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::grade_colors(grade).0),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::faint())
                            .child(grade_band_hint(health.score)),
                    ),
            );

        let incidents_panel = panel()
            .flex_1()
            .min_w_0()
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
                        let row = div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .px(px(20.))
                            .py(px(12.))
                            .border_t_1()
                            .border_color(theme::divider())
                            .when(i == 0, |d| d.bg(theme::surface_raised()))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(severity_badge(bucket.severity))
                                    .child(
                                        mono(bucket.reason.clone()).font_weight(FontWeight::MEDIUM),
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

        let nodes_panel = panel()
            .w(px(560.))
            .flex_shrink_0()
            .child(panel_header("Nodes", "pressure line at 85%"))
            .child(table_header(&[
                ("Node", None),
                ("CPU", Some(130.)),
                ("Memory", Some(130.)),
                ("Pods", Some(44.)),
            ]))
            .children(snapshot.nodes.iter().map(|node| {
                let pods = snapshot
                    .pods
                    .iter()
                    .filter(|p| p.node_name.as_deref() == Some(node.name.as_str()))
                    .count();
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
                    .child(fixed(130., usage_bar(node.cpu_pct)))
                    .child(fixed(130., usage_bar(node.memory_pct)))
                    .child(fixed(
                        44.,
                        mono(pods.to_string()).text_color(theme::muted()),
                    ))
            }));

        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(
                div()
                    .flex()
                    .gap(px(20.))
                    .child(trend_panel)
                    .child(why_panel),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(20.))
                    .child(incidents_panel)
                    .child(nodes_panel),
            )
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
            Screen::Cluster(index) => Some(index),
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

fn factor_row(label: &'static str, hint: &'static str, count: u32) -> Div {
    div()
        .flex()
        .items_center()
        .child(label)
        .child(
            div()
                .ml(px(10.))
                .text_size(px(12.))
                .text_color(theme::faint())
                .child(hint),
        )
        .child(mono(count.to_string()).ml_auto().text_color(if count > 0 {
            theme::warning()
        } else {
            theme::muted()
        }))
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
fn score_chart(values: Vec<u8>) -> impl IntoElement {
    let height = 220.;
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

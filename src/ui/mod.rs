pub mod components;
pub mod format;
pub mod theme;
pub mod views;

use ratatui::Frame;

use crate::app::{AppState, AppView};

pub fn render(f: &mut Frame, app: &mut AppState) {
    let area = f.area();
    // Clone view to avoid borrowing app while passing it to render functions
    let view = app.view.clone();
    match view {
        AppView::Dashboard => views::dashboard::render(f, area, app),
        AppView::PodDetail { .. } => views::pod_detail::render(f, area, app),
        AppView::NodeDetail { .. } => {
            views::dashboard::render(f, area, app);
            views::node_detail::render(f, area, app);
        }
    }
}

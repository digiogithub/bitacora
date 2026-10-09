//! Print view of the page on screen (BIT-US-0181): collects the rows the page view shows,
//! renders them with [`crate::print`], writes the file into the app cache and opens it in the
//! default browser, whose print dialog appears on load.

use rust_i18n::t;

use super::Workspace;
use crate::data;
use crate::nav::Route;
use crate::print::{self, PrintDoc};
use crate::render::model::Row;
use crate::ui::{App, Context, Level, Window, notify};
use crate::views::page_view::LoadState;

/// Upper bound of blocks read when the page view only holds its first chunk.
const WHOLE_PAGE: usize = 1_000_000;

impl Workspace {
    /// Whether the route on screen has something to print: a page or a zoomed block of an
    /// open graph (not the settings, the graph view, the journals feed, ...).
    pub fn can_print(&self, cx: &App) -> bool {
        self.graph_handle().is_some()
            && matches!(
                self.main.read(cx).route(),
                Some(Route::Page(_) | Route::PageAt { .. } | Route::Block(_))
            )
    }

    /// Tooltip of the print button.
    pub(super) fn print_tooltip(&self, cx: &App) -> String {
        if self.can_print(cx) {
            t!("print.tip").to_string()
        } else {
            t!("print.nothing").to_string()
        }
    }

    /// `Print` action, button and palette command.
    pub fn print_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_print(cx) {
            notify(window, cx, Level::Info, t!("print.nothing").to_string());
            return;
        }
        match self.build_print(cx) {
            Ok(Some(url)) => {
                cx.open_url(&url);
                notify(window, cx, Level::Info, t!("print.opened").to_string());
            }
            Ok(None) => notify(window, cx, Level::Info, t!("print.not_ready").to_string()),
            Err(error) => notify(
                window,
                cx,
                Level::Error,
                t!("print.failed", error = error).to_string(),
            ),
        }
    }

    /// Renders and writes the print file; the `file://` URL to open, `None` while the page is
    /// still loading.
    fn build_print(&self, cx: &App) -> Result<Option<String>, String> {
        let Some(handle) = self.graph_handle() else {
            return Ok(None);
        };
        let main = self.main.read(cx);
        let Some(route) = main.route() else {
            return Ok(None);
        };
        let view = main.page().read(cx);
        if view.route() != Some(route) || *view.state() != LoadState::Loaded {
            return Ok(None);
        }
        let title = view.title().unwrap_or_default().to_owned();
        let mut rows: Vec<Row> = view.rows().to_vec();
        if view.has_more()
            && let Route::Page(name) | Route::PageAt { page: name, .. } = route
        {
            // The view fetched the first chunk only: read the whole page from the index.
            rows = data::open_page(handle, name, WHOLE_PAGE)?.rows;
        }
        let title = if title.is_empty() {
            match route {
                Route::Page(name) | Route::PageAt { page: name, .. } => name.clone(),
                _ => t!("print.untitled").to_string(),
            }
        } else {
            title
        };
        let html = print::render_html(&PrintDoc {
            title: &title,
            rows: &rows,
            graph_root: &handle.root,
        });
        let dirs = crate::paths::AppDirs::from_project_dirs().map_err(|e| e.to_string())?;
        let path =
            print::write_print_file(&dirs.cache_dir, &title, &html).map_err(|e| e.to_string())?;
        Ok(Some(print::file_url(&path)))
    }

    pub(super) fn print_action(
        &mut self,
        _: &crate::actions::Print,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.print_current(window, cx);
    }
}

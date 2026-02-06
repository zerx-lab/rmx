#![allow(clippy::duplicated_attributes)]
#![cfg(windows)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::progress::Progress;
use gpui_component::Root;
use gpui_component_assets::Assets;

const MIN_DISPLAY_DURATION: Duration = Duration::from_millis(800);
const FAST_DELETE_THRESHOLD: usize = 50;

pub struct DeleteProgress {
    pub total_files: usize,
    pub total_dirs: usize,
    pub deleted_dirs: AtomicUsize,
    pub current_item: parking_lot::Mutex<String>,
    pub is_complete: AtomicBool,
    pub is_cancelled: AtomicBool,
    pub start_time: Instant,
    pub error_count: AtomicUsize,
    pub last_error: parking_lot::Mutex<Option<String>>,
}

impl DeleteProgress {
    pub fn new(total_files: usize, total_dirs: usize) -> Self {
        Self {
            total_files,
            total_dirs,
            deleted_dirs: AtomicUsize::new(0),
            current_item: parking_lot::Mutex::new(String::new()),
            is_complete: AtomicBool::new(false),
            is_cancelled: AtomicBool::new(false),
            start_time: Instant::now(),
            error_count: AtomicUsize::new(0),
            last_error: parking_lot::Mutex::new(None),
        }
    }

    pub fn total_items(&self) -> usize {
        self.total_files + self.total_dirs
    }

    pub fn deleted_dirs_count(&self) -> usize {
        self.deleted_dirs.load(Ordering::Relaxed)
    }

    pub fn progress_percent(&self) -> f32 {
        if self.total_dirs == 0 {
            return 100.0;
        }
        (self.deleted_dirs_count() as f32 / self.total_dirs as f32) * 100.0
    }

    pub fn set_current_item(&self, item: &str) {
        *self.current_item.lock() = item.to_string();
    }

    pub fn mark_complete(&self) {
        self.is_complete.store(true, Ordering::Release);
    }

    pub fn cancel(&self) {
        self.is_cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.is_cancelled.load(Ordering::Acquire)
    }

    pub fn set_error(&self, count: usize, message: Option<String>) {
        self.error_count.store(count, Ordering::Release);
        *self.last_error.lock() = message;
    }

    pub fn has_errors(&self) -> bool {
        self.error_count.load(Ordering::Acquire) > 0
    }

    pub fn get_error_count(&self) -> usize {
        self.error_count.load(Ordering::Acquire)
    }

    pub fn get_last_error(&self) -> Option<String> {
        self.last_error.lock().clone()
    }
}

pub struct DeleteProgressWindow {
    progress: Arc<DeleteProgress>,
    path: PathBuf,
    window_opened_at: Instant,
}

impl DeleteProgressWindow {
    pub fn new(progress: Arc<DeleteProgress>, path: PathBuf) -> Self {
        Self {
            progress,
            path,
            window_opened_at: Instant::now(),
        }
    }

    fn format_path_display(&self) -> String {
        let path_str = self.path.display().to_string();
        if path_str.len() > 50 {
            format!("...{}", &path_str[path_str.len() - 47..])
        } else {
            path_str
        }
    }

    fn should_auto_close(&self) -> bool {
        self.progress.is_complete.load(Ordering::Acquire)
            && self.window_opened_at.elapsed() >= MIN_DISPLAY_DURATION
    }
}

impl Render for DeleteProgressWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let percent = self.progress.progress_percent();
        let deleted_dirs = self.progress.deleted_dirs_count();
        let total_dirs = self.progress.total_dirs;
        let current_item = self.progress.current_item.lock().clone();
        let is_complete = self.progress.is_complete.load(Ordering::Acquire);
        let error_count = self.progress.get_error_count();
        let has_errors = error_count > 0;

        if self.should_auto_close() && !has_errors {
            cx.spawn(async move |_, cx| {
                cx.update(|cx| {
                    cx.quit();
                });
            })
            .detach();
        }

        let current_display = if current_item.len() > 60 {
            format!("...{}", &current_item[current_item.len() - 57..])
        } else {
            current_item
        };

        let (status_text, status_color) = if is_complete && has_errors {
            (
                format!("Completed with {} error(s)", error_count),
                rgb(0xcc0000),
            )
        } else if is_complete {
            ("Deletion complete".to_string(), rgb(0x666666))
        } else {
            (
                format!("Deleting {} of {} folders...", deleted_dirs, total_dirs),
                rgb(0x666666),
            )
        };

        let (icon, icon_color, title) = if is_complete && has_errors {
            ("!", rgb(0xcc0000), "Deletion Completed with Errors")
        } else if is_complete {
            ("", rgb(0x00aa00), "Deletion Complete")
        } else {
            ("", rgb(0x666666), "Deleting...")
        };

        let progress_clone = self.progress.clone();

        let mut container = div()
            .flex()
            .flex_col()
            .size_full()
            .p_4()
            .gap_3()
            .bg(rgb(0xf0f0f0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .size_8()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(icon_color)
                            .child(icon),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x666666))
                                    .child(self.format_path_display()),
                            ),
                    ),
            )
            .child(Progress::new("delete-progress").value(percent))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .child(div().text_xs().text_color(status_color).child(status_text))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x666666))
                            .child(format!("{:.0}%", percent)),
                    ),
            )
            .child(
                div()
                    .h_4()
                    .text_xs()
                    .text_color(rgb(0x888888))
                    .overflow_hidden()
                    .child(current_display),
            );

        if is_complete && has_errors {
            if let Some(error_msg) = self.progress.get_last_error() {
                let display_error = if error_msg.len() > 80 {
                    format!("{}...", &error_msg[..77])
                } else {
                    error_msg
                };
                container = container.child(
                    div()
                        .p_2()
                        .rounded(px(4.0))
                        .bg(rgb(0xffe0e0))
                        .text_xs()
                        .text_color(rgb(0x990000))
                        .child(display_error),
                );
            }
        }

        container.child(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .mt_2()
                .child(if is_complete {
                    Button::new("close")
                        .primary()
                        .label("Close")
                        .on_click(|_, _, cx| {
                            cx.quit();
                        })
                } else {
                    Button::new("cancel")
                        .label("Cancel")
                        .on_click(move |_, _, cx| {
                            progress_clone.cancel();
                            cx.quit();
                        })
                }),
        )
    }
}

pub fn should_show_progress_ui(total_items: usize) -> bool {
    total_items > FAST_DELETE_THRESHOLD
}

pub fn run_progress_window(progress: Arc<DeleteProgress>, path: PathBuf) -> anyhow::Result<()> {
    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        gpui_component::init(cx);

        let progress_clone = progress.clone();
        let path_clone = path.clone();
        let window_bounds = Bounds::centered(None, size(px(400.0), px(200.0)), cx);

        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Delete Progress".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                kind: WindowKind::PopUp,
                is_movable: true,
                ..Default::default()
            };

            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|_| DeleteProgressWindow::new(progress_clone, path_clone));
                cx.new(|cx| Root::new(view, window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();

        cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;

                cx.update(|cx| {
                    cx.refresh_windows();
                });

                let is_complete = progress.is_complete.load(Ordering::Acquire);
                let has_errors = progress.has_errors();
                let enough_time = progress.start_time.elapsed() >= MIN_DISPLAY_DURATION;

                if is_complete && enough_time && !has_errors {
                    cx.update(|cx| {
                        cx.quit();
                    });
                    break;
                }

                if is_complete && has_errors {
                    break;
                }
            }
        })
        .detach();
    });

    Ok(())
}

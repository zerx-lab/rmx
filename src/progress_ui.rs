#![allow(clippy::duplicated_attributes)]
#![cfg(windows)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::progress::Progress;
use gpui_component::{ActiveTheme, IconName, Root, Sizable};
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
    pub errors: parking_lot::Mutex<Vec<String>>,
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
            errors: parking_lot::Mutex::new(Vec::new()),
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

    pub fn set_errors(&self, errors: Vec<String>) {
        self.error_count.store(errors.len(), Ordering::Release);
        *self.errors.lock() = errors;
    }

    pub fn has_errors(&self) -> bool {
        self.error_count.load(Ordering::Acquire) > 0
    }

    pub fn get_error_count(&self) -> usize {
        self.error_count.load(Ordering::Acquire)
    }

    pub fn get_errors(&self) -> Vec<String> {
        self.errors.lock().clone()
    }

    pub fn get_first_error(&self) -> Option<String> {
        self.errors.lock().first().cloned()
    }
}

pub struct DeleteProgressWindow {
    progress: Arc<DeleteProgress>,
    path: PathBuf,
    window_opened_at: Instant,
    resized_for_errors: bool,
}

impl DeleteProgressWindow {
    pub fn new(progress: Arc<DeleteProgress>, path: PathBuf) -> Self {
        Self {
            progress,
            path,
            window_opened_at: Instant::now(),
            resized_for_errors: false,
        }
    }

    fn format_path_display(&self) -> String {
        let path_str = self.path.display().to_string();
        if path_str.len() > 45 {
            format!("...{}", &path_str[path_str.len() - 42..])
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let percent = self.progress.progress_percent();
        let deleted_dirs = self.progress.deleted_dirs_count();
        let total_dirs = self.progress.total_dirs;
        let current_item = self.progress.current_item.lock().clone();
        let is_complete = self.progress.is_complete.load(Ordering::Acquire);
        let error_count = self.progress.get_error_count();
        let has_errors = error_count > 0;

        if is_complete && has_errors && !self.resized_for_errors {
            self.resized_for_errors = true;
            window.resize(size(px(420.0), px(270.0)));
        }

        if self.should_auto_close() && !has_errors {
            cx.spawn(async move |_, cx| {
                cx.update(|cx| {
                    cx.quit();
                });
            })
            .detach();
        }

        let current_display = if current_item.len() > 50 {
            format!("...{}", &current_item[current_item.len() - 47..])
        } else {
            current_item
        };

        let theme = cx.theme();
        let bg = theme.background;
        let fg = theme.foreground;
        let muted_fg = theme.muted_foreground;
        let border = theme.border;
        let danger_color = theme.danger;
        let success_color = theme.success;

        let (icon_name, icon_color, title) = if is_complete && has_errors {
            (IconName::TriangleAlert, danger_color, "删除完成（有错误）")
        } else if is_complete {
            (IconName::CircleCheck, success_color, "删除完成")
        } else {
            (IconName::LoaderCircle, muted_fg, "正在删除...")
        };

        let status_text = if is_complete && has_errors {
            format!("完成，{} 个错误", error_count)
        } else if is_complete {
            "已完成".to_string()
        } else {
            format!("已删除 {} / {} 个目录", deleted_dirs, total_dirs)
        };

        let status_color = if is_complete && has_errors {
            danger_color
        } else {
            muted_fg
        };

        let progress_clone = self.progress.clone();
        let errors_for_copy = self.progress.get_errors();

        let mut content = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(bg)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p_4()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size_10()
                                    .rounded(px(20.0))
                                    .bg(icon_color.opacity(0.1))
                                    .child(if is_complete {
                                        gpui_component::Icon::new(icon_name)
                                            .small()
                                            .text_color(icon_color)
                                            .into_any_element()
                                    } else {
                                        gpui_component::Icon::new(icon_name)
                                            .small()
                                            .text_color(icon_color)
                                            .with_animation(
                                                "spinner",
                                                Animation::new(Duration::from_secs(1)).repeat(),
                                                |icon, delta| {
                                                    icon.transform(Transformation::rotate(
                                                        percentage(delta),
                                                    ))
                                                },
                                            )
                                            .into_any_element()
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(fg)
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted_fg)
                                            .whitespace_nowrap()
                                            .overflow_hidden()
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
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(status_color)
                                    .child(status_text),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted_fg)
                                    .child(format!("{:.0}%", percent)),
                            ),
                    )
                    .child(
                        div()
                            .h_4()
                            .text_xs()
                            .text_color(muted_fg.opacity(0.7))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(current_display),
                    ),
            );

        if is_complete && has_errors {
            if let Some(error_msg) = self.progress.get_first_error() {
                let display_error = if error_msg.len() > 70 {
                    format!("{}...", &error_msg[..67])
                } else {
                    error_msg
                };
                content = content.child(
                    div()
                        .mx_4()
                        .mb_2()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(danger_color.opacity(0.08))
                        .text_xs()
                        .text_color(danger_color)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(display_error),
                );
            }
        }

        content.child(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .items_center()
                .gap_2()
                .mt_auto()
                .px_4()
                .py_3()
                .border_t_1()
                .border_color(border)
                .when(is_complete && has_errors, |this| {
                    this.child(
                        Button::new("copy-errors")
                            .ghost()
                            .label("复制错误")
                            .on_click(move |_, _, cx| {
                                let text = errors_for_copy.join("\n");
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                            }),
                    )
                })
                .child(if is_complete {
                    Button::new("close")
                        .primary()
                        .label("关闭")
                        .on_click(|_, _, cx| {
                            cx.quit();
                        })
                } else {
                    Button::new("cancel")
                        .ghost()
                        .label("取消")
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

pub struct ConfirmState {
    pub confirmed: AtomicBool,
    pub cancelled: AtomicBool,
}

impl Default for ConfirmState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfirmState {
    pub fn new() -> Self {
        Self {
            confirmed: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        }
    }

    pub fn confirm(&self) {
        self.confirmed.store(true, Ordering::Release);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_confirmed(&self) -> bool {
        self.confirmed.load(Ordering::Acquire)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub struct ConfirmDeleteWindow {
    path: PathBuf,
    total_files: usize,
    total_dirs: usize,
    state: Arc<ConfirmState>,
}

impl ConfirmDeleteWindow {
    pub fn new(path: PathBuf, total_files: usize, total_dirs: usize, state: Arc<ConfirmState>) -> Self {
        Self {
            path,
            total_files,
            total_dirs,
            state,
        }
    }

    fn format_path_display(&self) -> String {
        let path_str = self.path.display().to_string();
        if path_str.len() > 45 {
            format!("...{}", &path_str[path_str.len() - 42..])
        } else {
            path_str
        }
    }

    fn format_item_summary(&self) -> String {
        if self.total_dirs == 0 && self.total_files <= 1 {
            return "1 个文件".to_string();
        }
        let mut parts = Vec::new();
        if self.total_files > 0 {
            parts.push(format!("{} 个文件", self.total_files));
        }
        if self.total_dirs > 0 {
            parts.push(format!("{} 个目录", self.total_dirs));
        }
        parts.join("，")
    }
}

impl Render for ConfirmDeleteWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let state_cancel = self.state.clone();
        let path_display = self.format_path_display();
        let item_summary = self.format_item_summary();

        let theme = cx.theme();
        let bg = theme.background;
        let fg = theme.foreground;
        let muted_fg = theme.muted_foreground;
        let border = theme.border;
        let danger_color = theme.danger;

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(bg)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p_4()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size_10()
                                    .rounded(px(20.0))
                                    .bg(danger_color.opacity(0.1))
                                    .child(
                                        gpui_component::Icon::new(IconName::Delete)
                                            .small()
                                            .text_color(danger_color),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(fg)
                                            .child("确认删除"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted_fg)
                                            .child("此操作不可撤销，文件不会进入回收站"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .px_3()
                            .py_2p5()
                            .rounded_md()
                            .border_1()
                            .border_color(border)
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(fg)
                                    .font_weight(FontWeight::MEDIUM)
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(path_display),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted_fg)
                                    .child(item_summary),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .items_center()
                    .gap_2()
                    .mt_auto()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(border)
                    .child(
                        Button::new("cancel-btn")
                            .ghost()
                            .label("取消")
                            .on_click(move |_, _, cx| {
                                state_cancel.cancel();
                                cx.quit();
                            }),
                    )
                    .child(
                        Button::new("confirm-btn")
                            .danger()
                            .label("删除")
                            .icon(IconName::Delete)
                            .on_click(move |_, _, cx| {
                                state.confirm();
                                cx.quit();
                            }),
                    ),
            )
    }
}

/// 显示删除确认对话框，返回用户选择
/// 
/// # Returns
/// - `Ok(true)` if user confirmed deletion
/// - `Ok(false)` if user cancelled
/// - `Err` if dialog failed to launch
pub fn run_confirmation_dialog(
    path: PathBuf,
    total_files: usize,
    total_dirs: usize,
) -> anyhow::Result<bool> {
    let state = Arc::new(ConfirmState::new());
    let state_clone = state.clone();

    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        gpui_component::init(cx);

        let state_inner = state_clone.clone();
        let path_clone = path.clone();
        let window_bounds = Bounds::centered(None, size(px(420.0), px(210.0)), cx);

        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("确认删除".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                window_min_size: Some(size(px(320.0), px(180.0))),
                kind: WindowKind::PopUp,
                is_movable: true,
                ..Default::default()
            };

            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|_| {
                    ConfirmDeleteWindow::new(path_clone, total_files, total_dirs, state_inner)
                });
                cx.new(|cx| Root::new(view, window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });

    Ok(state.is_confirmed())
}

pub fn run_progress_window(progress: Arc<DeleteProgress>, path: PathBuf) -> anyhow::Result<()> {
    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        gpui_component::init(cx);

        let progress_clone = progress.clone();
        let path_clone = path.clone();
        let window_bounds = Bounds::centered(None, size(px(420.0), px(200.0)), cx);

        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("删除进度".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                window_min_size: Some(size(px(320.0), px(180.0))),
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

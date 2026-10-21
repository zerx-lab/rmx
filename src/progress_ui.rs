#![allow(clippy::duplicated_attributes)]
#![cfg(windows)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::progress::Progress;
use gpui_component::theme::Theme;
use gpui_component::{ActiveTheme, IconName, Root, Sizable};
use gpui_component_assets::Assets;

use crate::i18n::{count, fill, plural, strings, Lang};

const MIN_DISPLAY_DURATION: Duration = Duration::from_millis(800);
const FAST_DELETE_THRESHOLD: usize = 50;

/// Widths of the fields that show paths, in logical pixels (the windows are
/// 560 px wide). Used as a budget for the text that has to fit them.
const PATH_LINE_WIDTH: f32 = 440.0; // progress window: path line under the title
const DIALOG_PATH_WIDTH: f32 = 470.0; // confirm dialog: path box
const CELL_WIDTH: f32 = 380.0; // unlock window: table cells
/// Width of the error box in logical pixels, and how many lines the error text
/// may occupy. Longer messages are cut in the middle, so the start of the path
/// and the reason at its end stay visible; the full text is always available
/// through the Copy errors button.
const ERROR_LINE_WIDTH: f32 = 500.0;
const ERROR_MAX_LINES: usize = 5;
/// How often a message is cut shorter when it still wraps into too many lines
/// (a wrapped line ends at the last separator, not at the very edge).
const ERROR_FIT_ATTEMPTS: usize = 4;


/// Measures text the way gpui paints it.
///
/// The windows draw 12 px text (`text_xs` = 0.75 rem) and a width estimated per
/// character from a constant is off by tens of percent: wrapped lines then end
/// far from the right edge of their box. Every character is measured once with
/// the text system and cached, so the layout matches the painted text.
struct TextMeasurements {
    text_system: Arc<WindowTextSystem>,
    text_style: TextStyle,
    font_size: Pixels,
    line_height: Pixels,
    char_widths: HashMap<char, Pixels>,
}

impl TextMeasurements {
    fn new(text_system: Arc<WindowTextSystem>, theme: &Theme, rem_size: Pixels) -> Self {
        let text_style = TextStyle {
            font_family: theme.font_family.clone(),
            font_size: rems(0.75).into(),
            ..Default::default()
        };
        let font_size = text_style.font_size.to_pixels(rem_size);
        let line_height = text_style.line_height_in_pixels(rem_size);

        Self {
            text_system,
            text_style,
            font_size,
            line_height,
            char_widths: HashMap::new(),
        }
    }

    /// The height of one line of this text, which is how much the error box
    /// grows for every extra line.
    fn line_height(&self) -> Pixels {
        self.line_height
    }

    fn char_width(&mut self, character: char) -> Pixels {
        if let Some(width) = self.char_widths.get(&character) {
            return *width;
        }

        let mut buffer = [0; 4];
        let text = character.encode_utf8(&mut buffer);
        let run = self.text_style.to_run(text.len());
        let width = self
            .text_system
            .layout_line(text, self.font_size, &[run], None)
            .width;
        self.char_widths.insert(character, width);
        width
    }

    fn width(&mut self, text: &str) -> Pixels {
        text.chars()
            .map(|character| self.char_width(character))
            .sum()
    }
}


/// Shortens `text` so that it fits `max_width` pixels, cutting the middle out.
///
/// The middle is replaced with "...". This keeps both the start of a path and
/// the file name or error reason at its end readable, and the line fills its
/// field instead of stopping half way.
fn fit_middle(text: &str, max_width: Pixels, measurements: &mut TextMeasurements) -> String {
    if measurements.width(text) <= max_width {
        return text.to_string();
    }

    let ellipsis = "...";
    let budget = max_width - measurements.width(ellipsis);
    if budget <= px(0.0) {
        return ellipsis.to_string();
    }
    let side = budget * 0.5;

    let mut head_end = 0;
    let mut head_width = px(0.0);
    for (index, character) in text.char_indices() {
        let width = measurements.char_width(character);
        if head_width + width > side {
            break;
        }
        head_width += width;
        head_end = index + character.len_utf8();
    }

    let mut tail_start = text.len();
    let mut tail_width = px(0.0);
    for (index, character) in text.char_indices().rev() {
        if index < head_end {
            break;
        }
        let width = measurements.char_width(character);
        if tail_width + width > side {
            break;
        }
        tail_width += width;
        tail_start = index;
    }

    format!("{}{}{}", &text[..head_end], ellipsis, &text[tail_start..])
}

/// Wraps `text` into lines of at most `max_width` pixels.
///
/// gpui counts `.`, `-` and `_` as word characters, so a path such as
/// `target.txttarget.txt` is a single word and the built-in wrapper breaks it
/// only at separators like `\`, which leaves large gaps at the end of a line.
/// Here the break is preferred right after a separator, so paths and words stay
/// readable; a run without separators is broken anywhere instead of leaving a
/// gap.
fn wrap_to_width(text: &str, max_width: Pixels, measurements: &mut TextMeasurements) -> String {
    const SEPARATORS: &[char] = &[' ', '\\', '/', '.', '-', '_', ':', ','];

    let mut wrapped = String::with_capacity(text.len() + text.len() / 40);
    let mut line = String::new();
    let mut line_width = px(0.0);
    let mut last_separator: Option<usize> = None;

    for character in text.chars() {
        let width = measurements.char_width(character);
        if !line.is_empty() && line_width + width > max_width {
            match last_separator {
                Some(index) if index < line.len() => {
                    let rest = line.split_off(index);
                    wrapped.push_str(&line);
                    wrapped.push('\n');
                    line_width = measurements.width(&rest);
                    line = rest;
                }
                _ => {
                    wrapped.push_str(&line);
                    wrapped.push('\n');
                    line.clear();
                    line_width = px(0.0);
                }
            }
            last_separator = None;
        }

        line.push(character);
        line_width += width;
        if SEPARATORS.contains(&character) {
            last_separator = Some(line.len());
        }
    }

    wrapped.push_str(&line);
    wrapped
}

/// Fits an error message into the error box, cutting the middle out when needed.
///
/// A wrapped line ends at the last separator rather than at the very edge, so a
/// message that fills the budget may still wrap into one line too many: such a
/// message is cut shorter and wrapped again.
fn fit_error(error: &str, measurements: &mut TextMeasurements) -> String {
    let line_width = px(ERROR_LINE_WIDTH);
    let mut budget = line_width * ERROR_MAX_LINES as f32;
    let mut wrapped = String::new();

    for _ in 0..ERROR_FIT_ATTEMPTS {
        let fitted = fit_middle(error, budget, measurements);
        wrapped = wrap_to_width(&fitted, line_width, measurements);
        if wrapped.matches('\n').count() < ERROR_MAX_LINES {
            break;
        }
        budget *= 0.85;
    }

    wrapped
}

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
    lang: Lang,
}

impl DeleteProgressWindow {
    pub fn new(progress: Arc<DeleteProgress>, path: PathBuf, lang: Lang) -> Self {
        Self {
            progress,
            path,
            window_opened_at: Instant::now(),
            resized_for_errors: false,
            lang,
        }
    }

    fn format_path_display(&self, measurements: &mut TextMeasurements) -> String {
        fit_middle(&self.path.display().to_string(), px(PATH_LINE_WIDTH), measurements)
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

        let mut measurements =
            TextMeasurements::new(window.text_system().clone(), cx.theme(), window.rem_size());

        let display_error = if is_complete && has_errors {
            self.progress
                .get_first_error()
                .map(|error| fit_error(&error, &mut measurements))
        } else {
            None
        };
        let error_lines = display_error
            .as_ref()
            .map(|error| error.matches('\n').count() + 1);
        let error_line_height = measurements.line_height();
        let current_display = fit_middle(&current_item, px(PATH_LINE_WIDTH), &mut measurements);
        let path_display = self.format_path_display(&mut measurements);

        if let Some(lines) = error_lines {
            if !self.resized_for_errors {
                self.resized_for_errors = true;
                // The error box is wrapped here, so let the window grow with it.
                window.resize(
                    size(px(560.0), px(270.0) + error_line_height * lines.saturating_sub(1) as f32),
                );
            }
        }

        if self.should_auto_close() && !has_errors {
            cx.spawn(async move |_, cx| {
                cx.update(|cx| {
                    cx.quit();
                });
            })
            .detach();
        }

        let theme = cx.theme();
        let bg = theme.background;
        let fg = theme.foreground;
        let muted_fg = theme.muted_foreground;
        let border = theme.border;
        let danger_color = theme.danger;
        let success_color = theme.success;

        let s = strings(self.lang);

        let (icon_name, icon_color, title) = if is_complete && has_errors {
            (IconName::TriangleAlert, danger_color, s.delete_complete_errors)
        } else if is_complete {
            (IconName::CircleCheck, success_color, s.delete_complete)
        } else {
            (IconName::LoaderCircle, muted_fg, s.deleting)
        };

        let status_text = if is_complete && has_errors {
            count(s.done_error_one, s.done_errors_many, error_count)
        } else if is_complete {
            s.completed.to_string()
        } else {
            fill(
                plural(s.deleted_dirs_one, s.deleted_dirs_many, total_dirs),
                &[&deleted_dirs, &total_dirs],
            )
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
                                            .child(path_display),
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

        if let Some(display_error) = display_error {
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
                    .child(display_error),
            );
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
                            .label(s.copy_errors)
                            .on_click(move |_, _, cx| {
                                let text = errors_for_copy.join("\n");
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                            }),
                    )
                })
                .child(if is_complete {
                    Button::new("close")
                        .primary()
                        .label(s.close)
                        .on_click(|_, _, cx| {
                            cx.quit();
                        })
                } else {
                    Button::new("cancel")
                        .ghost()
                        .label(s.cancel)
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
    pub skip_next_confirm: AtomicBool,
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
            skip_next_confirm: AtomicBool::new(false),
        }
    }

    pub fn confirm(&self) {
        self.confirmed.store(true, Ordering::Release);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn set_skip_next_confirm(&self, skip: bool) {
        self.skip_next_confirm.store(skip, Ordering::Release);
    }

    pub fn is_confirmed(&self) -> bool {
        self.confirmed.load(Ordering::Acquire)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn should_skip_next_confirm(&self) -> bool {
        self.skip_next_confirm.load(Ordering::Acquire)
    }
}

pub struct ConfirmDeleteWindow {
    paths: Vec<PathBuf>,
    total_files: usize,
    total_dirs: usize,
    state: Arc<ConfirmState>,
    lang: Lang,
}

impl ConfirmDeleteWindow {
    pub fn new(
        paths: Vec<PathBuf>,
        total_files: usize,
        total_dirs: usize,
        state: Arc<ConfirmState>,
        lang: Lang,
    ) -> Self {
        Self {
            paths,
            total_files,
            total_dirs,
            state,
            lang,
        }
    }

    fn format_path_display(&self, measurements: &mut TextMeasurements) -> String {
        let first = self
            .paths
            .first()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        fit_middle(&first, px(DIALOG_PATH_WIDTH), measurements)
    }

    fn format_item_summary(&self) -> String {
        let s = strings(self.lang);
        if self.paths.len() > 1 {
            return fill(s.items_selected, &[&self.paths.len()]);
        }
        if self.total_dirs == 0 && self.total_files <= 1 {
            return s.files_one.to_string();
        }
        let mut parts = Vec::new();
        if self.total_files > 0 {
            parts.push(count(s.files_one, s.files_many, self.total_files));
        }
        if self.total_dirs > 0 {
            parts.push(count(s.dirs_one, s.dirs_many, self.total_dirs));
        }
        parts.join(s.item_separator)
    }
}

impl Render for ConfirmDeleteWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let state_cancel = self.state.clone();
        let state_checkbox = self.state.clone();
        let mut measurements =
            TextMeasurements::new(_window.text_system().clone(), cx.theme(), _window.rem_size());
        let path_display = self.format_path_display(&mut measurements);
        let item_summary = self.format_item_summary();
        let skip_checked = self.state.should_skip_next_confirm();
        let s = strings(self.lang);

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
                                            .child(s.confirm_deletion),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted_fg)
                                            .child(s.confirm_warning),
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
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .mt_auto()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(border)
                    .child(
                        gpui_component::checkbox::Checkbox::new("skip-confirm-checkbox")
                            .checked(skip_checked)
                            .label(s.dont_ask_again)
                            .with_size(gpui_component::Size::Small)
                            .on_click(move |checked, _, _| {
                                state_checkbox.set_skip_next_confirm(*checked);
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("cancel-btn")
                                    .ghost()
                                    .label(s.cancel)
                                    .on_click(move |_, _, cx| {
                                        state_cancel.cancel();
                                        cx.quit();
                                    }),
                            )
                            .child(
                                Button::new("confirm-btn")
                                    .danger()
                                    .label(s.delete)
                                    .icon(IconName::Delete)
                                    .on_click(move |_, _, cx| {
                                        state.confirm();
                                        cx.quit();
                                    }),
                            ),
                    ),
            )
    }
}

/// 确认对话框的返回结果
pub struct ConfirmResult {
    /// 用户是否确认了删除
    pub confirmed: bool,
    /// 用户是否勾选了"下次不再确认"
    pub skip_next_confirm: bool,
}

/// 显示删除确认对话框，返回用户选择
/// 
/// # Returns
/// - `Ok(ConfirmResult)` with confirmation and skip_confirm state
/// - `Err` if dialog failed to launch
pub fn run_confirmation_dialog(
    paths: Vec<PathBuf>,
    total_files: usize,
    total_dirs: usize,
    lang: Lang,
) -> anyhow::Result<ConfirmResult> {
    let state = Arc::new(ConfirmState::new());
    let state_clone = state.clone();

    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        gpui_component::init(cx);

        let state_inner = state_clone.clone();
        let paths_clone = paths.clone();
        let window_bounds = Bounds::centered(None, size(px(560.0), px(210.0)), cx);

        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(strings(lang).confirm_deletion.into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                window_min_size: Some(size(px(420.0), px(180.0))),
                kind: WindowKind::PopUp,
                is_movable: true,
                ..Default::default()
            };

            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|_| {
                    ConfirmDeleteWindow::new(paths_clone, total_files, total_dirs, state_inner, lang)
                });
                cx.new(|cx| Root::new(view, window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });

    Ok(ConfirmResult {
        confirmed: state.is_confirmed(),
        skip_next_confirm: state.should_skip_next_confirm(),
    })
}

pub fn run_progress_window(
    progress: Arc<DeleteProgress>,
    path: PathBuf,
    lang: Lang,
) -> anyhow::Result<()> {
    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        gpui_component::init(cx);

        let progress_clone = progress.clone();
        let path_clone = path.clone();
        let window_bounds = Bounds::centered(None, size(px(560.0), px(200.0)), cx);

        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(strings(lang).deletion_progress.into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                window_min_size: Some(size(px(420.0), px(180.0))),
                kind: WindowKind::PopUp,
                is_movable: true,
                ..Default::default()
            };

            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|_| DeleteProgressWindow::new(progress_clone, path_clone, lang));
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

/// Shows the progress window in its error state.
///
/// Used when no progress window was displayed at all: a single file or a
/// directory with few items deletes silently, so a failure would otherwise be
/// invisible (the process is started detached and has no console).
pub fn show_error_window(path: PathBuf, message: String, lang: Lang) -> anyhow::Result<()> {
    let progress = Arc::new(DeleteProgress::new(1, 0));
    progress.set_errors(vec![message]);
    progress.mark_complete();
    run_progress_window(progress, path, lang)
}

// ── Unlock mode UI (仿火绒风格) ─────────────────────────────────────────

pub struct UnlockFileInfo {
    pub file_name: String,
    pub full_path: PathBuf,
}

struct KillFailure {
    name: String,
    pid: u32,
    error: String,
}

type KillResult = Arc<parking_lot::Mutex<Option<(usize, Vec<KillFailure>)>>>;

enum UnlockPhase {
    Confirm,
    Working,
    Success { killed: usize },
    Failed { killed: usize, failures: Vec<KillFailure> },
}

pub struct UnlockProgressWindow {
    files: Vec<UnlockFileInfo>,
    locking_processes: Vec<crate::winapi::LockingProcess>,
    phase: UnlockPhase,
    confirm_signal: Arc<AtomicBool>,
    result: KillResult,
    lang: Lang,
}

impl UnlockProgressWindow {
    pub fn new(
        files: Vec<UnlockFileInfo>,
        locking_processes: Vec<crate::winapi::LockingProcess>,
        lang: Lang,
    ) -> Self {
        Self {
            files,
            locking_processes,
            phase: UnlockPhase::Confirm,
            confirm_signal: Arc::new(AtomicBool::new(false)),
            result: Arc::new(parking_lot::Mutex::new(None)),
            lang,
        }
    }

    fn render_process_table(
        &self,
        processes: &[crate::winapi::LockingProcess],
        theme: &Theme,
        measurements: &mut TextMeasurements,
    ) -> Div {
        let fg = theme.foreground;
        let muted_fg = theme.muted_foreground;
        let border = theme.border;
        let s = strings(self.lang);

        let mut table = div()
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(border)
            .max_h(px(120.0))
            .overflow_hidden();

        table = table.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .px_3()
                .py_1p5()
                .bg(theme.secondary.opacity(0.3))
                .border_b_1()
                .border_color(border)
                .child(
                    div()
                        .w(px(120.0))
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(muted_fg)
                        .child(s.name),
                )
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(muted_fg)
                        .child(s.path),
                ),
        );

        for proc in processes {
            let exe_display = proc
                .exe_path
                .as_deref()
                .map(|p| fit_middle(p, px(CELL_WIDTH), measurements))
                .unwrap_or_else(|| format!("PID: {}", proc.pid));

            table = table.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_3()
                    .py_1p5()
                    .border_b_1()
                    .border_color(border.opacity(0.3))
                    .child(
                        div()
                            .w(px(120.0))
                            .text_xs()
                            .text_color(fg)
                            .child(proc.name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(muted_fg)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(exe_display),
                    ),
            );
        }
        table
    }
}

impl Render for UnlockProgressWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // ── 状态转换 ──
        if self.confirm_signal.load(Ordering::Acquire) && matches!(self.phase, UnlockPhase::Confirm) {
            self.phase = UnlockPhase::Working;

            let procs = self.locking_processes.clone();
            let result_slot = self.result.clone();

            cx.spawn(async move |_this, cx| {
                let result = cx.background_executor().spawn(async move {
                    let mut killed = 0usize;
                    let mut failures = Vec::new();

                    for proc in &procs {
                        if proc.pid == 0 || proc.pid == 4 {
                            continue;
                        }
                        match crate::winapi::kill_process(proc.pid) {
                            Ok(()) => killed += 1,
                            Err(e) => failures.push(KillFailure {
                                name: proc.name.clone(),
                                pid: proc.pid,
                                error: e.to_string(),
                            }),
                        }
                    }

                    (killed, failures)
                }).await;

                *result_slot.lock() = Some(result);

                cx.update(|cx| {
                    cx.refresh_windows();
                });
            }).detach();
        }

        if matches!(self.phase, UnlockPhase::Working) {
            if let Some((killed, failures)) = self.result.lock().take() {
                if failures.is_empty() {
                    self.phase = UnlockPhase::Success { killed };
                } else {
                    self.phase = UnlockPhase::Failed { killed, failures };
                    window.resize(size(px(560.0), px(380.0)));
                }
            }
        }

        let processes = self.locking_processes.clone();
        let files = &self.files;

        let theme = cx.theme();
        let mut measurements =
            TextMeasurements::new(window.text_system().clone(), theme, window.rem_size());
        let bg = theme.background;
        let fg = theme.foreground;
        let muted_fg = theme.muted_foreground;
        let border = theme.border;
        let warning_color = theme.warning;
        let success_color = theme.success;
        let danger_color = theme.danger;

        let s = strings(self.lang);
        let file_count = files.len();

        let mut content = div().flex().flex_col().size_full().bg(bg);

        // ── Header ──
        match &self.phase {
            UnlockPhase::Confirm => {
                content = content.child(
                    self.render_header(fg, muted_fg, s.unlock_files, s.unlock_help, None),
                );
            }
            UnlockPhase::Working => {
                content = content.child(
                    self.render_header(fg, muted_fg, s.unlocking, s.terminating_locking, Some(muted_fg)),
                );
            }
            UnlockPhase::Success { killed } => {
                content = content.child(
                    div()
                        .flex().flex_row().items_center().px_4().pt_4().pb_2()
                        .child(
                            div().flex().flex_col().gap_1()
                                .child(
                                    div().flex().flex_row().items_center().gap_2()
                                        .child(
                                            gpui_component::Icon::new(IconName::CircleCheck)
                                                .xsmall()
                                                .text_color(success_color),
                                        )
                                        .child(
                                            div().text_base().font_weight(FontWeight::BOLD)
                                                .text_color(fg).child(s.unlock_successful),
                                        ),
                                )
                                .child(
                                    div().text_xs().text_color(muted_fg)
                                        .child(count(s.terminated_one, s.terminated_many, *killed)),
                                ),
                        ),
                );
            }
            UnlockPhase::Failed { killed, failures } => {
                content = content.child(
                    div()
                        .flex().flex_row().items_center().px_4().pt_4().pb_2()
                        .child(
                            div().flex().flex_col().gap_1()
                                .child(
                                    div().flex().flex_row().items_center().gap_2()
                                        .child(
                                            gpui_component::Icon::new(IconName::TriangleAlert)
                                                .xsmall()
                                                .text_color(danger_color),
                                        )
                                        .child(
                                            div().text_base().font_weight(FontWeight::BOLD)
                                                .text_color(fg).child(s.some_unlocks_failed),
                                        ),
                                )
                                .child(
                                    div().text_xs().text_color(muted_fg)
                                        .child(fill(s.succeeded_failed, &[&killed, &failures.len()])),
                                ),
                        ),
                );
            }
        }

        // ── 文件列表 (Confirm / Working) ──
        if matches!(self.phase, UnlockPhase::Confirm | UnlockPhase::Working) {
            content = content.child(
                div().px_4().py_1().text_xs().text_color(muted_fg)
                    .child(count(
                        s.unlock_following_one,
                        s.unlock_following_many,
                        file_count,
                    )),
            );

            let mut file_list = div()
                .flex().flex_col().mx_4().rounded_md()
                .border_1().border_color(border)
                .max_h(px(120.0)).overflow_hidden();

            file_list = file_list.child(
                div().flex().flex_row().items_center().px_3().py_1p5()
                    .bg(theme.secondary.opacity(0.3)).border_b_1().border_color(border)
                    .child(div().flex_1().text_xs().font_weight(FontWeight::MEDIUM).text_color(muted_fg).child(s.file_folder_name))
                    .child(div().w(px(60.0)).text_xs().font_weight(FontWeight::MEDIUM).text_color(muted_fg).text_right().child(s.status)),
            );

            let status_text = if matches!(self.phase, UnlockPhase::Working) { s.status_unlocking } else { s.status_pending };
            for file in files {
                file_list = file_list.child(
                    div().flex().flex_row().items_center().px_3().py_1p5()
                        .border_b_1().border_color(border.opacity(0.3))
                        .child(div().flex_1().text_xs().text_color(fg).overflow_hidden().whitespace_nowrap().child(file.file_name.clone()))
                        .child(div().w(px(60.0)).text_xs().text_color(warning_color).text_right().child(status_text)),
                );
            }

            content = content.child(file_list);
        }

        // ── 锁定进程详情 (Confirm / Working) ──
        if matches!(self.phase, UnlockPhase::Confirm | UnlockPhase::Working) && !processes.is_empty() {
            let first_file_name = files.first().map(|f| f.file_name.clone()).unwrap_or_default();
            content = content.child(
                div().flex().flex_col().gap_1().px_4().pt_3()
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(fg)
                        .child(fill(s.locked_by, &[&first_file_name])))
                    .child(self.render_process_table(&processes, theme, &mut measurements)),
            );
        }

        // ── 失败详情 ──
        if let UnlockPhase::Failed { failures, .. } = &self.phase {
            let mut fail_list = div()
                .flex().flex_col().mx_4().mt_2().rounded_md()
                .border_1().border_color(danger_color.opacity(0.3))
                .max_h(px(150.0)).overflow_hidden();

            fail_list = fail_list.child(
                div().flex().flex_row().items_center().px_3().py_1p5()
                    .bg(danger_color.opacity(0.08)).border_b_1().border_color(danger_color.opacity(0.2))
                    .child(div().w(px(120.0)).text_xs().font_weight(FontWeight::MEDIUM).text_color(danger_color).child(s.process))
                    .child(div().flex_1().text_xs().font_weight(FontWeight::MEDIUM).text_color(danger_color).child(s.reason)),
            );

            for f in failures {
                fail_list = fail_list.child(
                    div().flex().flex_row().items_center().px_3().py_1p5()
                        .border_b_1().border_color(border.opacity(0.3))
                        .child(div().w(px(120.0)).text_xs().text_color(fg).child(format!("{} ({})", f.name, f.pid)))
                        .child(div().flex_1().text_xs().text_color(danger_color).overflow_hidden().whitespace_nowrap().child(f.error.clone())),
                );
            }

            content = content.child(fail_list);
        }

        // ── 底部按钮 ──
        content = content.child(
            div().flex().flex_row().justify_end().items_center().gap_2()
                .mt_auto().px_4().py_3().border_t_1().border_color(border)
                .when(matches!(self.phase, UnlockPhase::Confirm), |this| {
                    let signal = self.confirm_signal.clone();
                    this.child(
                        Button::new("unlock-btn").primary().label(s.unlock_all)
                            .on_click(move |_, _, cx| {
                                signal.store(true, Ordering::Release);
                                cx.refresh_windows();
                            }),
                    )
                    .child(
                        Button::new("cancel-btn").ghost().label(s.cancel)
                            .on_click(|_, _, cx| { cx.quit(); }),
                    )
                })
                .when(matches!(self.phase, UnlockPhase::Working), |this| {
                    this.child(div().text_xs().text_color(muted_fg).child(s.processing))
                })
                .when(matches!(self.phase, UnlockPhase::Success { .. }), |this| {
                    this.child(
                        Button::new("close-btn-ok").primary().label(s.ok)
                            .on_click(|_, _, cx| { cx.quit(); }),
                    )
                })
                .when(matches!(self.phase, UnlockPhase::Failed { .. }), |this| {
                    this.child(
                        Button::new("close-btn").primary().label(s.close)
                            .on_click(|_, _, cx| { cx.quit(); }),
                    )
                }),
        );

        content
    }
}

impl UnlockProgressWindow {
    fn render_header(
        &self,
        fg: Hsla,
        muted_fg: Hsla,
        title: &str,
        subtitle: &str,
        spinner_color: Option<Hsla>,
    ) -> Div {
        div()
            .flex().flex_row().items_center().px_4().pt_4().pb_2()
            .child(
                div().flex().flex_col().gap_1()
                    .child(
                        div().flex().flex_row().items_center().gap_2()
                            .child(
                                div().text_base().font_weight(FontWeight::BOLD)
                                    .text_color(fg).child(title.to_string()),
                            )
                            .when_some(spinner_color, |this, color| {
                                this.child(
                                    gpui_component::Icon::new(IconName::LoaderCircle)
                                        .xsmall()
                                        .text_color(color)
                                        .with_animation(
                                            "spinner",
                                            Animation::new(Duration::from_secs(1)).repeat(),
                                            |icon, delta| {
                                                icon.transform(Transformation::rotate(percentage(delta)))
                                            },
                                        ),
                                )
                            }),
                    )
                    .child(
                        div().text_xs().text_color(muted_fg).child(subtitle.to_string()),
                    ),
            )
    }
}

pub struct NoLockWindow {
    _path: PathBuf,
    lang: Lang,
}

impl NoLockWindow {
    pub fn new(path: PathBuf, lang: Lang) -> Self {
        Self { _path: path, lang }
    }
}

impl Render for NoLockWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.background;
        let fg = theme.foreground;
        let border = theme.border;
        let warning_color = theme.warning;
        let s = strings(self.lang);

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(bg)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .flex_1()
                    .gap_3()
                    .p_6()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size_12()
                            .rounded_full()
                            .border_2()
                            .border_color(warning_color)
                            .child(
                                div()
                                    .text_color(warning_color)
                                    .font_weight(FontWeight::BOLD)
                                    .text_xl()
                                    .child("!"),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(fg)
                            .child(s.no_locked_files),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(border)
                    .child(
                        Button::new("ok-btn")
                            .primary()
                            .label(s.ok)
                            .on_click(|_, _, cx| {
                                cx.quit();
                            }),
                    ),
            )
    }
}

pub fn run_unlock_dialog(
    path: PathBuf,
    files: Vec<UnlockFileInfo>,
    locking_processes: Vec<crate::winapi::LockingProcess>,
    lang: Lang,
) -> anyhow::Result<()> {
    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        gpui_component::init(cx);

        let path_clone = path.clone();

        if locking_processes.is_empty() {
            let window_bounds = Bounds::centered(None, size(px(380.0), px(200.0)), cx);

            cx.spawn(async move |cx| {
                let window_options = WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(strings(lang).unlock_files.into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                    window_min_size: Some(size(px(300.0), px(150.0))),
                    kind: WindowKind::PopUp,
                    is_movable: true,
                    ..Default::default()
                };

                cx.open_window(window_options, |window, cx| {
                    let view = cx.new(|_| NoLockWindow::new(path_clone, lang));
                    cx.new(|cx| Root::new(view, window, cx))
                })?;

                Ok::<_, anyhow::Error>(())
            })
            .detach();
        } else {
            let procs_clone = locking_processes.clone();
            let proc_count = locking_processes.len();
            let file_count = files.len();
            let base_height = 280;
            let file_rows_height = std::cmp::min(file_count as i32, 3) * 28;
            let proc_rows_height = std::cmp::min(proc_count as i32, 4) * 28;
            let window_height = std::cmp::min(
                520,
                base_height + file_rows_height + proc_rows_height,
            ) as f32;
            let window_bounds = Bounds::centered(None, size(px(560.0), px(window_height)), cx);

            cx.spawn(async move |cx| {
                let window_options = WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(strings(lang).unlock_files.into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                    window_min_size: Some(size(px(420.0), px(300.0))),
                    kind: WindowKind::PopUp,
                    is_movable: true,
                    ..Default::default()
                };

                cx.open_window(window_options, |window, cx| {
                    let view = cx.new(|_| {
                        UnlockProgressWindow::new(files, procs_clone, lang)
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                })?;

                Ok::<_, anyhow::Error>(())
            })
            .detach();
        }
    });

    Ok(())
}

---
name: gpui-patterns
description: Common UI patterns and advanced techniques for GPUI applications. Use when implementing modals, lists, forms, state sharing, or complex component compositions.
---

# GPUI Patterns

This skill provides common UI patterns and advanced techniques for GPUI applications.

## Modal/Overlay Pattern

### Basic Modal

```rust
use gpui::*;

struct Modal {
    is_open: bool,
    content: SharedString,
}

impl Render for Modal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .when(self.is_open, |this| {
                this.absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba(0, 0, 0, 0.5))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.close(cx);
                    }))
                    .child(
                        div()
                            .p_6()
                            .bg(rgb(0x1a1a1a))
                            .rounded_lg()
                            .min_w(px(300.0))
                            .child(self.content.clone())
                    )
            })
    }
}

impl Modal {
    fn open(&mut self, content: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.is_open = true;
        self.content = content.into();
        cx.notify();
    }
    
    fn close(&mut self, cx: &mut Context<Self>) {
        self.is_open = false;
        cx.notify();
    }
}
```

## List Pattern

### Dynamic List Rendering

```rust
struct ListView {
    items: Vec<String>,
}

impl Render for ListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_2()
            .children(
                self.items.iter().enumerate().map(|(index, item)| {
                    self.render_item(index, item, cx)
                })
            )
    }
}

impl ListView {
    fn render_item(
        &self,
        index: usize,
        item: &str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        div()
            .p_3()
            .bg(rgb(0x1a1a1a))
            .rounded(px(4.0))
            .flex()
            .justify_between()
            .child(format!("{}. {}", index + 1, item))
            .child(
                div()
                    .px_3()
                    .py_1()
                    .bg(rgb(0xef4444))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .child("Delete")
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.remove_item(index, cx);
                    }))
            )
    }
    
    fn remove_item(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.items.len() {
            self.items.remove(index);
            cx.notify();
        }
    }
}
```

## Form Pattern

### Form with Validation

```rust
struct LoginForm {
    username: SharedString,
    password: SharedString,
    error: Option<SharedString>,
}

impl Render for LoginForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_4()
            .p_6()
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .p_3()
                        .bg(rgb(0xef4444))
                        .rounded(px(4.0))
                        .child(error)
                )
            })
            .child(self.render_input("Username", &self.username, cx))
            .child(self.render_input("Password", &self.password, cx))
            .child(
                div()
                    .px_6()
                    .py_3()
                    .bg(rgb(0x3b82f6))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .child("Login")
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.submit(cx);
                    }))
            )
    }
}

impl LoginForm {
    fn render_input(
        &self,
        label: &str,
        value: &SharedString,
        _cx: &Context<Self>,
    ) -> impl IntoElement {
        div()
            .v_flex()
            .gap_1()
            .child(
                div().text_sm().child(label)
            )
            .child(
                div()
                    .w_full()
                    .px_3()
                    .py_2()
                    .bg(rgb(0x1a1a1a))
                    .border_1()
                    .border_color(rgb(0x4a4a4a))
                    .rounded(px(4.0))
                    .child(value.clone())
            )
    }
    
    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.username.is_empty() || self.password.is_empty() {
            self.error = Some("Please fill in all fields".into());
            cx.notify();
            return;
        }
        
        self.error = None;
        // Handle login...
    }
}
```

## Global State Pattern

### Shared Application State

```rust
#[derive(Clone)]
struct AppState {
    user: Option<String>,
    theme: String,
}

// Set global state
fn init_app_state(cx: &mut App) {
    let state = AppState {
        user: None,
        theme: "dark".to_string(),
    };
    cx.set_global(state);
}

// Access global state
fn use_app_state(cx: &App) -> AppState {
    cx.global::<AppState>().clone()
}

// Update global state
fn set_user(cx: &mut App, user: String) {
    let mut state = cx.global::<AppState>().clone();
    state.user = Some(user);
    cx.set_global(state);
}
```

## Parent-Child Communication

### Child Notifying Parent

```rust
#[derive(Clone, Debug)]
enum ChildEvent {
    ValueChanged(i32),
}

impl EventEmitter<ChildEvent> for Child {}

struct Parent {
    child: Entity<Child>,
    _subscription: Subscription,
}

impl Parent {
    fn new(cx: &mut Context<Self>) -> Self {
        let child = cx.new(|_| Child::new());
        
        let subscription = cx.subscribe(&child, |this, _child, event, cx| {
            match event {
                ChildEvent::ValueChanged(value) => {
                    println!("Child value changed to: {}", value);
                    cx.notify();
                }
            }
        });
        
        Self {
            child,
            _subscription: subscription,
        }
    }
}

struct Child {
    value: i32,
}

impl Child {
    fn new() -> Self {
        Self { value: 0 }
    }
    
    fn set_value(&mut self, value: i32, cx: &mut Context<Self>) {
        self.value = value;
        cx.emit(ChildEvent::ValueChanged(value));
        cx.notify();
    }
}
```

## Tab Navigation

### Tab View Pattern

```rust
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Home,
    Settings,
    Profile,
}

struct TabView {
    active_tab: Tab,
}

impl Render for TabView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .size_full()
            .child(self.render_tabs(cx))
            .child(self.render_content())
    }
}

impl TabView {
    fn render_tabs(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .h_flex()
            .gap_2()
            .p_2()
            .bg(rgb(0x1a1a1a))
            .child(self.render_tab("Home", Tab::Home, cx))
            .child(self.render_tab("Settings", Tab::Settings, cx))
            .child(self.render_tab("Profile", Tab::Profile, cx))
    }
    
    fn render_tab(&self, label: &str, tab: Tab, cx: &Context<Self>) -> impl IntoElement {
        let is_active = self.active_tab == tab;
        
        div()
            .px_4()
            .py_2()
            .rounded(px(4.0))
            .cursor_pointer()
            .bg(if is_active { rgb(0x3b82f6) } else { rgb(0x2a2a2a) })
            .child(label)
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.active_tab = tab;
                cx.notify();
            }))
    }
    
    fn render_content(&self) -> impl IntoElement {
        match self.active_tab {
            Tab::Home => div().child("Home Content"),
            Tab::Settings => div().child("Settings Content"),
            Tab::Profile => div().child("Profile Content"),
        }
    }
}
```

## Loading State Pattern

### Async Data Loading

```rust
enum LoadState<T> {
    Idle,
    Loading,
    Loaded(T),
    Error(String),
}

struct DataView {
    state: LoadState<Vec<String>>,
}

impl Render for DataView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.state {
            LoadState::Idle => {
                div().child("Click to load")
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.load_data(cx);
                    }))
            }
            LoadState::Loading => {
                div().child("Loading...")
            }
            LoadState::Loaded(items) => {
                div()
                    .v_flex()
                    .gap_2()
                    .children(items.iter().map(|item| {
                        div().child(item.clone())
                    }))
            }
            LoadState::Error(error) => {
                div()
                    .p_3()
                    .bg(rgb(0xef4444))
                    .child(format!("Error: {}", error))
            }
        }
    }
}

impl DataView {
    fn load_data(&mut self, cx: &mut Context<Self>) {
        self.state = LoadState::Loading;
        cx.notify();
        
        cx.spawn(async move |this, cx| {
            match fetch_data().await {
                Ok(data) => {
                    this.update(&mut *cx, |view, cx| {
                        view.state = LoadState::Loaded(data);
                        cx.notify();
                    })?;
                }
                Err(e) => {
                    this.update(&mut *cx, |view, cx| {
                        view.state = LoadState::Error(e.to_string());
                        cx.notify();
                    })?;
                }
            }
            
            Ok(())
        }).detach();
    }
}

async fn fetch_data() -> Result<Vec<String>, anyhow::Error> {
    // Simulate API call
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok(vec!["Item 1".to_string(), "Item 2".to_string()])
}
```

## Dropdown Pattern

```rust
struct Dropdown {
    is_open: bool,
    selected: Option<String>,
    options: Vec<String>,
}

impl Render for Dropdown {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .child(
                div()
                    .px_4()
                    .py_2()
                    .bg(rgb(0x1a1a1a))
                    .border_1()
                    .border_color(rgb(0x4a4a4a))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .child(self.selected.clone().unwrap_or("Select...".into()))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.is_open = !this.is_open;
                        cx.notify();
                    }))
            )
            .when(self.is_open, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(40.0))
                        .left_0()
                        .w_full()
                        .bg(rgb(0x1a1a1a))
                        .border_1()
                        .border_color(rgb(0x4a4a4a))
                        .rounded(px(4.0))
                        .v_flex()
                        .children(
                            self.options.iter().map(|option| {
                                let option = option.clone();
                                div()
                                    .px_4()
                                    .py_2()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(0x2a2a2a)))
                                    .child(option.clone())
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.selected = Some(option.clone());
                                        this.is_open = false;
                                        cx.notify();
                                    }))
                            })
                        )
                )
            })
    }
}
```

## Production Component Patterns

> [!IMPORTANT]
> **gpui-component is optional**. These patterns show how the library implements components, but you can build the same functionality with pure GPUI code. Use gpui-component for convenience, or implement patterns yourself for full control.

These patterns are based on real implementations from [gpui-component](https://github.com/longbridge/gpui-component).

### Builder Pattern with Trait Methods

Create fluent APIs using traits:

```rust
use gpui::*;

pub trait ButtonVariants: Sized {
    fn with_variant(self, variant: ButtonVariant) -> Self;
    
    fn primary(self) -> Self {
        self.with_variant(ButtonVariant::Primary)
    }
    
    fn danger(self) -> Self {
        self.with_variant(ButtonVariant::Danger)
    }
    
    fn ghost(self) -> Self {
        self.with_variant(ButtonVariant::Ghost)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    Primary,
    #[default]
    Secondary,
    Danger,
    Ghost,
}

impl ButtonVariants for Button {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
}

// Usage
Button::new("save")
    .primary()
    .label("Save")
    .on_click(|_, _, _| {})
```

### Component Structure Pattern

Production-ready component with all common features:

```rust
use gpui::*;
use std::rc::Rc;

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    base: Stateful<Div>,
    style: StyleRefinement,
    
    // Content
    icon: Option<Icon>,
    label: Option<SharedString>,
    children: Vec<AnyElement>,
    
    // State
    disabled: bool,
    selected: bool,
    loading: bool,
    variant: ButtonVariant,
    size: Size,
    
    // Callbacks
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    on_hover: Option<Rc<dyn Fn(&bool, &mut Window, &mut App)>>,
    
    // Features
    tooltip: Option<(SharedString, Option<Box<dyn Action>>)>,
    
    // Focus
    tab_index: isize,
    tab_stop: bool,
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            base: div().id(id),
            style: StyleRefinement::default(),
            icon: None,
            label: None,
            children: Vec::new(),
            disabled: false,
            selected: false,
            loading: false,
            variant: ButtonVariant::default(),
            size: Size::Medium,
            on_click: None,
            on_hover: None,
            tooltip: None,
            tab_index: 0,
            tab_stop: true,
        }
    }
    
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }
    
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
    
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
    
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some((tooltip.into(), None));
        self
    }
}

// Trait implementations
impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for Button {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for Button {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}
```

### Variant System with States

Complete variant system handling all interaction states:

```rust
struct ButtonVariantStyle {
    bg: Hsla,
    border: Hsla,
    fg: Hsla,
    shadow: bool,
}

impl ButtonVariant {
    fn normal(&self, outline: bool, cx: &mut App) -> ButtonVariantStyle {
        let bg = if outline {
            cx.theme().background
        } else {
            match self {
                Self::Primary => cx.theme().primary,
                Self::Secondary => cx.theme().secondary,
                Self::Danger => cx.theme().danger,
                Self::Ghost => cx.theme().transparent,
            }
        };
        
        let fg = match self {
            Self::Primary if outline => cx.theme().primary,
            Self::Primary => cx.theme().primary_foreground,
            Self::Danger if outline => cx.theme().danger,
            Self::Danger => cx.theme().danger_foreground,
            _ => cx.theme().foreground,
        };
        
        let border = if outline {
            match self {
                Self::Primary => cx.theme().primary,
                Self::Danger => cx.theme().danger,
                _ => cx.theme().border,
            }
        } else {
            bg
        };
        
        ButtonVariantStyle {
            bg,
            border,
            fg,
            shadow: matches!(self, Self::Primary | Self::Secondary),
        }
    }
    
    fn hovered(&self, outline: bool, cx: &mut App) -> ButtonVariantStyle {
        let bg = match self {
            Self::Primary if outline => cx.theme().primary.opacity(0.1),
            Self::Primary => cx.theme().primary_hover,
            Self::Danger if outline => cx.theme().danger.opacity(0.1),
            Self::Danger => cx.theme().danger_hover,
            Self::Secondary => cx.theme().secondary_hover,
            Self::Ghost => cx.theme().element_hover,
        };
        
        let mut style = self.normal(outline, cx);
        style.bg = bg;
        style
    }
    
    fn active(&self, outline: bool, cx: &mut App) -> ButtonVariantStyle {
        let bg = match self {
            Self::Primary => cx.theme().primary_active,
            Self::Danger => cx.theme().danger_active,
            Self::Secondary => cx.theme().secondary_active,
            Self::Ghost => cx.theme().element_active,
        };
        
        let mut style = self.normal(outline, cx);
        style.bg = bg;
        style
    }
    
    fn disabled(&self, outline: bool, cx: &mut App) -> ButtonVariantStyle {
        let mut style = self.normal(outline, cx);
        style.bg = style.bg.opacity(0.5);
        style.fg = style.fg.opacity(0.5);
        style.border = style.border.opacity(0.5);
        style.shadow = false;
        style
    }
}
```

### RenderOnce Implementation

Complete render implementation with all states:

```rust
impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_clickable = !self.disabled && !self.loading && self.on_click.is_some();
        let normal_style = self.variant.normal(false, cx);
        
        // Get or create focus handle
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let is_focused = focus_handle.is_focused(window);
        
        self.base
            .when(!self.disabled, |this| {
                this.track_focus(&focus_handle.tab_index(self.tab_index).tab_stop(self.tab_stop))
            })
            .flex()
            .items_center()
            .justify_center()
            .cursor_default()
            .when(cx.theme().shadow && normal_style.shadow, |this| {
                this.shadow_xs()
            })
            // Sizing based on whether it's icon-only or has label
            .when(!self.label.is_some() && self.children.is_empty(), |this| {
                // Icon button - square
                match self.size {
                    Size::XSmall => this.size_5(),
                    Size::Small => this.size_6(),
                    _ => this.size_8(),
                }
            })
            .when(self.label.is_some() || !self.children.is_empty(), |this| {
                // Normal button - rectangular
                match self.size {
                    Size::XSmall => this.h_5().px_1(),
                    Size::Small => this.h_6().px_3(),
                    _ => this.h_8().px_4(),
                }
            })
            .rounded_md()
            .border_1()
            .text_color(normal_style.fg)
            .border_color(normal_style.border)
            .bg(normal_style.bg)
            .when(self.selected, |this| {
                let selected_style = self.variant.active(false, cx);
                this.bg(selected_style.bg)
                    .border_color(selected_style.border)
                    .text_color(selected_style.fg)
            })
            .when(!self.disabled && !self.selected, |this| {
                this.hover(|this| {
                    let hover_style = self.variant.hovered(false, cx);
                    this.bg(hover_style.bg)
                        .border_color(hover_style.border)
                        .text_color(hover_style.fg)
                })
                .active(|this| {
                    let active_style = self.variant.active(false, cx);
                    this.bg(active_style.bg)
                        .border_color(active_style.border)
                        .text_color(active_style.fg)
                })
            })
            .when(self.disabled, |this| {
                let disabled_style = self.variant.disabled(false, cx);
                this.bg(disabled_style.bg)
                    .text_color(disabled_style.fg)
                    .border_color(disabled_style.border)
                    .shadow_none()
            })
            .refine_style(&self.style)
            .when_some(self.on_click, |this, on_click| {
                this.on_click(move |event, window, cx| {
                    if is_clickable {
                        on_click(event, window, cx);
                    } else {
                        cx.stop_propagation();
                    }
                })
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .when(!self.loading, |this| {
                        this.when_some(self.icon, |this, icon| {
                            this.child(icon.with_size(self.size))
                        })
                    })
                    .when(self.loading, |this| {
                        this.child(Spinner::new().with_size(self.size))
                    })
                    .when_some(self.label, |this, label| {
                        this.child(div().line_height(relative(1.)).child(label))
                    })
                    .children(self.children)
            )
            .when_some(self.tooltip, |this, (tooltip, action)| {
                this.tooltip(move |window, cx| {
                    Tooltip::new(tooltip.clone())
                        .when_some(action.clone(), |this, action| {
                            this.action(action.as_ref(), None)
                        })
                        .build(window, cx)
                })
            })
            .focus_ring(is_focused, px(0.), window, cx)
    }
}
```

### Children Management (TabBar Pattern)

Using SmallVec for performance with dynamic children:

```rust
use smallvec::SmallVec;

#[derive(IntoElement)]
pub struct TabBar {
    base: Stateful<Div>,
    children: SmallVec<[Tab; 2]>, // Optimize for 2 tabs on stack
    selected_index: Option<usize>,
    variant: TabVariant,
    size: Size,
    on_click: Option<Rc<dyn Fn(&usize, &mut Window, &mut App) + 'static>>,
}

impl TabBar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id),
            children: SmallVec::new(),
            selected_index: None,
            variant: TabVariant::default(),
            size: Size::default(),
            on_click: None,
        }
    }
    
    pub fn children(mut self, children: impl IntoIterator<Item = impl Into<Tab>>) -> Self {
        self.children.extend(children.into_iter().map(Into::into));
        self
    }
    
    pub fn child(mut self, child: impl Into<Tab>) -> Self {
        self.children.push(child.into());
        self
    }
    
    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = Some(index);
        self
    }
    
    pub fn on_click<F>(mut self, on_click: F) -> Self
    where
        F: Fn(&usize, &mut Window, &mut App) + 'static,
    {
        self.on_click = Some(Rc::new(on_click));
        self
    }
}

impl RenderOnce for TabBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let selected_index = self.selected_index;
        let on_click = self.on_click.clone();
        
        self.base
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .children(
                        self.children.into_iter().enumerate().map(|(ix, child)| {
                            child
                                .with_variant(self.variant) // Inherit variant
                                .with_size(self.size) // Inherit size
                                .when_some(selected_index, |this, selected_ix| {
                                    this.selected(selected_ix == ix)
                                })
                                .when_some(on_click.clone(), move |this, on_click| {
                                    this.on_click(move |_, window, cx| {
                                        on_click(&ix, window, cx)
                                    })
                                })
                        })
                    )
            )
    }
}
```

### Size System Pattern

Responsive sizing with custom values:

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Size {
    XSmall,
    Small,
    Medium,
    Large,
    Size(Pixels), // Custom pixel size
}

impl Size {
    pub fn button_height(&self) -> Pixels {
        match self {
            Size::Size(px) => *px,
            Size::XSmall => px(20.0),
            Size::Small => px(24.0),
            Size::Medium => px(32.0),
            Size::Large => px(40.0),
        }
    }
    
    pub fn icon_size(&self) -> Size {
        match self {
            Size::Size(px) => Size::Size(*px * 0.75),
            _ => *self,
        }
    }
}

// In component:
match self.size {
    Size::Size(v) => this.h(v).px(v * 0.2),
    Size::XSmall => this.h_5().px_1(),
    Size::Small => this.h_6().px_3(),
    Size::Medium => this.h_8().px_4(),
    Size::Large => this.h_10().px_6(),
}
```

### Focus Management Pattern

Complete focus handling with keyed state:

```rust
impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Use keyed state to persist focus handle
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        
        let is_focused = focus_handle.is_focused(window);
        
        div()
            .track_focus(&focus_handle.tab_index(self.tab_index))
            .when(is_focused, |this| {
                this.border_color(cx.theme().primary)
            })
            .on_key_down(|event, window, cx| {
                if event.keystroke.key == "Enter" {
                    // Handle enter
                }
            })
            .focus_ring(is_focused, px(2.), window, cx)
    }
}
```

## Summary

- Use modals for overlay content
- Implement dynamic lists with `.children()` and iterators
- Create forms with validation and error handling
- Use global state with `cx.set_global()` and `cx.global()`
- Communicate between components with events and subscriptions
- Manage tabs with state and conditional rendering
- Handle async loading with state machines
- Implement dropdowns with relative/absolute positioning
- **Use builder pattern with traits** for fluent APIs
- **Implement variant systems** with normal/hover/active/disabled states
- **Use SmallVec** for performance with dynamic children
- **Manage focus** with keyed state and track_focus
- **Support custom sizes** with Size enum

## References

- [GPUI Documentation](https://gpui.rs)
- [gpui-component Patterns](https://github.com/longbridge/gpui-component)
- [gpui-component Button](https://github.com/longbridge/gpui-component/blob/main/crates/ui/src/button/button.rs)
- [gpui-component TabBar](https://github.com/longbridge/gpui-component/blob/main/crates/ui/src/tab/tab_bar.rs)


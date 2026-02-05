---
name: gpui-code-quality
description: Best practices and code quality guidelines for GPUI development. Use when refactoring, reviewing code, or ensuring adherence to GPUI idioms.
---

# GPUI Code Quality

This skill covers best practices and code quality guidelines for GPUI development.

## Error Handling

### Never Use unwrap()

**Rule**: Avoid `unwrap()` and panic-inducing methods

```rust
// ❌ WRONG
let value = option.unwrap();
let result = operation().unwrap();

// ✅ CORRECT - propagate with ?
let value = option.ok_or_else(|| anyhow::anyhow!("Missing value"))?;
let result = operation()?;

// ✅ ALSO CORRECT - explicit handling
match option {
    Some(value) => {
        // Use value
    }
    None => {
        // Handle error case
    }
}
```

### Never Silently Discard Errors

**Rule**: Don't use `let _ =` on fallible operations

```rust
// ❌ WRONG - silently discards error
let _ = client.request(url).await?;

// ✅ CORRECT - propagate error
client.request(url).await?;

// ✅ ALSO CORRECT - log but ignore
operation().log_err();

// ✅ ALSO CORRECT - explicit handling
if let Err(e) = operation() {
    eprintln!("Operation failed: {}", e);
}
```

### Propagate Errors to UI

```rust
fn save_file(&mut self, cx: &mut Context<Self>) {
    cx.spawn(async move |this, cx| {
        match write_file(path, data).await {
            Ok(()) => {
                this.update(&mut *cx, |view, cx| {
                    view.status = "Saved successfully".into();
                    cx.notify();
                })?;
            }
            Err(e) => {
                this.update(&mut *cx, |view, cx| {
                    view.error = Some(format!("Save failed: {}", e));
                    cx.notify();
                })?;
            }
        }
        
        Ok(())
    }).detach_and_log_err(cx);
}
```

## Variable Naming

### Use Full Words

**Rule**: Avoid abbreviations, use descriptive names

```rust
// ❌ WRONG
let q = VecDeque::new();
let cnt = 0;
let btn = Button::new();

// ✅ CORRECT
let queue = VecDeque::new();
let count = 0;
let button = Button::new();
```

### Meaningful Names

```rust
// ❌ WRONG
let x = calculate();
let temp = process(data);
let thing = Entity::new();

// ✅ CORRECT
let result = calculate();
let processed_data = process(data);
let user_profile = Entity::new();
```

## Async Context Variable Shadowing

Use variable shadowing to scope clones in async contexts:

```rust
// ✅ CORRECT - clear scoping
fn start_work(&mut self, cx: &mut Context<Self>) {
    let data = self.data.clone();
    let config = self.config.clone();
    
    cx.spawn(async move |this, cx| {
        let result = process(data, config).await;
        
        this.update(&mut *cx, |view, cx| {
            view.result = result;
            cx.notify();
        })?;
        
        Ok(())
    }).detach();
}
```

## File Organization

### Avoid mod.rs Files

**Rule**: Use `src/module.rs` instead of `src/module/mod.rs`

```
// ❌ WRONG
src/
  components/
    mod.rs      # Don't do this
    button.rs

// ✅ CORRECT
src/
  components.rs  # Module file
  components/
    button.rs
```

### Library Root Paths

For crates, specify library root in `Cargo.toml`:

```toml
[lib]
path = "src/my_lib.rs"  # Instead of default lib.rs
```

## State Management

### Call cx.notify() After Changes

```rust
// ❌ WRONG
fn update_data(&mut self, data: String, cx: &mut Context<Self>) {
    self.data = data;
    // Missing cx.notify() - UI won't update!
}

// ✅ CORRECT
fn update_data(&mut self, data: String, cx: &mut Context<Self>) {
    self.data = data;
    cx.notify(); // Trigger re-render
}
```

### Use WeakEntity for Back-References

```rust
// ❌ WRONG - memory leak
struct Child {
    parent: Entity<Parent>, // Strong reference creates cycle!
}

// ✅ CORRECT
struct Child {
    parent: WeakEntity<Parent>, // Weak reference prevents leak
}
```

## Rendering

### Keep Render Pure

**Rule**: Don't mutate state in `render()` method

```rust
// ❌ WRONG
impl Render for MyView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.render_count += 1; // Don't mutate in render!
        div().child(format!("Renders: {}", self.render_count))
    }
}

// ✅ CORRECT - mutate in event handlers
impl Render for MyView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(format!("Count: {}", self.count))
            .child(
                div()
                    .child("Increment")
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.count += 1;
                        cx.notify();
                    }))
            )
    }
}
```

### Use SharedString for Text

```rust
// ❌ LESS EFFICIENT
struct MyView {
    title: String,
}

// ✅ MORE EFFICIENT
struct MyView {
    title: SharedString,
}
```

## Async Patterns

### Detach or Store Tasks

```rust
// ❌ WRONG - task dropped immediately
cx.spawn(async move |this, cx| {
    // Work...
    Ok(())
});

// ✅ CORRECT - detach
cx.spawn(async move |this, cx| {
    // Work...
    Ok(())
}).detach();

// ✅ ALSO CORRECT - store for cancellation
self.current_task = Some(cx.spawn(async move |this, cx| {
    // Work...
    Ok(())
}));
```

### Use background_spawn for CPU Work

```rust
// ❌ WRONG - blocks UI thread
cx.spawn(async move |this, cx| {
    let result = expensive_computation(); // Blocks UI!
    Ok(())
});

// ✅ CORRECT - run on background thread
cx.spawn(async move |this, cx| {
    let result = cx.background_spawn(async {
        expensive_computation()
    }).await;
    
    this.update(&mut *cx, |view, cx| {
        view.result = result;
        cx.notify();
    })?;
    
    Ok(())
});
```

## Comments

### Only Explain "Why"

**Rule**: Don't write comments that summarize code

```rust
// ❌ WRONG - obvious from code
// Increment the counter
self.counter += 1;

// Set the title to "Hello"
self.title = "Hello".into();

// ✅ CORRECT - explains non-obvious reasoning
// We must update the cache before notifying observers
// to ensure they see the most recent data
self.update_cache();
cx.notify();

// ✅ ALSO CORRECT - explains tricky logic
// Use saturating_sub to avoid underflow when count is 0
self.count = self.count.saturating_sub(1);
```

## Testing

### Use GPUI Timers in Tests

```rust
// ❌ WRONG
#[gpui::test]
async fn test_delay(cx: &mut TestAppContext) {
    smol::Timer::after(Duration::from_secs(1)).await;
}

// ✅ CORRECT
#[gpui::test]
async fn test_delay(cx: &mut TestAppContext) {
    cx.background_executor.timer(Duration::from_secs(1)).await;
    cx.background_executor.run_until_parked();
}
```

## Build Guidelines

### Use Project Scripts

```bash
# ❌ WRONG
cargo clippy

# ✅ CORRECT (for Zed-based projects)
./script/clippy

# Always use -q flag for cargo
cargo build -q
cargo test -q
```

## Code Organization

### Prefer Existing Files

**Rule**: Add functionality to existing files unless it's a new logical component

```rust
// Instead of creating many small files:
// src/button_click.rs
// src/button_hover.rs
// src/button_focus.rs

// Keep related functionality together:
// src/button.rs (with all button behavior)
```

### Module Organization

```rust
// Good module structure
src/
  ui/
    components.rs   // Component definitions
    theme.rs        // Theme and colors
    styles.rs       // Shared styles
  data/
    models.rs       // Data models
    state.rs        // Application state
```

## Summary of Best Practices

| Rule | Rationale |
|------|-----------|
| Never use `unwrap()` | Prevents panics, forces explicit error handling |
| Never `let _ =` on fallible ops | Errors should be handled or logged |
| Use full variable names | Improves readability and maintainability |
| Avoid mod.rs files | Cleaner project structure |
| Call `cx.notify()` after mutations | Ensures UI updates |
| Use `WeakEntity` for back-refs | Prevents memory leaks |
| Keep `render()` pure | Avoids race conditions |
| Use `SharedString` for text | Reduces allocations |
| Detach or store tasks | Prevents cancelled work |
| Use GPUI timers in tests | Prevents test failures |
| Only comment "why", not "what" | Reduces noise, focuses on reasoning |
| Propagate errors to UI | Provides user feedback |
| Use `background_spawn()` for CPU work | Keeps UI responsive |

## Checklist for Code Review

- [ ] No `unwrap()` calls
- [ ] No silently discarded errors (`let _ =` on fallible ops)
- [ ] `cx.notify()` called after state changes
- [ ] Tasks are detached or stored
- [ ] Weak references used for back-pointers
- [ ] `SharedString` used for text
- [ ] No mutations in `render()`
- [ ] Descriptive variable names
- [ ] Comments explain "why", not "what"
- [ ] GPUI timers used in tests
- [ ] Errors propagated to UI layer

## References

- [GPUI Documentation](https://gpui.rs)
- [Zed GEMINI.md](https://github.com/zed-industries/zed/blob/main/GEMINI.md)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

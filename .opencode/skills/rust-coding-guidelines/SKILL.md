---
name: rust-coding-guidelines
description: Rust and GPUI coding standards. MUST LOAD before writing any Rust code. Covers error handling, naming conventions, async patterns, file organization, and GPUI-specific rules. This skill is MANDATORY for all Rust work in this project.
---

# Rust Coding Guidelines

**IMPORTANT**: This skill contains mandatory coding rules for this project. Violations will cause code review failures.

## Core Principles

1. **Prioritize correctness and clarity** - Speed/efficiency are secondary unless specified
2. **Avoid creative additions** - Unless explicitly requested
3. **Prefer existing files** - Unless creating a new logical component
4. **Comments explain "why", not "what"** - No organizational/summary comments

## Error Handling (CRITICAL)

### NEVER use `unwrap()`

```rust
// WRONG
let value = option.unwrap();
let result = operation().unwrap();

// CORRECT - propagate with ?
let value = option.ok_or_else(|| anyhow::anyhow!("Missing value"))?;
let result = operation()?;

// CORRECT - explicit handling
match option {
    Some(value) => { /* use value */ }
    None => { /* handle missing */ }
}
```

### NEVER silently discard errors

```rust
// WRONG - silently discards error
let _ = client.request(url).await?;

// CORRECT - propagate
client.request(url).await?;

// CORRECT - log but continue
operation().log_err();

// CORRECT - explicit handling
if let Err(e) = operation() {
    eprintln!("Operation failed: {}", e);
}
```

### ALWAYS propagate errors to UI

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

### Indexing Safety

Be careful with indexing operations - they may panic if indexes are out of bounds.

```rust
// WRONG - may panic
let item = items[index];

// CORRECT - safe access
let item = items.get(index).ok_or_else(|| anyhow::anyhow!("Index out of bounds"))?;
```

## Variable Naming

### Use full words (NO abbreviations)

```rust
// WRONG
let q = VecDeque::new();
let cnt = 0;
let btn = Button::new();

// CORRECT
let queue = VecDeque::new();
let count = 0;
let button = Button::new();
```

## Async Patterns

### Variable Shadowing for Clones

Use variable shadowing to scope clones in async contexts:

```rust
// CORRECT pattern
executor.spawn({
    let task_ran = task_ran.clone();
    async move {
        *task_ran.borrow_mut() = true;
    }
});
```

## File Organization

### NEVER create `mod.rs` files

```
// WRONG
src/
  components/
    mod.rs       // Don't do this
    button.rs

// CORRECT
src/
  components.rs  // Module file
  components/
    button.rs
```

### Library Root Paths

For crates, specify library root in `Cargo.toml`:

```toml
[lib]
path = "src/my_lib.rs"  # Instead of default lib.rs
```

## GPUI-Specific Rules

### Context Types

- `App` - root context, access to global state
- `Context<T>` - provided when updating `Entity<T>`, dereferences to `App`
- `AsyncApp` / `AsyncWindowContext` - from `cx.spawn`, can be held across await points

### Window Parameter

`Window` comes **before** `cx` when present:

```rust
impl Render for MyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // window first, cx second
    }
}
```

### Entity Operations

```rust
// Read
let view = entity.read(cx);

// Update (use inner cx!)
entity.update(cx, |view, inner_cx| {
    view.count += 1;
    inner_cx.notify(); // Use inner_cx, NOT outer cx!
});

// NEVER update while already updating
entity.update(cx, |view, cx| {
    entity.update(cx, |_, _| {}); // PANIC!
});
```

### Testing Timers

In GPUI tests, use GPUI executor timers, NOT `smol::Timer`:

```rust
// WRONG - may cause "nothing left to run"
smol::Timer::after(duration).await;

// CORRECT
cx.background_executor().timer(duration).await;
// or
cx.background_executor.timer(duration).await; // in TestAppContext
```

### Event Handlers

```rust
// Using cx.listener for callbacks
.on_click(cx.listener(|this: &mut Self, event, window, cx| {
    // this: &mut Self
    // event: click event
    // window: &mut Window
    // cx: &mut Context<Self>
}))
```

### Notify After State Changes

```rust
fn update_state(&mut self, cx: &mut Context<Self>) {
    self.data = new_data;
    cx.notify(); // Required for UI update
}
```

### EventEmitter

```rust
// Declare event type
impl EventEmitter<MyEvent> for MyView {}

// Emit in handler
cx.emit(MyEvent::ValueChanged(value));
```

### Subscriptions

Store `Subscription` in struct fields - they auto-cancel when dropped:

```rust
struct Parent {
    child: Entity<Child>,
    _subscription: Subscription, // Keep alive
}
```

## Project-Specific Rules

### dbg!() and todo!() are DENIED

This project has Clippy rules that deny `dbg!()` and `todo!()` macros.

```rust
// WRONG - will fail clippy
dbg!(value);
todo!();

// Use proper logging or error handling instead
```

### Workspace Dependencies

All dependencies must be declared in workspace `Cargo.toml`, sub-crates use `.workspace = true`:

```toml
# In sub-crate Cargo.toml
[dependencies]
gpui.workspace = true
```

## Quick Reference Checklist

Before submitting code:

- [ ] No `unwrap()` calls
- [ ] No `let _ =` on fallible operations
- [ ] No `dbg!()` or `todo!()`
- [ ] Full variable names (no abbreviations)
- [ ] No `mod.rs` files
- [ ] `cx.notify()` after state changes
- [ ] Errors propagate to UI
- [ ] GPUI timers in tests (not smol)
- [ ] Subscriptions stored in struct fields
- [ ] Using inner `cx` in entity update closures

## References

- Project `.rule` file
- [Zed GEMINI.md](https://github.com/zed-industries/zed/blob/main/GEMINI.md)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

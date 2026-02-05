---
name: gpui-troubleshooting
description: Common errors and solutions for GPUI development. Use when debugging build errors, runtime panics, borrow checker issues, or unexpected behavior.
---

# GPUI Troubleshooting

This skill covers common errors and solutions when developing with GPUI.

## Borrow Checker Errors

### Multiple Borrow Error

**Error**: Cannot borrow `cx` as mutable more than once

**Solution**: Use the inner `cx` provided to closures, not the outer one

```rust
// ❌ WRONG
entity.update(cx, |view, inner_cx| {
    view.count += 1;
    cx.notify(); // Using outer cx - ERROR!
});

// ✅ CORRECT
entity.update(cx, |view, inner_cx| {
    view.count += 1;
    inner_cx.notify(); // Using inner cx
});
```

### Moved Value Error

**Error**: Use of moved value in async block

**Solution**: Clone before moving into async block

```rust
// ❌ WRONG
cx.spawn(async move |this, cx| {
    self.data.do_something(); // ERROR: self moved
});

// ✅ CORRECT
let data = self.data.clone();
cx.spawn(async move |this, cx| {
    data.do_something();
});
```

## Entity Errors

### Update While Updating Panic

**Error**: `already mutably borrowed: BorrowError`

**Cause**: Trying to update an entity while it's already being updated

**Solution**: Avoid nested entity updates

```rust
// ❌ WRONG - will panic
entity.update(cx, |view, cx| {
    entity.update(cx, |view2, cx2| {
        // Nested update - PANIC!
    });
});

// ✅ CORRECT - restructure logic
entity.update(cx, |view, cx| {
    view.prepare_update();
});
// Update happens after first update completes
entity.update(cx, |view, cx| {
    view.apply_update();
});
```

### WeakEntity Upgrade Failure

**Error**: Panic when calling methods on `None`

**Solution**: Always check `upgrade()` result

```rust
// ❌ WRONG
let entity = weak.upgrade().unwrap(); // May panic!

// ✅ CORRECT
if let Some(entity) = weak.upgrade() {
    entity.update(cx, |view, cx| {
        // ...
    });
} else {
    // Entity was dropped
}

// ✅ ALSO CORRECT (in async context)
this.update(&mut *cx, |view, cx| {
    // ...
})?; // Propagate error if entity gone
```

## Async Errors

### "Nothing Left to Run" in Tests

**Error**: `run_until_parked()` completes but test fails

**Cause**: Using `smol::Timer` instead of GPUI timers

**Solution**: Use GPUI executor timers

```rust
// ❌ WRONG
smol::Timer::after(Duration::from_secs(1)).await;

// ✅ CORRECT
cx.background_executor.timer(Duration::from_secs(1)).await;
```

### Task Dropped Before Completion

**Error**: Async work doesn't complete

**Cause**: Task dropped without being awaited or detached

**Solution**: Either detach or store the task

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

// ✅ ALSO CORRECT - store
self.current_task = Some(cx.spawn(async move |this, cx| {
    // Work...
    Ok(())
}));
```

### Async Context Borrow Error

**Error**: Cannot dereference `cx` in async context

**Solution**: Use `&mut *cx` to dereference

```rust
// ❌ WRONG
cx.spawn(async move |this, cx| {
    this.update(cx, |view, cx| { // ERROR!
        // ...
    });
});

// ✅ CORRECT
cx.spawn(async move |this, cx| {
    this.update(&mut *cx, |view, cx| {
        // ...
    })?;
    Ok(())
});
```

## Trait Errors

### IntoElement Not Implemented

**Error**: `the trait IntoElement is not implemented for ...`

**Solution**: Ensure type implements `IntoElement` or use `.child()` correctly

```rust
// ❌ WRONG
div().child(some_struct); // Error if SomeStruct doesn't impl IntoElement

// ✅ CORRECT - use entity
let entity = cx.new(|_| SomeStruct::new());
div().child(entity);

// ✅ ALSO CORRECT - implement RenderOnce
#[derive(IntoElement)]
struct SomeStruct;

impl RenderOnce for SomeStruct {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div().child("Content")
    }
}
```

### Render Trait Signature Mismatch

**Error**: Method signature doesn't match trait

**Solution**: Ensure correct signature with `Window` and `Context<Self>`

```rust
// ❌ WRONG
impl Render for MyView {
    fn render(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        // Missing window parameter
    }
}

// ✅ CORRECT
impl Render for MyView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Content")
    }
}
```

## UI Not Updating

### Forgetting cx.notify()

**Symptom**: State changes but UI doesn't update

**Solution**: Call `cx.notify()` after state changes

```rust
// ❌ WRONG
fn increment(&mut self, cx: &mut Context<Self>) {
    self.count += 1;
    // Missing cx.notify() - UI won't update!
}

// ✅ CORRECT
fn increment(&mut self, cx: &mut Context<Self>) {
    self.count += 1;
    cx.notify(); // Trigger re-render
}
```

### Subscription Dropped

**Symptom**: Events stop being received

**Solution**: Store `Subscription` in a field

```rust
// ❌ WRONG
impl Parent {
    fn new(cx: &mut Context<Self>) -> Self {
        let child = cx.new(|_| Child::new());
        
        cx.subscribe(&child, |this, _child, event, cx| {
            // Handle event
        }); // Subscription dropped here!
        
        Self { child }
    }
}

// ✅ CORRECT
struct Parent {
    child: Entity<Child>,
    _subscription: Subscription, // Store to keep alive
}

impl Parent {
    fn new(cx: &mut Context<Self>) -> Self {
        let child = cx.new(|_| Child::new());
        
        let subscription = cx.subscribe(&child, |this, _child, event, cx| {
            // Handle event
        });
        
        Self {
            child,
            _subscription: subscription,
        }
    }
}
```

## Build Errors

### Missing Imports

**Error**: Cannot find type/trait in this scope

**Solution**: Import from `gpui::*` or specific module

```rust
// Add at top of file
use gpui::*;

// Or specific imports
use gpui::{
    div, rgb, px,
    App, Context, Entity, Window,
    Render, IntoElement, RenderOnce,
};
```

### Clippy Warnings

Use project's clippy script:

```bash
# ❌ WRONG
cargo clippy

# ✅ CORRECT (for Zed-based projects)
./script/clippy
```

## Runtime Panics

### Index Out of Bounds

**Error**: Panic from vector indexing

**Solution**: Use safe access methods

```rust
// ❌ WRONG
let item = self.items[index]; // May panic!

// ✅ CORRECT
if let Some(item) = self.items.get(index) {
    // Use item
}

// ✅ ALSO CORRECT
if index < self.items.len() {
    let item = &self.items[index];
}
```

### Unwrap on None/Err

**Error**: `called unwrap() on a None value`

**Solution**: Never use `unwrap()` - use `?` or proper error handling

```rust
// ❌ WRONG
let value = option.unwrap(); // May panic!
let result = fallible_op().unwrap(); // May panic!

// ✅ CORRECT - propagate error
let value = option.ok_or_else(|| anyhow::anyhow!("Missing value"))?;
let result = fallible_op()?;

// ✅ ALSO CORRECT - handle explicitly
match option {
    Some(value) => {
        // Use value
    }
    None => {
        // Handle missing case
    }
}
```

## Common Mistakes Summary

| Issue | Cause | Solution |
|-------|-------|----------|
| Multiple borrow | Using outer `cx` in closure | Use inner `cx` |
| Update panic | Nested entity updates | Avoid nesting, restructure |
| UI not updating | Missing `cx.notify()` | Call after state changes |
| Task not running | Task dropped | Use `.detach()` or store task |
| Test timeout | Using `smol::Timer` | Use GPUI `timer()` |
| Events not received | Subscription dropped | Store in field |
| Weak entity panic | Not checking `upgrade()` | Use `if let Some(...)` or `?` |
| Unwrap panic | Calling `.unwrap()` | Use `?` or proper error handling |

## Debugging Tips

1. **Check the inner cx**: In update closures, always use the inner `cx`
2. **Verify notify calls**: Add `cx.notify()` after state changes
3. **Store subscriptions**: Keep `Subscription` values in struct fields
4. **Use GPUI timers in tests**: Replace `smol::Timer` with `cx.background_executor.timer()`
5. **Avoid unwrap**: Use `?` for error propagation or explicit error handling
6. **Check task lifecycle**: Ensure tasks are detached or stored, not dropped
7. **Watch for nested updates**: Avoid updating entities within update closures

## References

- [GPUI Documentation](https://gpui.rs)
- [Zed GEMINI.md](https://github.com/zed-industries/zed/blob/main/GEMINI.md)

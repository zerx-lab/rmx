---
name: gpui-styling
description: Styling GPUI elements with Tailwind-like utility methods. Use when applying styles, layouts, colors, spacing, or creating responsive designs.
---

# GPUI Styling

This skill covers styling GPUI elements using Tailwind-like utility methods.

## Overview

GPUI provides chainable methods similar to Tailwind CSS:
- **Sizing**: `.w()`, `.h()`, `.size_full()`
- **Flexbox**: `.flex()`, `.flex_col()`, `.items_center()`, `.justify_center()`
- **Spacing**: `.p()`, `.m()`, `.gap()`
- **Colors**: `.bg()`, `.text_color()`, `.border_color()`
- **Borders**: `.border()`, `.rounded()`

## Sizing

### Width and Height

```rust
use gpui::*;

div()
    .w(px(200.0))              // Fixed width
    .h(px(100.0))              // Fixed height
    .w_full()                   // Width: 100%
    .h_full()                   // Height: 100%
    .size_full()                // Width & Height: 100%
```

### Relative Units

```rust
div()
    .w(relative(0.5))           // 50% of parent
    .h(relative(1.0))           // 100% of parent
```

### Min/Max Sizes

```rust
div()
    .min_w(px(100.0))
    .max_w(px(500.0))
    .min_h(px(50.0))
    .max_h(px(300.0))
```

## Flexbox Layout

### Flex Direction

```rust
div()
    .flex()                     // Enable flexbox
    .flex_row()                 // Horizontal (default)
    .flex_col()                 // Vertical
    
// Short forms (from gpui-component)
div()
    .v_flex()                   // flex + flex_col
    .h_flex()                   // flex + flex_row
```

### Alignment

```rust
div()
    .flex()
    .items_start()              // Align items to start
    .items_center()             // Center items
    .items_end()                // Align items to end
    .items_stretch()            // Stretch items
    
    .justify_start()            // Justify to start
    .justify_center()           // Center justify
    .justify_end()              // Justify to end
    .justify_between()          // Space between
    .justify_around()           // Space around
```

### Flex Properties

```rust
div()
    .flex_1()                   // flex: 1 (grow to fill)
    .flex_grow()                // Allow growing
    .flex_shrink()              // Allow shrinking
    .flex_none()                // Don't grow or shrink
    .flex_wrap()                // Wrap items
```

## Spacing

### Padding

```rust
div()
    .p(px(16.0))                // All sides
    .p_1()                      // 4px (0.25rem)
    .p_2()                      // 8px (0.5rem)
    .p_4()                      // 16px (1rem)
    .p_6()                      // 24px (1.5rem)
    .p_8()                      // 32px (2rem)
    
    .px(px(16.0))               // Horizontal padding
    .py(px(16.0))               // Vertical padding
    .px_4()                     // Horizontal 16px
    .py_2()                     // Vertical 8px
    
    .pt(px(8.0))                // Top padding
    .pr(px(8.0))                // Right padding
    .pb(px(8.0))                // Bottom padding
    .pl(px(8.0))                // Left padding
```

### Margin

```rust
div()
    .m(px(16.0))                // All sides
    .m_4()                      // 16px
    
    .mx(px(16.0))               // Horizontal margin
    .my(px(16.0))               // Vertical margin
    .mx_4()                     // Horizontal 16px
    
    .mt(px(8.0))                // Top margin
    .mr(px(8.0))                // Right margin
    .mb(px(8.0))                // Bottom margin
    .ml(px(8.0))                // Left margin
```

### Gap

```rust
div()
    .flex()
    .flex_col()
    .gap(px(16.0))              // Gap between children
    .gap_1()                    // 4px
    .gap_2()                    // 8px
    .gap_4()                    // 16px
    .gap_6()                    // 24px
```

## Colors

### Background Colors

```rust
use gpui::*;

div()
    .bg(rgb(0x3b82f6))          // Hex color
    .bg(rgb(59, 130, 246))      // RGB
    .bg(rgba(59, 130, 246, 0.5)) // RGBA with alpha
    .bg(hsla(0.6, 0.8, 0.6, 1.0)) // HSLA
```

### Text Colors

```rust
div()
    .text_color(rgb(0xffffff))
    .text_color(rgba(255, 255, 255, 0.9))
```

### Border Colors

```rust
div()
    .border_color(rgb(0x4a4a4a))
```

## Borders

### Border Width

```rust
div()
    .border()                   // 1px all sides
    .border_1()                 // 1px
    .border_2()                 // 2px
    .border_t()                 // Top border
    .border_r()                 // Right border
    .border_b()                 // Bottom border
    .border_l()                 // Left border
```

### Border Radius

```rust
div()
    .rounded(px(4.0))           // Custom radius
    .rounded_sm()               // Small radius
    .rounded_md()               // Medium radius
    .rounded_lg()               // Large radius
    .rounded_full()             // Fully rounded (circle/pill)
```

## Typography

### Font Sizes

```rust
div()
    .text_xs()                  // Extra small
    .text_sm()                  // Small
    .text_base()                // Base size (default)
    .text_lg()                  // Large
    .text_xl()                  // Extra large
    .text_2xl()                 // 2x extra large
    .text_3xl()                 // 3x extra large
```

### Font Weight

```rust
div()
    .font_normal()              // Normal weight
    .font_medium()              // Medium weight
    .font_semibold()            // Semibold
    .font_bold()                // Bold
```

### Text Alignment

```rust
div()
    .text_left()
    .text_center()
    .text_right()
```

## Cursor

```rust
div()
    .cursor_pointer()           // Pointer cursor on hover
    .cursor_default()           // Default cursor
    .cursor_text()              // Text cursor
```

## Hover States

```rust
div()
    .bg(rgb(0x3b82f6))
    .hover(|style| {
        style.bg(rgb(0x2563eb))
    })
```

## Visibility

```rust
div()
    .visible()
    .invisible()
```

## Common Patterns

### Card Component

```rust
div()
    .p_6()
    .bg(rgb(0x1a1a1a))
    .rounded_lg()
    .border_1()
    .border_color(rgb(0x2a2a2a))
    .flex()
    .flex_col()
    .gap_4()
```

### Button

```rust
div()
    .px_6()
    .py_3()
    .bg(rgb(0x3b82f6))
    .text_color(rgb(0xffffff))
    .rounded_md()
    .cursor_pointer()
    .font_semibold()
    .hover(|style| {
        style.bg(rgb(0x2563eb))
    })
```

### Input Field

```rust
div()
    .w_full()
    .px_3()
    .py_2()
    .bg(rgb(0x1a1a1a))
    .border_1()
    .border_color(rgb(0x4a4a4a))
    .rounded_md()
    .text_base()
```

### Navbar

```rust
div()
    .w_full()
    .h(px(64.0))
    .px_6()
    .bg(rgb(0x1a1a1a))
    .flex()
    .flex_row()
    .items_center()
    .justify_between()
    .border_b_1()
    .border_color(rgb(0x2a2a2a))
```

### Centered Container

```rust
div()
    .flex()
    .items_center()
    .justify_center()
    .size_full()
```

### Grid Layout

```rust
div()
    .flex()
    .flex_wrap()
    .gap_4()
    .children((0..12).map(|i| {
        div()
            .w(px(200.0))
            .h(px(150.0))
            .bg(rgb(0x3b82f6))
            .rounded_md()
    }))
```

## Color Utilities

### Common Colors

```rust
// Grays
rgb(0x1a1a1a)  // Dark gray
rgb(0x2a2a2a)  // Medium dark gray
rgb(0x4a4a4a)  // Medium gray
rgb(0x9ca3af)  // Light gray
rgb(0xf3f4f6)  // Very light gray

// Blues
rgb(0x3b82f6)  // Primary blue
rgb(0x2563eb)  // Darker blue
rgb(0x60a5fa)  // Lighter blue

// Greens
rgb(0x22c55e)  // Success green
rgb(0x16a34a)  // Darker green

// Reds
rgb(0xef4444)  // Error red
rgb(0xdc2626)  // Darker red

// Yellows
rgb(0xf59e0b)  // Warning yellow
```

### HSLA Colors

```rust
// More flexible color manipulation
hsla(0.0, 0.0, 0.1, 1.0)    // Dark gray
hsla(0.6, 0.8, 0.5, 1.0)    // Blue
hsla(0.33, 0.7, 0.5, 1.0)   // Green
hsla(0.0, 0.8, 0.6, 1.0)    // Red
hsla(0.15, 0.9, 0.5, 1.0)   // Yellow/Orange
```

## Theme System

### Constant Colors

```rust
// In theme.rs
pub const BG_PRIMARY: Hsla = hsla(0.0, 0.0, 0.05, 1.0);
pub const BG_SECONDARY: Hsla = hsla(0.0, 0.0, 0.1, 1.0);
pub const TEXT_PRIMARY: Hsla = hsla(0.0, 0.0, 0.9, 1.0);
pub const TEXT_SECONDARY: Hsla = hsla(0.0, 0.0, 0.6, 1.0);
pub const ACCENT: Hsla = hsla(0.6, 0.8, 0.5, 1.0);

// Usage
div()
    .bg(theme::BG_PRIMARY)
    .text_color(theme::TEXT_PRIMARY)
```

## Best Practices

1. **Use spacing scale**: Stick to `.p_1()`, `.p_2()`, `.p_4()`, etc. for consistency
2. **Define color constants**: Create a theme module with color constants
3. **Use HSLA for theming**: Easier to adjust lightness/saturation
4. **Combine utilities**: Chain methods for concise styling
5. **Extract common styles**: Create reusable components for repeated patterns

## Summary

- Use `.size_full()` for full width/height
- Use `.v_flex()` for vertical layouts, `.h_flex()` for horizontal
- Use spacing utilities `.p_4()`, `.gap_2()` for consistent spacing
- Use `.bg()`, `.text_color()` for colors
- Chain methods for concise styling
- Use `.hover()` for interactive states

## References

- [GPUI Documentation](https://gpui.rs)
- [Tailwind CSS](https://tailwindcss.com) - Similar utility concepts

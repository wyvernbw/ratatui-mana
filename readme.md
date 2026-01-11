# mana-tui

never outta magic.

ECS based library for builiding performant TUI libraries when you need more than just a few widgets.

## architecture:

- [ ] `mana-tui`
- [x] (optional) `mana-tui-macros`
  - manasx dsl
- [ ] `mana-tui-beheaded`
  - headless ui components
- [x] [`mana-tui-elemental`][]
  - provides the basic element abstraction and layout engine
- built on top of [ratatui](https://ratatui.rs/) & [hecs](https://docs.rs/hecs/latest/hecs/)

[`mana-tui-elemental`]: ./mana-tui-elemental/
[`mana-tui-macros`]: ./mana-tui-macros/

## Features

### Layout Engine

```plaintext
╭parent──────────╮
│╭──╮  ╭──╮  ╭──╮│
││01│  │02│  │03││
│╰──╯  ╰──╯  ╰──╯│
╰────────────────╯
```

Mana-tui has a full flexbox style layout engine in [`mana-tui-elemental`], the
foundation of the library, that is more powerful than ratatui's built in layout
options and easier to work with. In terms of interacting with it, the biggest
difference is that elemental takes the full description of the layout upfront
instead of imperatively splitting the layout as you render. This allows for multiple
layout passes and out of order sizing (meaning that parent containers can defer calculating
their size until after their children) allowing for fit and grow style sizing.

Here's how a layout definition might look with elemental:

```rust
# use ratatui::{buffer::Buffer, layout::Rect};
use mana_tui_elemental::prelude::*;

// first, create a context
// the context efficiently stores widgets and their components (no Box<dyn _> in sight)
let mut ctx = ElementCtx::new();
let block = || Block::bordered().border_type(BorderType::Rounded);

let root = ui(block().title_top("parent"))
    //        ^^^^^^ `ui` takes a widget as an argument
    // provide a list of components in the `with` function
    .with((
        Width::fixed(36),
        Height::fixed(18),
        Direction::Horizontal,
        Padding::uniform(1),
    ))
    // children can take a tuple, vec, array and other types
    .children((
        ui(block().title_top("sidebar"))
            .with((Width::fixed(10), Height::grow(), Padding::uniform(1)))
            .child(ui(Paragraph::new(
                "this sidebar is so amazing it can have long text that wraps around",
            )
            .wrap(ratatui::widgets::Wrap { trim: false }))),
        ui(block().title_top("child #1"))
            .with((
                Width::grow(),
                Height::grow(),
                Padding::uniform(1),
                Gap(1),
                Direction::Vertical,
            ))
            .children((
                ui(block().title_top("child #2")).with((Width::grow(), Height::grow())),
                ui(block().title_top("child #3")).with((Width::grow(), Height::grow())),
            )),
    )); // returned a `UiBuilder`
// then, `spawn_ui` creates all entities and their relationships and does some post processing
// to ensure sane defaults
//
// in mana-tui, every element or node is an entity in the ECS.
let root = ctx.spawn_ui(root);
// calculate the layout
let mut buf = Buffer::empty(Rect::new(0, 0, 50, 24));
ctx.calculate_layout(root, buf.area).unwrap();
// finally render to the buffer
ctx.render(root, buf.area, &mut buf);
```

Note that you can call `render` as many times as you want. After the UI is spawned, it is not necessary to
respawn it or even to recalculate its layout before rerendering.

### Manasx

This is mana's DSL for defining element trees (called `View`s in mana). This
feature requires the `macros` feature and is not enabled by default.

This is how our previous example would look like with manasx:

```rust
# use ratatui::{buffer::Buffer, layout::Rect};
use mana_tui_elemental::prelude::*;
use mana_tui_macros::{subview, ui};  // the macros

let mut ctx = ElementCtx::new();

#[subview]
fn sidebar() -> View {
    let value = "i am formatted";
    ui! {
        <Block .rounded .title_top="sidebar" Width::fixed(10) Height::grow()>
            <Paragraph .wrap={Wrap::default()}>
                "this sidebar is so amazing it can have long text that wraps around
                and formatting arguments {value}"
            </Paragraph>
        </Block>
    }
}

let root = ui! {
    <Block
        .rounded .title_top="parent"
        Width::fixed(36) Height::fixed(18) Direction::Horizontal Padding::uniform(1)
    >
        <Sidebar />
        <Block .rounded .title_top="child #1"
            Width::grow() Height::grow() Padding::uniform(1) Gap(1) Direction::Vertical
        >
            <Block .rounded .title_top="child #2" Width::grow() Height::grow()/>
            <Block .rounded .title_top="child #3" Width::grow() Height::grow()/>
        </Block>
    </Block>
};
let root = ctx.spawn_ui(root);
let mut buf = Buffer::empty(Rect::new(0, 0, 36, 18));
ctx.calculate_layout(root, buf.area).unwrap();
ctx.render(root, buf.area, &mut buf);


let expected = Buffer::with_lines(vec![
    "╭parent────────────────────────────╮",
    "│╭sidebar─╮╭child #1──────────────╮│",
    "││        ││╭child #2────────────╮││",
    "││        │││                    │││",
    "││        │││                    │││",
    "││        │││                    │││",
    "││        │││                    │││",
    "││        │││                    │││",
    "││        ││╰────────────────────╯││",
    "││        ││                      ││",
    "││        ││╭child #3────────────╮││",
    "││        │││                    │││",
    "││        │││                    │││",
    "││        │││                    │││",
    "││        │││                    │││",
    "││        ││╰────────────────────╯││",
    "│╰────────╯╰──────────────────────╯│",
    "╰──────────────────────────────────╯",
]);
assert_eq!(buf, expected);
```

This is not a one to one translation but you get the idea. the `ui!` macro just
generates builder syntax code. `sideview` attribute makes any function (supports
generics and impl trait arguments) that returns View into a subview that can be
used as a tag inside the `ui!` macro. This might be familiar to you if you know
react, but manasx subviews cannot hold state (this is why i named them subviews
and not components) and that is on purpose. I think managing your app state
through the ECS by writing systems is cleaner and more performant.

Arbitrary expression blocks and iterators are also supported:

```rust ignore
#[subview]
fn root() -> View {
    ui! {
        <Block>
        {
          (0..3).map(|idx| ui!{
              <NumberedBox .idx={idx} />
          })
        }
        </Block>
    }
}
```

Note that in both cases the api is still being ironed out so it still has a few quirks.

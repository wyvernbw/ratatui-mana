pub mod handlers;

use std::any::TypeId;

use hecs::{Entity, Or, World};
use im::Vector;
use mana_tui_elemental::layout::{Children, Props};
use mana_tui_utils::resource::Resources;
use ratatui::{layout::Rect, style::Style};

use crate::{
    DefaultEvent, Effect, Message,
    backends::{DefaultBackend, DefaultKeyEvent, ManaBackend},
    focus::handlers::{On, OnClick, OnKey},
};

#[derive(Debug, Clone, Copy)]
pub enum FocusPolicy {
    Popup,
    Pass,
    Block,
}

#[derive(Debug, Clone, Default)]
pub enum Navigation {
    Cycle(DefaultEvent),
    #[default]
    Directional,
}

#[derive(Debug, Clone, Default)]
pub struct NavGroup {
    nav: Navigation,
    elements: Vector<Entity>,
}

#[derive(Debug, Clone, Default)]
pub struct UiStack {
    stack: Vector<NavGroup>,
}

pub(crate) fn generate_ui_stack(world: &mut World, root: Entity) {
    let mut stack = Vector::new();
    let last_group = generate_ui_stack_impl(world, root, &mut stack, NavGroup::default());
    if !last_group.elements.is_empty() {
        stack.push_back(last_group);
    }
    world.insert_or_update_resource(UiStack { stack });
}

#[tracing::instrument(skip(world))]
pub(crate) fn generate_ui_stack_impl(
    world: &World,
    root: Entity,
    stack: &mut Vector<NavGroup>,
    mut current_group: NavGroup,
) -> NavGroup {
    current_group.elements.push_back(root);
    let mut query = world.query_one::<&Navigation>(root);

    if query.get().is_ok() {
        if !current_group.elements.is_empty() {
            stack.push_back(current_group.clone());
        }
        current_group = NavGroup::default();
    }

    let children = world.get::<&Children>(root);
    if let Ok(children) = children {
        for child in children.iter() {
            current_group = generate_ui_stack_impl(world, *child, stack, current_group.clone());
        }
    } else {
        stack.push_back(current_group.clone());
    }

    current_group
}

#[must_use]
#[derive(Debug, Clone, Copy)]
pub struct FocusTarget(TypeId);

#[must_use]
#[derive(Debug, Clone, Copy)]
pub struct FocusPopup;

impl FocusTarget {
    pub fn new<T: 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

pub(crate) fn init_focus_system(world: &mut World) {
    let _ = world.get_or_insert_resource_with::<&FocusContext>(|world| {
        let ui_stack = world.get_resource::<&UiStack>();
        let first_focus = ui_stack
            .ok()
            .and_then(|stack| stack.stack.iter().next().cloned())
            .and_then(|nav_group| nav_group.elements.iter().next().copied());
        let mut ctx = FocusContext { stack: Vec::new() };
        if let Some(entity) = first_focus {
            if let Ok(target) = world.get::<&FocusTarget>(entity) {
                ctx.push(target.0);
            }
        }
        ctx
    });
}

macro_rules! try_handler {
    ($world:ident, $entity:ident, $on:ident, $model:ident, $msg:ident) => {
        let value = $on($model, $msg);
        if let Some(value) = value {
            _ = try_grab_focus($world, $entity);
            return Ok(Some(value));
        }
    };
    ($world:ident, $entity:ident, Key($key:ident), $on:ident, $model:ident, $msg:ident) => {
        if let Some(key_event) = DefaultBackend::<std::io::Stdout>::event_as_key($msg.clone())
            && &key_event == $key
        {
            let value = $on($model, $msg);
            if let Some(value) = value {
                _ = try_grab_focus($world, $entity);
                return Ok(Some(value));
            }
        }
    };
}

pub(crate) fn propagate_key_event<Msg: Message>(
    world: &World,
    model: &Msg::Model,
    msg: &DefaultEvent,
) -> Result<Option<(Msg, Effect<Msg>)>, anyhow::Error> {
    let stack = world.get_resource::<&UiStack>()?;
    let mut query = world.query::<Or<&On<Msg>, &OnKey<Msg>>>();
    let query = query.view();
    for group in &stack.stack {
        for entity in group.elements.iter().copied() {
            if let Some(value) = query.get(entity) {
                match value {
                    Or::Left(On(on)) => {
                        try_handler!(world, entity, on, model, msg);
                    }
                    Or::Right(OnKey(key, cb)) => {
                        try_handler!(world, entity, Key(key), cb, model, msg);
                    }
                    Or::Both(On(on), OnKey(key, on_key)) => {
                        try_handler!(world, entity, Key(key), on_key, model, msg);
                        try_handler!(world, entity, on, model, msg);
                    }
                }
            }
        }
    }
    Ok(None)
}

pub(crate) fn propagate_mouse_event<Msg: Message>(
    world: &World,
    model: &Msg::Model,
    msg: &DefaultEvent,
    x_coord: u16,
    y_coord: u16,
) -> Result<Option<(Msg, Effect<Msg>)>, anyhow::Error> {
    #[cfg(feature = "crossterm")]
    {
        use crossterm::event::{Event, MouseEvent, MouseEventKind};
        if !matches!(
            msg,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(_),
                ..
            })
        ) {
            return Ok(None);
        }
    }
    let stack = world.get_resource::<&UiStack>()?;
    let mut query = world.query::<(&OnClick<Msg>, &Props)>();
    let query = query.view();
    for group in &stack.stack {
        for entity in group.elements.iter().copied() {
            if let Some((OnClick(on_click), props)) = query.get(entity) {
                let area = Rect {
                    x: props.position.x,
                    y: props.position.y,
                    width: props.size.x,
                    height: props.size.y,
                };
                if area.contains(ratatui::layout::Position {
                    x: x_coord,
                    y: y_coord,
                }) {
                    try_handler!(world, entity, on_click, model, msg);
                }
            }
        }
    }
    Ok(None)
}

pub(crate) fn propagate_event<Msg: Message>(
    world: &World,
    model: &Msg::Model,
    msg: &DefaultEvent,
) -> Result<Option<(Msg, Effect<Msg>)>, anyhow::Error> {
    #[cfg(feature = "crossterm")]
    {
        match msg {
            crossterm::event::Event::Key(_) => propagate_key_event(world, model, msg),
            crossterm::event::Event::Mouse(ev) => {
                propagate_mouse_event(world, model, msg, ev.column, ev.row)
            }
            _ => Ok(None),
        }
    }
}

pub(crate) fn try_grab_focus(world: &World, entity: Entity) -> anyhow::Result<()> {
    let mut query = world.query_one::<(&FocusTarget, Option<&FocusPopup>)>(entity);
    let (&focus_target, popup) = query.get()?;
    let popup = popup.is_some();

    let mut focus_ctx = world.get_resource::<&mut FocusContext>()?;
    if popup {
        if focus_ctx.top() != Some(focus_target.0) {
            focus_ctx.push(focus_target.0);
        }
    } else {
        focus_ctx.focus_on_value(focus_target.0);
    }

    Ok(())
}

pub(crate) struct FocusContext {
    stack: Vec<TypeId>,
}

impl FocusContext {
    fn top(&self) -> Option<TypeId> {
        self.stack.last().copied()
    }
    fn push(&mut self, value: TypeId) {
        self.stack.push(value);
    }

    fn pop(&mut self) -> Option<TypeId> {
        self.stack.pop()
    }

    fn focus_on<T: 'static>(&mut self) {
        self.pop();
        self.push(TypeId::of::<T>());
    }

    fn focus_on_value(&mut self, value: TypeId) {
        self.pop();
        self.push(value);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FocusStyle(pub Style);

pub(crate) fn set_focus_style(world: &mut World) -> anyhow::Result<()> {
    let focus_ctx = world.get_resource::<&FocusContext>()?;
    let current = focus_ctx.top();
    drop(focus_ctx);

    let focused_on = world
        .query_mut::<(Entity, &Props, &FocusTarget, &FocusStyle)>()
        .into_iter()
        .find_map(|(entity, props, focus_target, focus_style)| {
            if Some(focus_target.0) == current {
                Some((entity, props, focus_style))
            } else {
                None
            }
        });
    if let Some((entity, &props, &style)) = focused_on {
        (props.set_style)(world, entity, style.0);
    }

    Ok(())
}

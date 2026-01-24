use std::any::TypeId;
use std::sync::Arc;
use std::time::Duration;

use hecs::{CommandBuffer, Component, Or};
use hecs::{Entity, World};
use mana_tui_elemental::layout::Children;
use mana_tui_elemental::layout::Props;
use mana_tui_utils::Ecs;
use mana_tui_utils::resource::Resources;
use mana_tui_utils::systems::SystemsExt;
use ratatui::crossterm::event::{
    KeyCode, KeyEventKind, KeyEventState, KeyModifiers, MouseEventKind,
};
use ratatui::crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::Position;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use smallvec::SmallVec;

use crate::schedule::{PostRenderSchedule, PreRenderSchedule};
use crate::{EventQueue, Marker, Store};

#[derive(Debug, Clone, Copy)]
pub enum FocusPolicy {
    Block,
    Pass,
}

pub struct Focused;
pub struct Hovered;
pub struct Clicked;

pub struct UiStack {
    stack: Arc<[Entity]>,
}

pub(crate) fn generate_ui_stack(world: &mut World, root: Entity) {
    let mut stack = vec![];
    generate_ui_stack_impl(world, root, &mut stack);
    world.insert_or_update_resource(UiStack {
        stack: stack.into(),
    });
}

pub(crate) fn generate_ui_stack_impl(world: &World, root: Entity, stack: &mut Vec<Entity>) {
    stack.push(root);
    let children = world.get::<&Children>(root);
    if let Ok(children) = children {
        for child in children.iter() {
            generate_ui_stack_impl(world, *child, stack);
        }
    }
}

pub(crate) fn clear_old_hovers(world: &mut World) {
    let mut cmd = CommandBuffer::new();
    for (entity, _) in world.query_mut::<(Entity, &Hovered)>() {
        cmd.remove_one::<Hovered>(entity);
    }
    cmd.run_on(world);
}

pub(crate) fn handle_mouse_event(
    world: &mut World,
    event: MouseEvent,
) -> Result<bool, hecs::ComponentError> {
    let queue = world.get_resource::<&EventQueue>().unwrap().0.clone();
    tracing::info!(?event);
    let uistack = {
        let Ok(uistack) = world.get_resource::<&UiStack>() else {
            return Ok(false);
        };

        uistack.stack.clone()
    };

    let mut consumed = false;

    for entity in uistack.iter().rev().copied() {
        let mut query_one = world.query_one::<(&Props, Option<&FocusPolicy>, &Marker)>(entity);
        let Ok((props, focus_policy, &marker)) = query_one.get() else {
            continue;
        };
        let props = *props;
        let focus_policy = *focus_policy.unwrap_or(&FocusPolicy::Pass);
        let area = Rect {
            x: props.position.x,
            y: props.position.y,
            width: props.size.x,
            height: props.size.y,
        };
        drop(query_one);

        if !area.contains(Position {
            x: event.column,
            y: event.row,
        }) {
            continue;
        }

        match event.kind {
            MouseEventKind::Down(_) => match focus_policy {
                FocusPolicy::Block => {
                    world.insert_one(entity, Clicked)?;
                    consumed = true;
                    tokio::task::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        _ = queue
                            .0
                            .send_async(crate::UiEvent::ClickedStyleFinished(marker.0))
                            .await;
                    });
                    break;
                }
                FocusPolicy::Pass => {}
            },
            MouseEventKind::Moved => match focus_policy {
                FocusPolicy::Block => {
                    consumed = true;
                    world.insert_one(entity, Hovered)?;
                    break;
                }
                FocusPolicy::Pass => {}
            },
            _ => {}
        }
    }

    Ok(consumed)
}

pub(crate) fn click_post_update_system(world: &mut World) {
    let mut cmd = CommandBuffer::new();
    for (clicked, _) in world.query_mut::<(Entity, &Clicked)>() {
        cmd.remove_one::<Clicked>(clicked);
        cmd.insert_one(clicked, Focused);
    }
    cmd.run_on(world);
}

pub(crate) fn press_post_update_system(world: &mut World) {
    let mut cmd = CommandBuffer::new();
    for (clicked, _) in world.query_mut::<(Entity, &Pressed)>() {
        cmd.remove_one::<Pressed>(clicked);
        cmd.insert_one(clicked, Focused);
    }
    cmd.run_on(world);
}

#[derive(derive_more::Deref, derive_more::DerefMut, Clone)]
pub struct OnClick(Arc<dyn Fn(&mut World) + Send + Sync + 'static>);

impl OnClick {
    pub fn new(func: impl Fn(&mut World) + Send + Sync + 'static) -> Self {
        Self(Arc::new(func) as Arc<_>)
    }
}

#[derive(derive_more::Deref, derive_more::DerefMut, Clone)]
pub struct OnPress(Arc<dyn Fn(&mut World) + Send + Sync + 'static>);

impl OnPress {
    pub fn new(func: impl Fn(&mut World) + Send + Sync + 'static) -> Self {
        Self(Arc::new(func) as Arc<_>)
    }
}

pub(crate) fn on_click_system(world: &mut World) {
    let handlers = world
        .query_mut::<(&OnClick, &Clicked)>()
        .into_iter()
        .map(|(handler, _)| handler.0.clone())
        .collect::<Vec<_>>();
    for handler in handlers {
        handler(world);
    }
}

pub struct Keybind(KeyEvent);

impl Keybind {
    #[must_use]
    pub fn new(code: KeyCode) -> Self {
        Self(KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        })
    }

    #[must_use]
    pub fn event_eq(&self, event: KeyEvent, kind: KeyEventKind) -> bool {
        self.0.code == event.code && self.0.modifiers == event.modifiers && event.kind == kind
    }

    #[must_use]
    pub fn char(ch: char) -> Self {
        Self::new(KeyCode::Char(ch))
    }

    #[must_use]
    pub fn modifiers(self, modifiers: KeyModifiers) -> Self {
        Self(KeyEvent {
            modifiers,
            ..self.0
        })
    }

    #[must_use]
    pub fn kind(self, kind: KeyEventKind) -> Self {
        Self(KeyEvent { kind, ..self.0 })
    }
}

pub struct Pressed;

pub(crate) fn keybind_clicked_system(world: &mut World, event: KeyEvent) -> bool {
    let queue = world.get_resource::<&EventQueue>().unwrap().0.clone();
    let mut cmd = CommandBuffer::new();
    let mut consumed = false;
    for (entity, keybind, &marker) in world.query_mut::<(Entity, &Keybind, &Marker)>() {
        if keybind.event_eq(event, KeyEventKind::Press) {
            consumed = true;
            cmd.insert_one(entity, Pressed);
            let tx = queue.0.clone();
            tokio::task::spawn(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                _ = tx
                    .send_async(crate::UiEvent::ClickedStyleFinished(marker.0))
                    .await;
            });
        } else if keybind.event_eq(event, KeyEventKind::Release) {
            cmd.remove_one::<Pressed>(entity);
        }
    }
    cmd.run_on(world);
    consumed
}

pub(crate) fn handle_pressed(world: &mut World) {
    let handlers: SmallVec<[_; 10]> = world
        .query_mut::<(Entity, &Pressed, Or<&OnClick, &OnPress>)>()
        .into_iter()
        .map(|(_, _, handler)| handler.cloned())
        .collect();
    for handler in handlers {
        match handler {
            Or::Left(onclick) => onclick(world),
            Or::Right(onpress) => onpress(world),
            Or::Both(onclick, onpress) => {
                onclick(world);
                onpress(world);
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FocusState {
    pub(crate) normal_style: Option<Style>,
    pub(crate) current_style: Option<Style>,
    pub(crate) entity: Option<Entity>,
}

pub trait Transient: Default + Clone + Component {
    fn hook_transient<T: Component, W: Ecs + SystemsExt + ?Sized>(ecs: &mut W) {
        let is_new = ecs.get_resource::<&Store<Self>>().is_ok();
        let mut store = ecs.get_or_insert_resource_with::<&mut Store<Self>>(Store::new);
        let original = store.get(&TypeId::of::<T>()).cloned();

        if original.is_none() {
            store.insert(TypeId::of::<T>(), Self::default());
        }

        drop(store);

        if is_new {
            ecs.add_system::<PostRenderSchedule>(|world| {
                Self::update::<T>(world);
            });
            ecs.add_system::<PreRenderSchedule>(|world| {
                Self::restore::<T>(world);
            });
        }
    }

    fn restore<T: Component>(world: &mut World);

    fn update<T: Component>(world: &mut World);
}

impl Transient for FocusState {
    fn restore<T: Component>(world: &mut World) {
        let Some((entity, _, &props)) =
            world.query_mut::<(Entity, &T, &Props)>().into_iter().next()
        else {
            return;
        };
        _ = world.insert_one(entity, Marker(TypeId::of::<T>()));
        let mut store = world.get_resource::<&mut Store<FocusState>>().unwrap();
        if let Some(focus) = store.get_mut(&TypeId::of::<T>()) {
            focus.entity = Some(entity);
            let Some(style) = focus.current_style else {
                return;
            };
            drop(store);
            (props.set_style)(world, entity, style);
        }
    }

    fn update<T: Component>(world: &mut World) {
        let Some((entity, &props, _, _)) = world
            .query_mut::<(Entity, &Props, Or<&Clicked, &Pressed>, &T)>()
            .into_iter()
            .next()
        else {
            return;
        };
        _ = world.insert_one(entity, Marker(TypeId::of::<T>()));
        let mut store = world.get_resource::<&mut Store<FocusState>>().unwrap();
        if let Some(focus) = store.get_mut(&TypeId::of::<T>()) {
            let style = (props.get_style)(world, entity);
            let style = style.unwrap_or_default();
            if focus.normal_style.is_none() {
                focus.normal_style = Some(style);
            }
            focus.current_style = Some(style.add_modifier(Modifier::REVERSED));
            focus.entity = Some(entity);
        }
    }
}

pub trait FocusExt: Ecs + SystemsExt {
    fn use_focus<T: Component>(&mut self) {
        FocusState::hook_transient::<T, Self>(self);
    }
}

impl<T: Ecs> FocusExt for T {}

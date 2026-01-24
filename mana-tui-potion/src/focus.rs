use std::sync::Arc;

use hecs::{Component, Entity, World};
use mana_tui_elemental::layout::Children;
use mana_tui_utils::resource::Resources;
use smallbox::SmallBox;

use crate::{DefaultBackend, DefaultEvent, Effect, EventStream, ManaBackend, PinnedFuture};

#[derive(Debug, Clone, Copy)]
pub enum FocusPolicy {
    Block,
    Pass,
}

pub struct Focused;
pub struct Hovered;
pub struct Clicked;
type CallbackRes<Msg> = Option<(Msg, Effect<Msg>)>;

pub struct On<Msg, Model>(Arc<dyn Fn(&Model, &DefaultEvent) -> CallbackRes<Msg>>);

impl<Msg, Model> On<Msg, Model> {
    pub fn new<F>(func: F) -> Self
    where
        F: Fn(&Model, &DefaultEvent) -> CallbackRes<Msg> + 'static,
    {
        On(Arc::new(func))
    }
}

unsafe impl<Msg, Model> Send for On<Msg, Model> {}
unsafe impl<Msg, Model> Sync for On<Msg, Model> {}

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

pub(crate) fn propagate_event<Msg: 'static, Model: 'static>(
    world: &World,
    model: &Model,
    msg: &DefaultEvent,
) -> Result<Option<(Msg, Effect<Msg>)>, hecs::ComponentError> {
    let stack = world.get_resource::<&UiStack>()?;
    let mut query = world.query::<&On<Msg, Model>>();
    let query = query.view();
    for entity in stack.stack.iter().copied() {
        if let Some(value) = query.get(entity) {
            let value = (value.0)(model, msg);
            if let Some(value) = value {
                return Ok(Some(value));
            }
        }
    }
    Ok(None)
}

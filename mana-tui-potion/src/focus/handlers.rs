use std::sync::Arc;

use hecs::{CommandBuffer, Entity, World};

use crate::{
    Effect, Message,
    backends::{DefaultEvent, DefaultKeyEvent},
};

type CallbackRes<Msg> = Option<(Msg, Effect<Msg>)>;
type Callback<Msg, Model> = Arc<dyn Fn(&Model, &DefaultEvent) -> CallbackRes<Msg>>;

#[must_use]
#[derive(Clone)]
pub struct On<Msg: Message>(pub(crate) Callback<Msg, Msg::Model>);

impl<Msg: Message> On<Msg> {
    pub fn new<F>(func: F) -> Self
    where
        F: Fn(&Msg::Model, &DefaultEvent) -> CallbackRes<Msg> + 'static,
    {
        On(Arc::new(func))
    }
}

unsafe impl<Msg: Message> Send for On<Msg> {}
unsafe impl<Msg: Message> Sync for On<Msg> {}

#[must_use]
#[derive(Clone)]
pub struct OnKey<Msg: Message>(pub DefaultKeyEvent, pub(crate) Callback<Msg, Msg::Model>);

impl<Msg: Message> OnKey<Msg> {
    pub fn with_fn<F>(key: DefaultKeyEvent, func: F) -> Self
    where
        F: Fn(&Msg::Model, &DefaultEvent) -> CallbackRes<Msg> + 'static,
    {
        OnKey(key, Arc::new(func))
    }
    pub fn new(key: DefaultKeyEvent, app_msg: Msg) -> Self {
        OnKey(key, Arc::new(msg(app_msg)))
    }
}

pub fn msg<Model, Msg>(msg: Msg) -> impl Fn(&Model, &DefaultEvent) -> CallbackRes<Msg> + 'static
where
    Msg: Clone + Send + Sync + 'static,
{
    move |_, _| Some((msg.clone(), Effect::none()))
}

unsafe impl<Msg: Message> Send for OnKey<Msg> {}
unsafe impl<Msg: Message> Sync for OnKey<Msg> {}

#[must_use]
#[derive(Clone)]
pub struct OnClick<Msg: Message>(pub(crate) Callback<Msg, Msg::Model>);

unsafe impl<Msg: Message> Send for OnClick<Msg> {}
unsafe impl<Msg: Message> Sync for OnClick<Msg> {}

impl<Msg: Message> OnClick<Msg> {
    pub fn with_fn<F>(func: F) -> Self
    where
        F: Fn(&Msg::Model, &DefaultEvent) -> CallbackRes<Msg> + 'static,
    {
        OnClick(Arc::new(func))
    }
    pub fn new(app_msg: Msg) -> Self {
        OnClick(Arc::new(msg(app_msg)))
    }
    #[must_use]
    pub fn keybind(self, key: DefaultKeyEvent) -> OnClickOrKey<Msg> {
        OnClickOrKey(key, self.0)
    }
}

#[must_use]
#[derive(Clone)]
pub struct OnClickOrKey<Msg: Message>(DefaultKeyEvent, Callback<Msg, Msg::Model>);

unsafe impl<Msg: Message> Send for OnClickOrKey<Msg> {}
unsafe impl<Msg: Message> Sync for OnClickOrKey<Msg> {}

impl<Msg: Message> OnClickOrKey<Msg> {
    pub fn with_fn<F>(key: DefaultKeyEvent, func: F) -> Self
    where
        F: Fn(&Msg::Model, &DefaultEvent) -> CallbackRes<Msg> + 'static,
    {
        OnClickOrKey(key, Arc::new(func))
    }
    pub fn new(key: DefaultKeyEvent, app_msg: Msg) -> Self {
        OnClickOrKey(key, Arc::new(msg(app_msg)))
    }
}

pub(crate) fn specialize_on_click_or_key_handlers<Msg: Message>(world: &mut World) {
    let mut cmd = CommandBuffer::new();

    world
        .query_mut::<(Entity, &OnClickOrKey<Msg>)>()
        .into_iter()
        .for_each(|(entity, OnClickOrKey(key, handler))| {
            cmd.insert_one(entity, OnClick(handler.clone()));
            cmd.insert_one(entity, OnKey(*key, handler.clone()));
        });

    cmd.run_on(world);
}

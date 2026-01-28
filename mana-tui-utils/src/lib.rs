use std::ops::{Deref, DerefMut};

use hecs::World;

pub mod ext;
pub mod resource;
pub mod systems;

pub trait Ecs: Deref<Target = World> + DerefMut<Target = World> {}

impl Ecs for &mut World {}

use std::ops::{Deref, DerefMut};

use hecs::World;

pub mod ext;
pub mod resource;
pub mod systems;

pub trait Ecs: Deref<Target = World> {}
pub trait EcsMut: DerefMut<Target = World> {}

impl Ecs for &mut World {}
impl Ecs for &World {}

impl EcsMut for &mut World {}

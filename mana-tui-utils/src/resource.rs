use std::{any::type_name, marker::PhantomData};

use hecs::{Component, ComponentRef, Entity, RefMut, World};

pub struct Res<T> {
    _ty: PhantomData<T>,
    entity: Entity,
}

unsafe impl<T> Send for Res<T> {}
unsafe impl<T> Sync for Res<T> {}

impl<'a, T: ComponentRef<'a>> Res<T> {
    fn get_res_entity(world: &World) -> Result<(Entity, Entity), hecs::NoSuchEntity> {
        let mut query = world.query::<(Entity, &Res<T::Component>)>();
        let mut iter = query.iter();
        let (entity, res) = iter.next().ok_or(hecs::NoSuchEntity)?;
        assert!(
            iter.next().is_none(),
            "there can only be one resource of type {}",
            type_name::<Res<T>>()
        );
        Ok((entity, res.entity))
    }
    fn get_entity(world: &'a World) -> Result<Entity, hecs::NoSuchEntity> {
        let (_, entity) = Self::get_res_entity(world)?;
        Ok(entity)
    }
    fn query(world: &'a World) -> Result<T::Ref, hecs::ComponentError> {
        let entity = Self::get_entity(world)?;
        world.get::<T>(entity)
    }
}

pub trait Resources {
    fn insert_resource<T: Component>(&'_ mut self, value: T) -> RefMut<'_, T>;
    fn get_resource<'w: 'a, 'a, T: ComponentRef<'a>>(
        &'w self,
    ) -> Result<T::Ref, hecs::ComponentError>;
    fn try_update_resource<T: Component>(&mut self, value: T) -> Result<(), hecs::NoSuchEntity>;
    fn update_resource<T: Component>(&mut self, value: T) {
        self.try_update_resource(value).unwrap_or_else(|_| {
            panic!(
                "tried to update non-existing resource of type {}",
                type_name::<T>()
            )
        });
    }
    fn insert_or_update_resource<T: Component>(&mut self, value: T);
    fn get_or_insert_resource_with<'w: 'a, 'a, T: ComponentRef<'a>>(
        &'w mut self,
        value: impl FnOnce(&mut Self) -> T::Component,
    ) -> T::Ref;
}

impl Resources for World {
    fn insert_resource<T: Component>(&'_ mut self, value: T) -> RefMut<'_, T> {
        let entity = self.spawn((value,));
        self.spawn((Res {
            _ty: PhantomData::<T>,
            entity,
        },));
        self.get_resource::<&mut T>().unwrap()
    }

    fn get_resource<'w: 'a, 'a, T: ComponentRef<'a>>(
        &'w self,
    ) -> Result<T::Ref, hecs::ComponentError> {
        Res::<T>::query(self)
    }

    fn try_update_resource<T: Component>(&mut self, value: T) -> Result<(), hecs::NoSuchEntity> {
        let (res_entity, entity) = Res::<&T>::get_res_entity(self)?;
        _ = self.despawn(entity);
        _ = self.despawn(res_entity);
        self.insert_resource(value);
        Ok(())
    }

    fn insert_or_update_resource<T: Component>(&mut self, value: T) {
        let Ok((res_entity, entity)) = Res::<&T>::get_res_entity(self) else {
            self.insert_resource(value);
            return;
        };
        _ = self.despawn(entity);
        _ = self.despawn(res_entity);
        self.insert_resource(value);
    }

    fn get_or_insert_resource_with<'w: 'a, 'a, T: ComponentRef<'a>>(
        &'w mut self,
        value: impl FnOnce(&mut Self) -> T::Component,
    ) -> T::Ref {
        let Ok((_, entity)) = Res::<T>::get_res_entity(self) else {
            let value = value(self);
            self.insert_resource(value);
            return self.get_resource::<T>().unwrap();
        };
        self.get::<T>(entity).unwrap()
    }
}

#[cfg(test)]
pub mod res_tests {
    use std::collections::HashMap;

    use hecs::World;

    use crate::resource::Resources;

    #[test]
    fn test_resource() {
        let mut world = World::new();
        world.insert_resource(HashMap::<String, i32>::new());
        {
            let mut map = world.get_resource::<&mut HashMap<String, i32>>().unwrap();
            map.insert("hello".to_owned(), 0);
        }
        let map = world.get_resource::<&HashMap<String, i32>>().unwrap();
        assert!(!map.is_empty());
    }

    #[test]
    #[should_panic(
        expected = "there can only be one resource of type mana_tui_beheaded::ecs_ext::Res<&std::collections::hash::map::HashMap<alloc::string::String, i32>>"
    )]
    fn test_multiple_insertions() {
        let mut world = World::new();
        world.insert_resource(HashMap::<String, i32>::new());
        world.insert_resource(HashMap::<String, i32>::new());
        let _map = world.get_resource::<&HashMap<String, i32>>().unwrap();
    }
}

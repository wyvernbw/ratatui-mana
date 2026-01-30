use hecs::{Query, QueryBorrow};

use crate::{Ecs, EcsMut};

pub trait EcsExt: Ecs + Sized {
    fn single<Q: Query>(&self) -> SingleBorrow<'_, Q> {
        SingleBorrow(self.query())
    }
}

pub trait EcsExtMut: EcsMut + Sized {
    fn single_mut<Q: Query>(&mut self) -> Q::Item<'_> {
        self.query_mut::<Q>().into_iter().single()
    }

    fn single_try_mut<Q: Query>(&mut self) -> Option<Q::Item<'_>> {
        self.query_mut::<Q>().into_iter().try_single()
    }
}

impl<E> EcsExt for E where E: Ecs + Sized {}
impl<E> EcsExtMut for E where E: EcsMut + Sized {}

pub struct SingleBorrow<'a, Q: Query>(QueryBorrow<'a, Q>);

impl<Q: Query> SingleBorrow<'_, Q> {
    pub fn get(&mut self) -> Q::Item<'_> {
        self.0.iter().single()
    }
    pub fn try_get(&mut self) -> Option<Q::Item<'_>> {
        self.0.iter().try_single()
    }
}

trait IterSingle: Iterator {
    fn single(&mut self) -> Self::Item;
    fn try_single(&mut self) -> Option<Self::Item>;
}

impl<I> IterSingle for I
where
    I: Iterator,
{
    fn single(&mut self) -> Self::Item {
        let first = self.next();
        let second = self.next();
        assert!(second.is_none(), "single: found more than one element");
        first.expect("single: no elements found")
    }

    fn try_single(&mut self) -> Option<Self::Item> {
        let first = self.next();
        let second = self.next();
        if second.is_some() {
            return None;
        }
        first
    }
}

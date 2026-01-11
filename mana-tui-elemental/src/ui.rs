//! helpers to create ui nodes
//!
//! # Usage
//!
//! ```
//! # use ratatui::widgets::Block;
//! # use mana_tui_elemental::ui::*;
//! # use mana_tui_elemental::prelude::*;
//!
//! let mut ctx = ElementCtx::new();
//! let root = ui(Block::new())
//!     .with((Width(Size::Grow), Height(Size::Fixed(40))))
//!     .children((
//!         ui(Block::new()),
//!         ui(Block::new())
//!     ));
//! ctx.spawn_ui(root);
//!
//! ```

use std::{borrow::Cow, collections::VecDeque, sync::Arc};

use glam::U16Vec2;
use hecs::{CommandBuffer, DynamicBundle, EntityBuilder};
use ratatui::{
    buffer::Buffer,
    layout::{Direction, Rect},
    text::Text,
    widgets::{Block, Padding},
};
use tracing::{Level, enabled, instrument};

use crate::layout::{
    Children, ElWidget, Element, ElementCtx, Gap, Height, Justify, MainJustify, Props, Size,
    TuiElMarker, Width,
};

/// create a ui element.
///
/// # Usage
///
/// ## Arguments
///
/// - `widget`: anything that implements the [`ElWidget`][crate::layout::ElWidget], so ratatui widgets and custom widgets.
///
/// ## Methods
///
/// - [`with`][UiBuilder::with] (optional): adds a component bundle to the element
/// - [`children`][UiBuilder::children] (optional): adds children to the element
/// - [`child`][UiBuilder::child] (optional): like `children`
///
/// # Example
///
/// barebones:
///
/// ```
/// # use ratatui::widgets::Block;
/// # use mana_tui_elemental::ui::*;
/// # use mana_tui_elemental::prelude::*;
///
/// let mut ctx = ElementCtx::new();
/// let root = ui(Block::new());
/// ctx.spawn_ui(root);
///
/// ```
///
/// with components:
///
/// ```
/// # use ratatui::widgets::Block;
/// # use mana_tui_elemental::ui::*;
/// # use mana_tui_elemental::prelude::*;
///
/// let mut ctx = ElementCtx::new();
/// let root = ui(Block::new())
///     .with((Width(Size::Grow), Height(Size::Fixed(40))));
/// ctx.spawn_ui(root);
///
/// ```
///
/// with children:
///
/// ```
/// # use ratatui::widgets::Block;
/// # use mana_tui_elemental::ui::*;
/// # use mana_tui_elemental::prelude::*;
///
/// let mut ctx = ElementCtx::new();
/// let root = ui(Block::new())
///     .children((
///         ui(Block::new()),
///         ui(Block::new())
///     ));
/// ctx.spawn_ui(root);
///
/// ```
///
/// full:
///
/// ```
/// # use ratatui::widgets::Block;
/// # use mana_tui_elemental::ui::*;
/// # use mana_tui_elemental::prelude::*;
///
/// let mut ctx = ElementCtx::new();
/// let root = ui(Block::new())
///     .with((Width(Size::Grow), Height(Size::Fixed(40))))
///     .children((
///         ui(Block::new()),
///         ui(Block::new())
///     ));
/// ctx.spawn_ui(root);
///
/// ```
pub fn ui(w: impl IntoView) -> UiBuilder<ui_builder::Empty> {
    __ui_internal(w.into_view())
}

/// trait that marks a type can be converted into a [`View`].
///
/// automatically implementeed for widgets.
pub trait IntoView {
    /// make the conversion to view.
    fn into_view(self) -> View;
}

impl<W> IntoView for W
where
    W: ElWidget,
{
    fn into_view(self) -> View {
        let mut builder = View::new();
        fn render_system<E: ElWidget>(
            ctx: &ElementCtx,
            entity: hecs::Entity,
            area: Rect,
            buf: &mut Buffer,
        ) {
            if let Ok(widget) = ctx.world.get::<&E>(entity) {
                widget.render_element(area, buf);
            }
        }
        builder.add(self);
        builder.add_bundle((
            TuiElMarker,
            Props {
                size: U16Vec2::default(),
                position: U16Vec2::default(),
                render: render_system::<W>,
            },
        ));
        builder
    }
}

/// internal function.
#[bon::builder]
#[builder(builder_type = UiBuilder)]
#[builder(finish_fn = done)]
pub fn __ui_internal(
    #[builder(start_fn)] view: View,
    #[builder(setters(vis = "", name = children_flag))] _children: Option<()>,
    #[builder(setters(vis = "", name = child_flag))] _child: Option<()>,
) -> EntityBuilder {
    view
}

impl<S> UiBuilder<S>
where
    S: ui_builder::State,
    S::Children: ui_builder::IsUnset,
    S::Child: ui_builder::IsUnset,
{
    /// sets the children of the element. the argument must implement [`IntoUiBuilderList`], which is
    /// implemented automatically for `N`-tuples, [`Vec<T>`] and arrays.
    ///
    /// can only be set once.
    ///
    /// NOTE: if using vecs or arrays, call [`UiBuilder::done`] in order to obtain the [`hecs::EntityBuilder`] for each element
    /// in order to store it.
    #[must_use = "You can use the builder with ElementCtx::spawn_ui"]
    pub fn children<M>(
        mut self,
        children: impl IntoUiBuilderList<M>,
    ) -> UiBuilder<impl ui_builder::State> {
        let children = children.into_list().collect::<Box<[_]>>();
        self.view.add(ChildrenBuilders(children));
        self.children_flag(())
    }
}

impl<S> UiBuilder<S>
where
    S: ui_builder::State,
    S::Children: ui_builder::IsUnset,
    S::Child: ui_builder::IsUnset,
{
    /// like [`UiBuilder::child`], but only takes one child.
    ///
    /// can only be set once.
    ///
    /// this method exists as a convenience so you don't have to do `.children((child,))` with a 1-tuple.
    #[must_use = "You can use the builder with ElementCtx::spawn_ui"]
    pub fn child(mut self, child: impl Into<EntityBuilder>) -> UiBuilder<impl ui_builder::State> {
        self.view.add(ChildrenBuilders(Box::new([child.into()])));
        self.child_flag(())
    }
}

impl<S> UiBuilder<S>
where
    S: ui_builder::State,
{
    /// adds the dynamic bundle to the elments components.
    ///
    /// this method can be set repeatedly. if the element already contained some of the bundle's components,
    /// they will be replaced.
    ///
    /// # Example
    /// ```
    /// # use ratatui::widgets::Block;
    /// # use mana_tui_elemental::ui::*;
    /// # use mana_tui_elemental::prelude::*;
    ///
    /// ui(Block::new())
    ///     .with((
    ///         Width(Size::Grow),
    ///         Height(Size::Fixed(40)),
    ///         Padding::uniform(1),
    ///     ));
    /// ```
    #[must_use = "You can use the builder with ElementCtx::spawn_ui"]
    pub fn with(
        mut self,
        bundle: impl DynamicBundle,
    ) -> UiBuilder<impl ui_builder::State<Children = S::Children, Child = S::Child>> {
        self.view.add_bundle(bundle);
        self
    }
}

impl<S> From<UiBuilder<S>> for EntityBuilder
where
    S: ui_builder::IsComplete,
{
    fn from(val: UiBuilder<S>) -> Self {
        val.done()
    }
}

/// trait that marks a type can be converted into an iterator over [`hecs::EntityBuilder`].
///
/// automatically implemented for N-tuples, vecs and arrays.
pub trait IntoUiBuilderList<Marker = ()> {
    /// convert into iterator.
    fn into_list(self) -> impl Iterator<Item = EntityBuilder>;
}
/// alias for `IntoUiBuilderList<IteratorMarker>`
#[cfg(feature = "nightly")]
pub trait AsChildren = IntoUiBuilderList<IteratorMarker>;

/// internal struct.
pub struct IteratorMarker;
impl<I> IntoUiBuilderList<IteratorMarker> for I
where
    I: IntoIterator<Item = EntityBuilder>,
{
    fn into_list(self) -> impl Iterator<Item = EntityBuilder> {
        self.into_iter()
    }
}

impl IntoUiBuilderList<()> for &'static str {
    fn into_list(self) -> impl Iterator<Item = EntityBuilder> {
        [ui(Text::raw(self))
            .with((Width::grow(), Height::grow()))
            .done()]
        .into_iter()
    }
}

impl IntoUiBuilderList<()> for String {
    fn into_list(self) -> impl Iterator<Item = EntityBuilder> {
        [ui(Text::raw(self))
            .with((Width::grow(), Height::grow()))
            .done()]
        .into_iter()
    }
}

impl<'a> IntoUiBuilderList<()> for Cow<'a, str> {
    fn into_list(self) -> impl Iterator<Item = EntityBuilder> {
        [ui(Text::raw(self.into_owned()))
            .with((Width::grow(), Height::grow()))
            .done()]
        .into_iter()
    }
}

enum OptionIterator<T> {
    Some(T),
    None,
}

impl<T, U> Iterator for OptionIterator<T>
where
    T: Iterator<Item = U>,
{
    type Item = U;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            OptionIterator::Some(iter) => iter.next(),
            OptionIterator::None => None,
        }
    }
}

/// internal marker type.
pub struct OptionMarker;

impl<T> IntoUiBuilderList<OptionMarker> for Option<T>
where
    T: IntoUiBuilderList,
{
    fn into_list(self) -> impl Iterator<Item = EntityBuilder> {
        match self {
            Some(value) => OptionIterator::Some(value.into_list()),
            None => OptionIterator::None,
        }
    }
}

macro_rules! impl_into_ui_builder_list_for_tuples {
    ($($idx:tt $name:ident),+) => {
        impl<$($name),+> IntoUiBuilderList<()> for ($($name,)+)
        where
            $($name: Into<EntityBuilder>,)+
        {
            fn into_list(self) -> impl Iterator<Item = EntityBuilder> {
                [$(self.$idx.into()),+].into_iter()
            }
        }
    };
}

// Generate implementations for tuples of size 1 through 12
impl_into_ui_builder_list_for_tuples!(0 U0);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1, 2 U2);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1, 2 U2, 3 U3);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1, 2 U2, 3 U3, 4 U4);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1, 2 U2, 3 U3, 4 U4, 5 U5);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1, 2 U2, 3 U3, 4 U4, 5 U5, 6 U6);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1, 2 U2, 3 U3, 4 U4, 5 U5, 6 U6, 7 U7);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1, 2 U2, 3 U3, 4 U4, 5 U5, 6 U6, 7 U7, 8 U8);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1, 2 U2, 3 U3, 4 U4, 5 U5, 6 U6, 7 U7, 8 U8, 9 U9);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1, 2 U2, 3 U3, 4 U4, 5 U5, 6 U6, 7 U7, 8 U8, 9 U9, 10 U10);
impl_into_ui_builder_list_for_tuples!(0 U0, 1 U1, 2 U2, 3 U3, 4 U4, 5 U5, 6 U6, 7 U7, 8 U8, 9 U9, 10 U10, 11 U11);

pub(crate) struct ChildrenBuilders(pub(crate) Box<[EntityBuilder]>);

#[instrument(skip(world))]
fn process_ui_system(world: &mut ElementCtx) {
    let mut to_process: VecDeque<Element> = world
        .query_mut::<&ChildrenBuilders>()
        .into_iter()
        .map(|(e, _)| e)
        .collect();

    while let Some(node) = to_process.pop_front() {
        if let Ok(builders) = world.remove_one::<ChildrenBuilders>(node) {
            let mut builders = builders.0;
            world.reserve_entities(builders.len() as u32);
            let children = builders
                .iter_mut()
                .map(|builder| {
                    let builder = builder.build();
                    let has_children = builder.has::<ChildrenBuilders>();
                    let entity = world.spawn(builder);
                    if has_children {
                        to_process.push_back(entity);
                    }
                    entity
                })
                .collect();
            world
                .insert_one(node, Children::Some(Arc::new(children)))
                .unwrap();
        }
    }

    let mut buffer = CommandBuffer::new();

    for (node, (block, padding)) in world.query_mut::<(&mut Block, Option<&Padding>)>() {
        if padding.is_none() {
            tracing::trace!(?node, "processing default padding for block",);
            let test_area = Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            };
            let inner_area = block.inner(test_area);
            let left = inner_area.left() - test_area.left();
            let top = inner_area.top() - test_area.top();
            let right = (test_area.height - inner_area.height).saturating_sub(1);
            let bottom = (test_area.height - inner_area.height).saturating_sub(1);
            buffer.insert_one(
                node,
                Padding {
                    left,
                    top,
                    right,
                    bottom,
                },
            );
        }
    }
    for (node, (_, width, height)) in world.query_mut::<(&Text, Option<&Width>, Option<&Height>)>()
    {
        if enabled!(Level::TRACE) && (width.is_none() || height.is_none()) {
            tracing::trace!(?node, "processing default size for text",);
        }
        if width.is_none() {
            buffer.insert_one(node, Width::grow());
        }
        if height.is_none() {
            buffer.insert_one(node, Height::grow());
        }
    }
    buffer.run_on(world);

    let mut query = world.query::<&TuiElMarker>();
    for (node, _) in query.iter() {
        let entity = world.entity(node).unwrap();
        if !entity.has::<Width>() {
            buffer.insert_one(node, Width(Size::Fit));
        }
        if !entity.has::<Height>() {
            buffer.insert_one(node, Height(Size::Fit));
        }
        if !entity.has::<Direction>() {
            buffer.insert_one(node, Direction::Vertical);
        }
        if !entity.has::<MainJustify>() {
            buffer.insert_one(node, MainJustify(Justify::Start));
        }
        if !entity.has::<Gap>() {
            buffer.insert_one(node, Gap::default());
        }
        if !entity.has::<Padding>() {
            buffer.insert_one(node, Padding::default());
        }
        if !entity.has::<Children>() {
            buffer.insert_one(node, Children::None);
        }
    }
    drop(query);

    buffer.run_on(world);
}

impl ElementCtx {
    /// spawns the root element along with its children.
    ///
    /// use this method instead of [`hecs::World::spawn`] as it also spawns all children
    /// recursively using a queue in `O(n)` time where `n` is the number of elements with children.
    ///
    /// also see [`ui`], [`Element`][crate::layout::Element]
    pub fn spawn_ui(&mut self, ui: impl Into<EntityBuilder>) -> Element {
        let mut ui = ui.into();
        let ui = ui.build();
        let root = self.spawn(ui);
        process_ui_system(self);
        root
    }
}

/// ui struct that can be spawned into the ecs. it is used to represent a tree of elements.
/// subviews can return this type.
///
/// # Example
///
/// using manasx:
///
/// ```
/// use mana_tui_elemental::prelude::*;
/// use mana_tui_macros::{subview, ui};
///
/// #[subview]
/// fn subview_test(name: &'static str) -> View {
///     ui! {
///         { format!("Hello {name}!") }
///     }
/// }
///
/// let root = ui! {
///     <Block .title_top="sidebar" Width::fixed(10) Padding::uniform(1)>
///         <Block .title_top="2" />
///         <SubviewTest .name="there" />
///     </Block>
/// };
/// ```
///
/// using builder sytnax:
///
/// ```
/// use mana_tui_elemental::prelude::*;
/// use mana_tui_macros::{subview, ui};
///
/// fn subview_test(name: &'static str) -> impl Into<View> {
///     ui(Text::raw(format!("hello {name}")))
/// }
///
/// let root = ui(Block::new().title_top("sidebar"))
/// .with((
///     Width::fixed(10), Padding::uniform(1)
/// ))
/// .children((
///     ui(Block::new().title_top("2")),
///     subview_test("there")
/// ));
///
pub type View = EntityBuilder;

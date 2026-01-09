use std::sync::Arc;

use bon::Builder;
use derive_more as d;
use glam::{U16Vec2, u16vec2};
use hecs::{Bundle, Component, ComponentError, Entity, Query, World};
use ratatui::{
    buffer::Buffer,
    layout::{Direction, Rect},
    widgets::{Padding, Widget},
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

pub trait ElWidget: std::fmt::Debug + Component {
    fn render_element(&self, area: Rect, buf: &mut Buffer);
}

impl<W: 'static> ElWidget for W
where
    W: Widget + Clone + std::fmt::Debug + Component,
{
    fn render_element(&self, area: Rect, buf: &mut Buffer) {
        self.clone().render(area, buf);
    }
}

#[derive(Default, d::Deref, d::DerefMut)]
pub struct ElementCtx {
    #[deref]
    #[deref_mut]
    pub(crate) world: World,
}

#[derive(Bundle)]
struct ElementBundle {
    props: Props,
    width: Width,
    height: Height,
    children: Children,
    padding: Padding,
    direction: Direction,
    gap: Gap,
}

impl ElementCtx {
    fn calculate_fit_sizes(&self, element: Element) -> Result<(), ComponentError> {
        let mut query = self
            .world
            .query_one::<(&Width, &Height, &Padding, &Children, &Direction)>(element)?;
        let (width, height, padding, children, direction) = query.get().unwrap();
        let mut props_query = self.world.query_one::<&mut Props>(element)?;
        let props = props_query.get().unwrap();

        if let Size::Fixed(size) = **width {
            props.size.x = size
        }
        if let Size::Fixed(size) = **height {
            props.size.y = size
        }
        let max_size = props.size.saturating_sub(u16vec2(
            padding.right + padding.left,
            padding.bottom + padding.top,
        ));

        let mut space_used = AxisSizes::default();

        drop(props_query);

        children
            .iter()
            .copied()
            .try_for_each(|child| -> Result<(), ComponentError> {
                self.calculate_fit_sizes(child)?;
                Ok(())
            })?;

        for child in children.iter().copied() {
            let mut child_props = self.world.get::<&mut Props>(child)?;
            if width.should_clamp() {
                child_props.size.x = child_props.size.x.clamp(0, max_size.x);
            }
            if height.should_clamp() {
                child_props.size.y = child_props.size.y.clamp(0, max_size.x);
            }
            space_used = space_used.increase(child_props.size, *direction);
        }

        let mut query = self.world.query_one::<(
            &mut Props,
            &Width,
            &Height,
            &Padding,
            &Children,
            &Direction,
            &Gap,
        )>(element)?;
        let (props, width, height, padding, children, direction, gap) = query.get().unwrap();

        space_used = space_used.pad(*padding, *direction);
        space_used.main_axis += children.len().saturating_sub(1) as u16 * **gap;
        let space_used = space_used.to_u16vec2(*direction);
        match **width {
            Size::Fit | Size::Grow => {
                props.size.x = space_used.x;
            }
            _ => {}
        }
        match **height {
            Size::Fit | Size::Grow => {
                props.size.y = space_used.y;
            }
            _ => {}
        }
        Ok(())
    }
    fn sum_space_used(&self, elements: &[Element]) -> U16Vec2 {
        elements
            .iter()
            .copied()
            .flat_map(|child| self.world.get::<&Props>(child))
            .map(|props| props.size)
            .sum::<U16Vec2>()
    }
    fn calculate_grow_sizes(&self, element: Element) -> Result<(), ComponentError> {
        let mut query = self
            .world
            .query_one::<(&mut Props, &Padding, &Children, &Direction, &Gap)>(element)?;
        let (props, &padding, children, &direction, &gap) = query.get().unwrap();
        let children = children.clone();
        let inner_size = props.size.saturating_sub(u16vec2(
            padding.right + padding.left,
            padding.bottom + padding.top,
        ));
        drop(query);
        let space_used = self.sum_space_used(&children);
        let remaining_size = inner_size.saturating_sub(space_used);
        let mut remaining_size = axify(remaining_size, direction);
        remaining_size.main_axis = remaining_size
            .main_axis
            .saturating_sub(children.len().saturating_sub(1) as u16 * *gap);

        // cross axis
        children
            .iter()
            .copied()
            .try_for_each(|child| -> Result<(), ComponentError> {
                let mut child_query = self
                    .world
                    .query_one::<(&mut Props, &Width, &Height)>(child)?;
                let (child_props, child_width, child_height) = child_query.get().unwrap();
                if !cross_size(direction, *child_width, *child_height).is_grow() {
                    return Ok(());
                }
                let mut size = AxisSizes::from_u16vec2(child_props.size, direction);
                size.cross_axis = axify(inner_size, direction).cross_axis;
                child_props.size = size.to_u16vec2(direction);
                Ok(())
            })?;

        // main axis
        #[derive(Query, Debug)]
        struct GrowQuery<'a> {
            props: &'a mut Props,
            width: &'a Width,
            height: &'a Height,
        }
        #[derive(d::Debug)]
        struct GrowEntry {
            is_grow: bool,
            #[debug("({}, {})", self.size.main_axis, self.size.cross_axis)]
            size: AxisSizes,
            entity: Element,
        }
        let mut buffer = children
            .iter()
            .copied()
            .flat_map(|child| self.query_one::<GrowQuery>(child).ok().zip(Some(child)))
            .map(|(mut grow_query, entity)| {
                let grow_query = grow_query.get().unwrap();
                let is_grow = main_size(direction, *grow_query.width, *grow_query.height).is_grow();
                let size = axify(grow_query.props.size, direction);
                GrowEntry {
                    is_grow,
                    size,
                    entity,
                }
            })
            .collect::<Vec<_>>();
        buffer.sort_by_key(|entry| entry.size.main_axis);
        let mut remaining = remaining_size.main_axis;
        while let Some([smallest, rest @ ..]) = buffer.get_mut(..) {
            let second_smallest = rest
                .iter()
                .position(|entry| entry.size.main_axis != smallest.size.main_axis);
            match second_smallest {
                None => {
                    // distribute remaining space evenly
                    // +1 to include smallest element
                    let grow_count = rest.iter().filter(|entry| entry.is_grow).count() + 1;
                    if grow_count == 0 {
                        break;
                    }
                    let growth = remaining as usize / grow_count;
                    let growth = growth as u16;
                    let remainder = remaining as usize % grow_count;
                    let mut remainder = remainder as u16;
                    for entry in buffer.iter_mut() {
                        if !entry.is_grow {
                            continue;
                        }
                        match remainder {
                            0 => {
                                entry.size.main_axis += growth;
                            }
                            _ => {
                                entry.size.main_axis += growth + 1;
                                remainder -= 1;
                            }
                        }
                    }
                    break;
                }
                Some(second_smallest) => {
                    let end = second_smallest;
                    let target_size = rest[second_smallest].size.main_axis;
                    remaining = remaining
                        .saturating_sub(target_size.saturating_sub(smallest.size.main_axis));
                    for entry in buffer[..=end].iter_mut() {
                        if entry.is_grow {
                            entry.size.main_axis = target_size;
                        }
                    }
                }
            }
        }

        for entry in buffer {
            let mut query = self.query_one::<GrowQuery>(entry.entity)?;
            let query = query.get().unwrap();
            query.props.size = entry.size.to_u16vec2(direction);
        }

        for child in children.iter().copied() {
            self.calculate_grow_sizes(child)?;
        }

        Ok(())
    }
    fn calculate_positions(&self, root: Element) -> Result<(), ComponentError> {
        let mut query = self
            .world
            .query_one::<(&Props, &Padding, &Children, &Direction, &Gap, &MainJustify)>(root)?;
        let (&props, &padding, children, &dir, &gap, &main_justify) = query.get().unwrap();
        let children = children.clone();
        drop(query);
        let space_used = self.sum_space_used(&children);
        let space_used = axify(space_used, dir).main_axis;
        let space_used = space_used + *gap * children.len().saturating_sub(1) as u16;
        let remaining_size = axify(props.size, dir)
            .shrink(padding, dir)
            .main_axis
            .saturating_sub(space_used);

        #[derive(Default)]
        struct AlignValues {
            start: u16,
            inbetween: u16,
            remainder: u16,
        }

        impl AlignValues {
            fn tick_rem(&mut self) -> u16 {
                match self.remainder {
                    0 => 0,
                    1.. => {
                        self.remainder -= 1;
                        1
                    }
                }
            }
        }

        let mut align = match *main_justify {
            Justify::Start => AlignValues::default(),
            Justify::Center => AlignValues {
                start: remaining_size / 2,
                inbetween: 0,
                remainder: 0,
            },
            Justify::SpaceBetween if children.is_empty() => AlignValues::default(),
            Justify::SpaceBetween => {
                let div_by = (children.len().saturating_sub(1)) as u16;
                let space = remaining_size / div_by;
                let space_rem = remaining_size % div_by;
                AlignValues {
                    start: 0,
                    inbetween: space,
                    remainder: space_rem,
                }
            }
            Justify::SpaceAround if children.is_empty() => AlignValues::default(),
            Justify::SpaceAround => {
                let div_by = (children.len() * 2) as u16;
                let space = remaining_size / div_by;
                let space_rem = remaining_size % div_by;
                AlignValues {
                    start: space,
                    inbetween: space * 2,
                    remainder: space_rem,
                }
            }
            Justify::SpaceEvenly if children.is_empty() => AlignValues::default(),
            Justify::SpaceEvenly => {
                let div_by = (children.len() * 2) as u16 + 2;
                let space = remaining_size / div_by;
                AlignValues {
                    start: space * 2,
                    inbetween: space * 2,
                    remainder: 0,
                }
            }
            Justify::End => AlignValues {
                start: remaining_size,
                inbetween: 0,
                remainder: 0,
            },
        };

        children
            .iter()
            .copied()
            .try_for_each(|child| -> Result<(), ComponentError> {
                {
                    let mut child_props = self.world.get::<&mut Props>(child)?;
                    child_props.position = props.position;
                    match dir {
                        Direction::Horizontal => child_props.position.x += align.start,
                        Direction::Vertical => child_props.position.y += align.start,
                    }
                    child_props.position += u16vec2(padding.left, padding.top);
                    align.start = increase_axis(align.start, dir, child_props.size);
                    align.start += *gap + align.inbetween + align.tick_rem();
                }
                self.calculate_positions(child)?;
                Ok(())
            })?;

        Ok(())
    }
    pub fn calculate_layout(&mut self, element: Element) -> Result<(), ComponentError> {
        self.calculate_fit_sizes(element)?;
        self.calculate_grow_sizes(element)?;
        self.calculate_positions(element)?;
        Ok(())
    }
    pub fn render(&self, root: Element, area: Rect, buf: &mut Buffer) {
        let mut query = self
            .world
            .query_one::<(&mut Props, Option<&Children>)>(root)
            .expect("mana-tui-elemental bug: root element must have props");
        let (props, children) = query.get().unwrap();
        let area = props.split_area(area);
        (props.render)(self, root, area, buf);
        if let Some(children) = children {
            let children = children.clone();
            drop(query);
            for child in children.iter().copied() {
                self.render(child, area, buf);
            }
        }
    }
}

fn increase_axis(init: u16, dir: Direction, size: U16Vec2) -> u16 {
    match dir {
        Direction::Horizontal => init + size.x,
        Direction::Vertical => init + size.y,
    }
}

#[derive(
    Debug, Clone, Copy, Default, d::Sub, d::SubAssign, d::Add, d::AddAssign, d::Sum, PartialEq, Eq,
)]
struct AxisSizes {
    main_axis: u16,
    cross_axis: u16,
}

const fn axify(vec: U16Vec2, dir: Direction) -> AxisSizes {
    AxisSizes::from_u16vec2(vec, dir)
}

impl AxisSizes {
    #[inline(always)]
    fn min(self, other: AxisSizes) -> AxisSizes {
        AxisSizes {
            main_axis: self.main_axis.min(other.main_axis),
            cross_axis: self.cross_axis.min(other.cross_axis),
        }
    }
    const fn with_main(mut self, value: u16) -> Self {
        self.main_axis = value;
        self
    }
    #[inline(always)]
    const fn from_u16vec2(value: U16Vec2, dir: Direction) -> Self {
        match dir {
            Direction::Horizontal => Self {
                main_axis: value.x,
                cross_axis: value.y,
            },
            Direction::Vertical => Self {
                main_axis: value.y,
                cross_axis: value.x,
            },
        }
    }
    #[inline(always)]
    const fn pad(self, padding: Padding, dir: Direction) -> AxisSizes {
        match dir {
            Direction::Horizontal => AxisSizes {
                main_axis: self.main_axis + padding.left + padding.right,
                cross_axis: self.cross_axis + padding.top + padding.bottom,
            },
            Direction::Vertical => AxisSizes {
                main_axis: self.main_axis + padding.top + padding.bottom,
                cross_axis: self.cross_axis + padding.left + padding.right,
            },
        }
    }
    #[inline(always)]
    const fn shrink(self, padding: Padding, dir: Direction) -> AxisSizes {
        match dir {
            Direction::Horizontal => AxisSizes {
                main_axis: self.main_axis.saturating_sub(padding.left + padding.right),
                cross_axis: self.cross_axis.saturating_sub(padding.top + padding.bottom),
            },
            Direction::Vertical => AxisSizes {
                main_axis: self.main_axis.saturating_sub(padding.top + padding.bottom),
                cross_axis: self.cross_axis.saturating_sub(padding.left + padding.right),
            },
        }
    }
    #[inline(always)]
    fn increase(self, by: U16Vec2, dir: Direction) -> AxisSizes {
        match dir {
            Direction::Horizontal => AxisSizes {
                main_axis: self.main_axis + by.x,
                cross_axis: self.cross_axis.max(by.y),
            },
            Direction::Vertical => AxisSizes {
                main_axis: self.main_axis + by.y,
                cross_axis: self.cross_axis.max(by.x),
            },
        }
    }
    #[inline(always)]
    const fn to_u16vec2(self, dir: Direction) -> U16Vec2 {
        match dir {
            Direction::Horizontal => u16vec2(self.main_axis, self.cross_axis),
            Direction::Vertical => u16vec2(self.cross_axis, self.main_axis),
        }
    }
}

pub type Element = Entity;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Props {
    pub(crate) size: U16Vec2,
    pub(crate) position: U16Vec2,
    pub(crate) render: fn(&ElementCtx, Element, Rect, &mut Buffer),
}

impl Props {
    fn split_area(&self, area: Rect) -> Rect {
        area.intersection(Rect {
            // DONE: implement position
            x: self.position.x,
            y: self.position.y,
            width: self.size.x,
            height: self.size.y,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, d::Deref)]
pub struct Width(pub Size);
#[derive(Debug, Clone, Copy, Default, d::Deref)]
pub struct Height(pub Size);
#[derive(Debug, Clone, Copy, Default, d::Deref)]
pub struct Gap(pub u16);
#[derive(Debug, Clone, Copy, Default, d::Deref)]
pub struct MainJustify(pub Justify);
#[derive(Debug, Clone, Default, d::Deref)]
pub struct Children(pub Arc<Vec<Element>>);

#[inline(always)]
fn cross_size(dir: Direction, x: Width, y: Height) -> Size {
    match dir {
        Direction::Horizontal => *y,
        Direction::Vertical => *x,
    }
}
#[inline(always)]
fn main_size(dir: Direction, x: Width, y: Height) -> Size {
    match dir {
        Direction::Horizontal => *x,
        Direction::Vertical => *y,
    }
}

#[derive(Default, Builder)]
pub struct LayoutParams {
    #[builder(default)]
    pub width: Size,
    #[builder(default)]
    pub height: Size,
    #[builder(default)]
    pub direction: Direction,
    #[builder(default)]
    pub padding: Padding,
    #[builder(default)]
    pub gap: u16,
    #[builder(default)]
    pub main_justify: Justify,
}

impl LayoutParams {}

#[derive(Default, Clone, Copy, Debug)]
pub enum Size {
    Fixed(u16),
    #[default]
    Fit,
    Grow,
}

#[derive(Default, Clone, Copy, Debug)]
pub enum Justify {
    #[default]
    Start,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    End,
}

impl Justify {
    pub fn iter() -> impl Iterator<Item = Justify> {
        [
            Self::Start,
            Self::Center,
            Self::SpaceBetween,
            Self::SpaceAround,
            Self::SpaceEvenly,
            Self::End,
        ]
        .into_iter()
    }
}

impl Size {
    fn should_clamp(&self) -> bool {
        match self {
            Size::Fixed(_) => true,
            Size::Fit => false,
            Size::Grow => false,
        }
    }
    fn is_grow(&self) -> bool {
        matches!(self, Size::Grow)
    }
}

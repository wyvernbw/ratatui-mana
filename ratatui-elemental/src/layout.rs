use std::sync::Arc;

use bon::Builder;
use derive_more as d;
use generational_arena::{Arena, Index};
use glam::{U16Vec2, u16vec2};
use ratatui::{
    buffer::Buffer,
    layout::{Direction, Rect},
    widgets::{Padding, Widget},
};
use smallbox::SmallBox;

type ElementArena = Arena<TuiElement>;
type WidgetBox = SmallBox<dyn ElWidget, usize>;
type WidgetArena = Arena<WidgetBox>;

pub trait ElWidget: std::fmt::Debug {
    fn render_element(&self, area: Rect, buf: &mut Buffer);
}

impl<W> ElWidget for W
where
    W: Widget + Clone + std::fmt::Debug,
{
    fn render_element(&self, area: Rect, buf: &mut Buffer) {
        self.clone().render(area, buf);
    }
}

#[derive(Default)]
pub struct ElementCtx {
    elements: ElementArena,
    widgets: WidgetArena,
}

impl std::ops::Index<ElementIdx> for ElementCtx {
    type Output = TuiElement;

    fn index(&self, index: ElementIdx) -> &Self::Output {
        &self.elements[*index]
    }
}

impl std::ops::IndexMut<ElementIdx> for ElementCtx {
    fn index_mut(&mut self, index: ElementIdx) -> &mut Self::Output {
        &mut self.elements[*index]
    }
}

#[bon::bon]
impl ElementCtx {
    #[builder(finish_fn = create)]
    pub fn element<W>(
        #[builder(start_fn)] widget: W,
        #[builder(finish_fn)] ctx: &mut Self,
        #[builder(default)] layout_params: LayoutParams,
        children: Option<Vec<ElementIdx>>,
    ) -> ElementIdx
    where
        W: ElWidget + 'static,
    {
        let children = children.unwrap_or_default();
        let children = Arc::new(children);
        let widget_idx = ctx.widgets.insert(SmallBox::new(widget) as WidgetBox);
        let widget_idx = WidgetIdx(widget_idx);
        let element = TuiElement {
            widget: widget_idx,
            layout_params,
            size: U16Vec2::default(),
            position: U16Vec2::default(),
            children,
        };
        let element_idx = ctx.elements.insert(element);

        ElementIdx(element_idx)
    }
    fn calculate_fit_sizes(&mut self, element: ElementIdx) {
        if let Size::Fixed(size) = self[element].layout_params.width {
            self[element].size.x = size
        }
        if let Size::Fixed(size) = self[element].layout_params.height {
            self[element].size.y = size
        }
        let children = self[element].children.clone();
        let padding = self[element].layout_params.padding;
        let max_size = self[element].size.saturating_sub(u16vec2(
            padding.right + padding.left,
            padding.bottom + padding.top,
        ));
        let direction = self[element].layout_params.direction;
        let mut space_used = AxisSizes::default();
        for child in children.iter().copied() {
            self.calculate_fit_sizes(child);
            if self[element].layout_params.width.should_clamp() {
                self[child].size.x = self[child].size.x.clamp(0, max_size.x);
            }
            if self[element].layout_params.width.should_clamp() {
                self[child].size.y = self[child].size.y.clamp(0, max_size.x);
            }
            space_used = space_used.increase(self[child].size, direction);
        }
        space_used = space_used.pad(padding, direction);
        space_used.main_axis +=
            children.len().saturating_sub(1) as u16 * self[element].layout_params.gap;
        let space_used = space_used.to_u16vec2(direction);
        match self[element].layout_params.width {
            Size::Fit | Size::Grow => {
                self[element].size.x = space_used.x;
            }
            _ => {}
        }
        match self[element].layout_params.height {
            Size::Fit | Size::Grow => {
                self[element].size.y = space_used.y;
            }
            _ => {}
        }
    }
    fn calculate_grow_sizes(&mut self, element: ElementIdx) {
        let children = self[element].children.clone();
        let padding = self[element].layout_params.padding;
        let max_size = self[element].size.saturating_sub(u16vec2(
            padding.right + padding.left,
            padding.bottom + padding.top,
        ));
        let direction = self[element].layout_params.direction;
        let used_space = children
            .iter()
            .copied()
            .map(|child| self[child].size)
            .sum::<U16Vec2>();
        let remaining_size = self[element]
            .size
            .saturating_sub(used_space)
            .clamp(U16Vec2::ZERO, max_size);
        let mut remaining_size = axify(remaining_size, direction);
        remaining_size.main_axis = remaining_size.main_axis.saturating_sub(
            children.len().saturating_sub(1) as u16 * self[element].layout_params.gap,
        );

        // cross axis
        for child in children.iter().copied() {
            if !self[child].layout_params.cross_size(direction).is_grow() {
                continue;
            }
            let mut size = AxisSizes::from_u16vec2(self[child].size, direction);
            size.cross_axis = axify(max_size, direction).cross_axis;
            self[child].size = size.to_u16vec2(direction);
        }

        // main axis
        while remaining_size.main_axis > 0 {
            let mut smallest: [Option<ElementIdx>; 2] = [None, None];
            let mut first = None;
            let mut all_equal = true;
            let mut grow_count = 0;
            for child in children.iter().copied() {
                let is_grow = self[child].layout_params.main_size(direction).is_grow();
                if !is_grow {
                    continue;
                }
                let size = self[child].size;
                let size = AxisSizes::from_u16vec2(size, direction);
                if first.is_some() && Some(size) != first {
                    all_equal = false;
                }
                grow_count += 1;
                first = Some(size);
                match smallest {
                    [None, None] => {
                        smallest[0] = Some(child);
                    }
                    [Some(a), None] => {
                        let asize = axify(self[a].size, direction);
                        if asize.main_axis < size.main_axis {
                            smallest[1] = Some(child);
                        } else if size.main_axis < asize.main_axis {
                            smallest[1] = smallest[0];
                            smallest[0] = Some(child);
                        }
                    }
                    [Some(a), Some(b)] => {
                        let asize = axify(self[a].size, direction);
                        let bsize = axify(self[b].size, direction);
                        if asize.main_axis < size.main_axis {
                            smallest[1] = smallest[0];
                            smallest[0] = Some(child);
                        } else if size.main_axis < bsize.main_axis {
                            smallest[1] = Some(child);
                        }
                    }
                    _ => unreachable!(),
                }
            }
            if all_equal && grow_count > 0 {
                let remainder = remaining_size.main_axis % grow_count;
                let remaining_size = remaining_size.main_axis / grow_count;
                let mut first = true;
                for child in children.iter().copied() {
                    let is_grow = self[child].layout_params.main_size(direction).is_grow();
                    if !is_grow {
                        continue;
                    }
                    let mut size = axify(self[child].size, direction);
                    size.main_axis = remaining_size;
                    if first {
                        size.main_axis += remainder;
                        first = false;
                    }
                    self[child].size = size.to_u16vec2(direction);
                }
                break;
            }
            match smallest {
                [Some(a), Some(b)] => {
                    let mut asize = axify(self[a].size, direction);
                    let bsize = axify(self[b].size, direction);
                    assert!(asize.main_axis != bsize.main_axis);
                    remaining_size = remaining_size.min(remaining_size - (bsize - asize));
                    asize.main_axis = remaining_size.main_axis;
                    self[a].size = asize.to_u16vec2(direction);
                }
                [Some(a), None] => {
                    let mut asize = axify(self[a].size, direction);
                    asize.main_axis = remaining_size.main_axis;
                    self[a].size = asize.to_u16vec2(direction);
                    break;
                }
                [None, None] => break,
                [None, Some(_)] => unreachable!(),
            }
        }

        for child in children.iter().copied() {
            self.calculate_grow_sizes(child);
        }
    }
    fn calculate_positions(&mut self, root: ElementIdx) {
        let dir = self[root].layout_params.direction;
        let children = self[root].children.clone();
        let padding = self[root].layout_params.padding;
        let gap = self[root].layout_params.gap;
        let main_justify = self[root].layout_params.main_justify;
        let space_used = children
            .iter()
            .copied()
            .map(|child| axify(self[child].size, dir).main_axis)
            .reduce(|acc, el| acc + el)
            .unwrap_or_default();
        let space_used = space_used + gap * children.len().saturating_sub(1) as u16;
        let remaining_size = axify(self[root].size, dir)
            .shrink(padding, dir)
            .main_axis
            .saturating_sub(space_used);

        #[derive(Default)]
        struct AlignValues {
            start: u16,
            inbetween: u16,
            after: u16,
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

        let mut align = match main_justify {
            Justify::Start => AlignValues::default(),
            Justify::Center => AlignValues {
                start: remaining_size / 2,
                inbetween: 0,
                after: 0,
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
                    after: 0,
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
                    after: space,
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
                    after: space,
                    remainder: 0,
                }
            }
            Justify::End => AlignValues {
                start: remaining_size,
                inbetween: 0,
                after: 0,
                remainder: 0,
            },
        };
        for child in children.iter().copied() {
            self[child].position = self[root].position;
            match dir {
                Direction::Horizontal => self[child].position.x += align.start,
                Direction::Vertical => self[child].position.y += align.start,
            }
            self[child].position += u16vec2(padding.left, padding.top);
            align.start = increase_axis(align.start, dir, self[child].size);
            align.start += gap + align.inbetween + align.tick_rem();
            self.calculate_positions(child);
        }
    }
    pub fn calculate_layout(&mut self, element: ElementIdx) {
        self.calculate_fit_sizes(element);
        self.calculate_grow_sizes(element);
        self.calculate_positions(element);
    }
    pub fn render(&self, root: ElementIdx, area: Rect, buf: &mut Buffer) {
        let el = &self[root];
        let area = el.split_area(area);
        self.widgets[*el.widget].render_element(area, buf);
        for child in el.children.iter().copied() {
            self.render(child, area, buf);
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

#[derive(d::Deref, d::From, Clone, Copy, Debug)]
pub struct WidgetIdx(Index);
#[derive(d::Deref, d::From, Clone, Copy, Debug)]
pub struct ElementIdx(Index);

impl ElementIdx {
    pub fn children(self, ctx: &mut ElementCtx, children: &[ElementIdx]) -> Self {
        ctx[self].children = Arc::new(children.to_vec());
        self
    }
}

pub struct TuiElement {
    widget: WidgetIdx,
    layout_params: LayoutParams,
    position: U16Vec2,
    size: U16Vec2,
    // FIXME: double pointer indirection
    children: Arc<Vec<ElementIdx>>,
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

impl LayoutParams {
    fn main_size(&self, dir: Direction) -> Size {
        match dir {
            Direction::Horizontal => self.width,
            Direction::Vertical => self.height,
        }
    }
    fn cross_size(&self, dir: Direction) -> Size {
        match dir {
            Direction::Horizontal => self.height,
            Direction::Vertical => self.width,
        }
    }
}

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

impl TuiElement {
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

use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Flex;
use ratatui::layout::Layout;
use ratatui::layout::Margin;
use ratatui::layout::Offset;
use ratatui::layout::Rect;
use ratatui::prelude::Buffer;
use ratatui::style::Style;
use ratatui::text;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::LineGauge;
use ratatui::widgets::Padding;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;
use tachyonfx::Duration;
use tachyonfx::Effect;
use tachyonfx::EffectRenderer;
use tui_term::widget::PseudoTerminal;

use crate::AppBridge;
use crate::AppStage;
use crate::RendererBuildState;
use crate::RendererState;

pub struct AppFx {
    pub(crate) title_hsl_shift: Option<Effect>,
    pub(crate) text_glitch_progress: f32,
}

impl AppBridge {
    pub(crate) fn get_pty_area(&self, area: Rect) -> Rect {
        let width = area.height * self.aspect.0 / self.aspect.1;
        Layout::horizontal([Constraint::Max(width)])
            .flex(Flex::Center)
            .areas::<1>(area)[0]
    }

    /// Renders the user interface.
    ///
    /// This is where you add new widgets. See the following resources for more information:
    /// - <https://docs.rs/ratatui/latest/ratatui/widgets/index.html>
    /// - <https://github.com/ratatui/ratatui/tree/master/examples>
    pub(crate) fn draw(&self, frame: &mut Frame, state: &mut RendererState, dt: Duration) {
        let running_app = state.running_app.as_ref().map_or("", |v| v);
        let title_text = format!("running {running_app}");
        let title_len = title_text.len();
        let title_text = format!(" 📺 {} ", title_text);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().dim())
            .padding(Padding::uniform(1))
            .title_top(title_text);
        if let Some(fx) = &mut state.app_fx.title_hsl_shift {
            let [title_area] =
                Layout::new(Direction::Vertical, [Constraint::Length(1)]).areas(frame.area());
            let [_, title_area] = Layout::new(
                Direction::Horizontal,
                [
                    Constraint::Length(32 + 2),
                    Constraint::Length(title_len as u16),
                ],
            )
            .areas(title_area);
            frame.render_effect(fx, title_area, dt);
        }
        let area = self.get_pty_area(frame.area()).outer(Margin {
            horizontal: 1,
            vertical: 1,
        });
        frame.render_widget(&block, area);
        let screen_area = block.inner(area);

        let [status_corner] = Layout::vertical([Constraint::Length(5)]).areas(frame.area());
        let [_, _, status_corner] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(area.width),
            Constraint::Min(24),
        ])
        .areas(status_corner);

        match state.stage {
            AppStage::StaringIpc => {
                let loading = Paragraph::new(format!("Loading {running_app}..."))
                    .centered()
                    .style(Style::new().dim());
                let screen_area = screen_area.centered_vertically(Constraint::Length(1));
                frame.render_widget(loading, screen_area);
            }
            AppStage::Running => {
                if let Some(screen) = &state.screen {
                    let term = PseudoTerminal::new(&**screen);
                    // let term = Paragraph::new("I am terminal").centered();
                    frame.render_widget(term, screen_area);
                }
            }
            _ => {}
        }

        StatusCorner { state, dt }.render(status_corner, frame.buffer_mut());
    }
}

pub struct StatusCorner<'a> {
    state: &'a RendererState,
    dt: Duration,
}

impl<'a> Widget for StatusCorner<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if let AppStage::StaringIpc = self.state.stage {
            return;
        }
        let running_app = self.state.running_app.as_ref().map_or("", |v| v);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(match self.state.mx_menu_open {
                true => Style::new().green(),
                false => Style::new().dim(),
            })
            .title_alignment(ratatui::layout::HorizontalAlignment::Right)
            .title_top(" C-c: more ");
        let inner = block.inner(area);
        block.render(area, buf);

        let [progress_area, status_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);

        let progress = match &self.state.stage {
            AppStage::Building(RendererBuildState::Building {
                build_max_progress,
                build_progress,
            }) => LineGauge::default().ratio(*build_progress as f64 / *build_max_progress as f64),
            _ => LineGauge::default().ratio(1.0),
        };
        let progress = progress
            .filled_style(Style::new().green())
            .label("Build")
            .unfilled_style(Style::new().dim());
        if self.state.build_duration != Duration::ZERO {
            let elapsed_text = text::Span::raw(format!(
                "{:.1}s 🎉",
                self.state.build_duration.as_secs_f32()
            ))
            .into_right_aligned_line();
            let [progress_area, elapsed_area] = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Length(elapsed_text.width() as u16 + 1),
            ])
            .areas(progress_area);
            progress.render(progress_area, buf);
            elapsed_text.render(elapsed_area, buf);
        } else {
            progress.render(progress_area, buf);
        }

        let status = Paragraph::new("Status: Running 🔮".to_string()).wrap(Wrap::default());
        status.render(status_area, buf);
    }
}

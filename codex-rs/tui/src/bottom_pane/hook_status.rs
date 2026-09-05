//! Transient hook activity, independent of whether an agent turn owns a status row.

use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::motion::MotionMode;
use crate::motion::shimmer_text;
use crate::render::renderable::Renderable;
use crate::text_formatting::capitalize_first;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;

pub(super) struct HookStatus<'a> {
    pub message: &'a str,
    pub animations_enabled: bool,
}

impl Renderable for HookStatus<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut spans = vec!["• ".dim()];
        spans.extend(shimmer_text(
            &capitalize_first(self.message),
            MotionMode::from_animations_enabled(self.animations_enabled),
        ));
        truncate_line_with_ellipsis_if_overflow(Line::from(spans).dim(), usize::from(area.width))
            .render(area, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        1
    }
}

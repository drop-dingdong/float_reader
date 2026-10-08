mod backend;
mod bookmark;
pub mod constant;
mod link;
pub mod multi_image;
pub mod reader;
mod resizeable;
mod view;

use std::sync::mpsc::Receiver;

use egui::{
    Align, Align2, Color32, CornerRadius, FontSelection, Painter, Rect, Response, RichText, Sense,
    Style, Vec2, text::LayoutJob, vec2,
};

fn vec2_bigger(vec0: Vec2, vec1: Vec2) -> bool {
    vec0.x >= vec1.x && vec0.y >= vec1.y
}

fn allocate_and_fill(
    rect: Rect,
    sense: Sense,
    fill_color: Color32,
    ui: &mut egui::Ui,
    painter: &Painter,
) -> Response {
    let response = ui.allocate_rect(rect, sense);
    painter.rect_filled(rect, CornerRadius::same(0), fill_color);
    response
}
pub fn painter_richtext(
    painter: Painter,
    rect: Rect,
    rich_text: RichText,
    font: FontSelection,
    text_color: Color32,
    background_color: Color32,
) {
    painter.rect_filled(rect, CornerRadius::same(0), background_color);
    let mut layout_job = LayoutJob::default();
    rich_text.background_color(background_color).append_to(
        &mut layout_job,
        &Style::default(),
        font,
        Align::Center,
    );
    let galley = painter.layout_job(layout_job);
    let rect = Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size());
    painter.galley(rect.min, galley, text_color);
}
fn fit_vec_in_rect(vec: Vec2, rect: Rect) -> Vec2 {
    let mut x = vec.x;
    x = x.min(rect.right()).max(rect.left());
    let mut y = vec.y;
    y = y.min(rect.bottom()).max(rect.top());
    vec2(x, y)
}
fn shrink2_but_meaningful(rect: Rect, vec: Vec2) -> Rect {
    // 相较于shrink2，会保证最后的rect的意义，即不会出现过度缩小
    let center = rect.center();
    let size = (rect.size() - 2. * vec).max(Vec2::ZERO);
    Rect::from_center_size(center, size)
}
struct Line<T> {
    vec0: Vec<T>,
    vec1: Vec<T>,
}
impl<T> Line<T> {
    pub fn new() -> Self {
        Self {
            vec0: Default::default(),
            vec1: Default::default(),
        }
    }
    pub fn clear(&mut self) {
        self.vec0.clear();
        self.vec1.clear();
    }
    pub fn insert(&mut self, item: T) {
        self.vec1.push(item);
    }
    pub fn pop_head(&mut self) -> Option<T> {
        if self.vec0.is_empty() && self.vec1.is_empty() {
            None
        } else {
            if self.vec0.is_empty() {
                while let Some(item) = self.vec1.pop() {
                    self.vec0.push(item);
                }
                self.vec0.pop()
            } else {
                self.vec0.pop()
            }
        }
    }
}

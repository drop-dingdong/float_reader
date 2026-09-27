use crate::{allocate_and_fill, fit_vec_in_rect, shrink2_but_meaningful};
use egui::{Color32, CornerRadius, Rect, Sense, pos2};
pub struct ResizeableRect<'a> {
    rect: &'a mut Rect,
    edge_color: Color32,
    corn_color: Color32,
    max_rect: Rect,
    width: f32, // 这里的width应该是half_width
}
impl<'a> ResizeableRect<'a> {
    pub fn new(rect: &'a mut Rect, width: f32) -> Self {
        let edge_color = Color32::GRAY;
        let corn_color = Color32::BLACK;
        let max_rect = Rect::EVERYTHING;
        Self {
            rect,
            edge_color,
            corn_color,
            width,
            max_rect,
        }
    }
    pub fn new_with_color(
        rect: &'a mut Rect,
        width: f32,
        edge_color: Color32,
        corn_color: Color32,
    ) -> Self {
        let mut res = Self::new(rect, width);
        res.edge_color = edge_color;
        res.corn_color = corn_color;
        res
    }
    pub fn new_with_max_rect(
        rect: &'a mut Rect,
        width: f32,
        edge_color: Color32,
        corn_color: Color32,
        max_rect: Rect,
    ) -> Self {
        let mut res = Self::new_with_color(rect, width, edge_color, corn_color);
        res.max_rect = max_rect;
        res
    }
    pub fn show(&mut self, ui: &mut egui::Ui) -> bool {
        let rect = &mut self.rect;
        let mt = self.max_rect.top();
        let ml = self.max_rect.left();
        let mr = self.max_rect.right();
        let mb = self.max_rect.bottom();

        let width = self.width;

        // let inner_rect = rect.shrink(width);
        let outer_rect = rect.shrink(-width);
        let painter = ui.painter_at(outer_rect);

        // let inner_response = allocate_and_fill(
        //     inner_rect,
        //     Sense::drag(),
        //     Color32::TRANSPARENT,
        //     ui,
        //     &painter,
        // );
        // **rect = rect.translate(inner_response.drag_delta());

        let mut l = rect.left();
        let mut r = rect.right();
        let mut t = rect.top();
        let mut b = rect.bottom();
        let size = rect.size();

        let top_rect = Rect::from_min_max(pos2(l + width, t - width), pos2(r - width, t + width));
        let left_rect = Rect::from_min_max(pos2(l - width, t + width), pos2(l + width, b - width));
        let right_rect = Rect::from_min_max(pos2(r - width, t + width), pos2(r + width, b - width));
        let bottom_rect =
            Rect::from_min_max(pos2(l + width, b - width), pos2(r - width, b + width));

        let lt_rect = Rect::from_pos(pos2(l, t)).shrink(-width);
        let rt_rect = Rect::from_pos(pos2(r, t)).shrink(-width);
        let lb_rect = Rect::from_pos(pos2(l, b)).shrink(-width);
        let rb_rect = Rect::from_pos(pos2(r, b)).shrink(-width);

        let top_response =
            allocate_and_fill(top_rect, Sense::drag(), self.edge_color, ui, &painter);
        if top_response.hovered() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::ResizeNorth;
            });
        }
        t += top_response.drag_delta().y;
        let left_response =
            allocate_and_fill(left_rect, Sense::drag(), self.edge_color, ui, &painter);
        if left_response.hovered() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::ResizeWest;
            });
        }
        l += left_response.drag_delta().x;
        let right_response =
            allocate_and_fill(right_rect, Sense::drag(), self.edge_color, ui, &painter);
        if right_response.hovered() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::ResizeEast;
            });
        }
        r += right_response.drag_delta().x;
        let bottom_response =
            allocate_and_fill(bottom_rect, Sense::drag(), self.edge_color, ui, &painter);
        if bottom_response.hovered() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::ResizeSouth;
            });
        }
        b += bottom_response.drag_delta().y;

        let lt_response = allocate_and_fill(lt_rect, Sense::drag(), self.corn_color, ui, &painter);
        let rt_response = allocate_and_fill(rt_rect, Sense::drag(), self.corn_color, ui, &painter);
        let lb_response = allocate_and_fill(lb_rect, Sense::drag(), self.corn_color, ui, &painter);
        let rb_response = allocate_and_fill(rb_rect, Sense::drag(), self.corn_color, ui, &painter);

        if lt_response.hovered() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::ResizeNorthWest;
            });
        }
        if rt_response.hovered() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::ResizeNorthEast;
            });
        }
        if lb_response.hovered() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::ResizeSouthWest;
            });
        }
        if rb_response.hovered() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::ResizeSouthEast;
            });
        }

        let lt_delta = lt_response.drag_delta();
        let rt_delta = rt_response.drag_delta();
        let lb_delta = lb_response.drag_delta();
        let rb_delta = rb_response.drag_delta();

        l += lt_delta.x + lb_delta.x;
        r += rt_delta.x + rb_delta.x;
        t += lt_delta.y + rt_delta.y;
        b += lb_delta.y + rb_delta.y;

        l = l.min(r);
        r = l.max(r);
        t = t.min(b);
        b = t.max(b);

        l = l.max(ml);
        r = r.min(mr);
        t = t.max(mt);
        b = b.min(mb);

        rect.set_top(t);
        rect.set_left(l);
        rect.set_right(r);
        rect.set_bottom(b);
        return size != rect.size();
    }
}
#[derive(Debug)]
// 假定outer_rect 包含 select_rect
pub struct SelectZone {
    outer_rect: Rect,
    select_rect: Rect,
    select_rect_cache: Rect,
}
impl SelectZone {
    pub fn new(outer_rect: Rect, select_rect: Rect) -> SelectZone {
        SelectZone {
            outer_rect,
            select_rect,
            select_rect_cache: select_rect,
        }
    }
    pub fn return_rect(self) -> (Rect, Rect) {
        (self.outer_rect, self.select_rect)
    }
    pub fn is_changed(&self) -> bool {
        self.select_rect_cache != self.select_rect
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        // 绘制一个扣去中间矩形的灰色区域，
        // 是不是利用stroke更好一些？
        // 存在一部分理应和reader.rs disp_image_in_rect_with_resized类似的代码段，但是没想好该如抽象
        let outer_rect = self.outer_rect;
        let select_rect = self.select_rect;

        let o_lt = outer_rect.left_top();
        let o_rb = outer_rect.right_bottom();

        let i_t = select_rect.top();
        let i_b = select_rect.bottom();

        let ol_it = pos2(o_lt.x, i_t);
        let ol_ib = pos2(o_lt.x, i_b);
        let or_it = pos2(o_rb.x, i_t);
        let or_ib = pos2(o_rb.x, i_b);

        let i_lb = select_rect.left_bottom();
        let i_rt = select_rect.right_top();

        let rect_top = Rect::from_min_max(o_lt, or_it);
        let rect_left = Rect::from_min_max(ol_it, i_lb);
        let rect_right = Rect::from_min_max(i_rt, or_ib);
        let rect_bottom = Rect::from_min_max(ol_ib, o_rb);

        ui.allocate_rect(outer_rect, Sense::click_and_drag());
        let painter = ui.painter_at(outer_rect);
        painter.rect_filled(
            rect_top,
            CornerRadius::same(0),
            Color32::from_black_alpha(128),
        );
        painter.rect_filled(
            rect_left,
            CornerRadius::same(0),
            Color32::from_black_alpha(128),
        );
        painter.rect_filled(
            rect_right,
            CornerRadius::same(0),
            Color32::from_black_alpha(128),
        );
        painter.rect_filled(
            rect_bottom,
            CornerRadius::same(0),
            Color32::from_black_alpha(128),
        );

        let size = select_rect.size();
        let center = select_rect.center();
        let allowed_rect =
            shrink2_but_meaningful(outer_rect, (size / 2.)).translate(-center.to_vec2());

        let response = allocate_and_fill(
            select_rect.shrink(2.),
            Sense::drag(),
            Color32::TRANSPARENT,
            ui,
            &painter,
        );
        if response.hovered() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::Grab;
            });
        } else if response.dragged() {
            ui.output_mut(|o| {
                o.cursor_icon = egui::CursorIcon::Grabbing;
            });
        }
        self.select_rect =
            select_rect.translate(fit_vec_in_rect(response.drag_delta(), allowed_rect));
        // 中间的可调整区域
        let mut resized_rect = ResizeableRect::new(&mut self.select_rect, 2.);
        resized_rect.show(ui);
    }
}

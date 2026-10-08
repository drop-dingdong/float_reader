use egui::{Color32, CornerRadius, Image, Pos2, Rect, TextureHandle, Vec2, vec2};

use crate::constant::*;
use crate::{fit_vec_in_rect, shrink2_but_meaningful};

#[derive(Debug)]
pub enum ImageShow<'a> {
    // 因为存在一些条件下对应的图片并没有完全加载，所以以I 和 C来区分两种状况，在未加载或者未有替代texture的情况下，使用color32来替代图片填充对应的区域，当前前提是需要知道图片的具体大小
    I(Image<'a>),
    C(Color32),
}
impl<'a> ImageShow<'a> {
    pub fn update_with_texture(&mut self, texture: &TextureHandle, img_uv: Rect) {
        let img = Image::from_texture(texture).uv(img_uv);
        *self = ImageShow::I(img);
    }
    pub fn paint_at(&self, ui: &mut egui::Ui, rect: Rect) {
        match self {
            ImageShow::I(img) => img.paint_at(ui, rect),
            ImageShow::C(color) => {
                ui.painter_at(rect)
                    .rect_filled(rect, CornerRadius::same(0), *color);
            }
        }
    }
}
pub struct MultiImage<'a> {
    pub images: Vec<(ImageShow<'a>, Rect)>, //image, uv
    pub disp_rect: Rect,
}

impl<'a> MultiImage<'a> {
    pub fn show(&self, ui: &mut egui::Ui) {
        let disp_size = self.disp_rect.size();
        let lt = self.disp_rect.left_top();
        for (img_show, uv) in self.images.iter() {
            let uv_size = uv.size();
            let uv_lt = uv.left_top();
            let rect = Rect::from_min_size(lt + uv_lt.to_vec2() * disp_size, uv_size * disp_size);
            img_show.paint_at(ui, rect);
        }
    }
}
pub trait GetImgInfo {
    fn get_img_number(&self) -> usize;
    fn get_min_outer_size(&self) -> Vec2;
    fn get_img_size(&self, pix: usize) -> Vec2;
}
// 图片矩阵的游标系统
pub struct Images2DCursor {
    direc: bool,        // 序号连续方向，其中true 连续方向为横向，否则为纵向；先按照true来设计
    line_num: usize,    // 序号连续方向允许排列的最大数目。必须不大于图片的总数img_num
    row_num: usize,     // 行的个数
    allowed_rect: Rect, // 中心可允许移动的rect，默认以outer_rect的左上角为原点
    outer_rect: Rect, //  包围该image_matrix的最小矩形，图片周围至少包含vhspace / disp_scale宽度的空白，outer_rect左上角为原点
    pub view_center: Vec2, // 视野中心 相对于outer_rect左上角的矢量。
    view_rect: Rect,  // 可视大小，尺寸单独意义，位置传递外部位置信息。egui尺寸
    vhspace: f32,     //不同图片之间最小浏览间隔，也就是显示的间隔并没有变化，egui尺寸
    pub disp_scale: f32, // egui尺寸和图片尺寸之间的比例，越大显示上表现为放大的效果
    item_size: Vec2, // 图片的最小可容纳尺寸，图片实际尺寸。也因此如果存在某些图片尺寸比较奇怪的话，图片间空格区域会比较大
    img_num: usize,
}
impl Images2DCursor {
    pub fn new_with_center_pos(
        line_num: usize,
        disp_scale: f32,
        view_rect: Rect,
        image_array: &impl GetImgInfo,
        center_pos: Vec2,
    ) -> Self {
        // 一定产生一个符合需求且正确的游标
        let mut res = Self::new(line_num, disp_scale, view_rect, image_array);
        let vec = fit_vec_in_rect(center_pos, res.allowed_rect);
        res.view_center = vec;
        res
    }
    fn new(
        line_num: usize,
        disp_scale: f32,
        view_rect: Rect,
        image_array: &impl GetImgInfo,
    ) -> Self {
        let img_num = image_array.get_img_number();
        let line_num = line_num.min(img_num);
        let row_num = img_num / line_num + if img_num % line_num == 0 { 0 } else { 1 };
        let item_size = image_array.get_min_outer_size();
        let vhspace = 20.;
        let vh = vhspace / disp_scale;
        let unit = item_size + Vec2::splat(vh);
        let outer_rect =
            Rect::from_min_size(Pos2::ZERO, (vec2(line_num as f32, row_num as f32) * unit));
        let read_view_size = view_rect.size() / disp_scale;
        let allowed_rect = shrink2_but_meaningful(outer_rect, read_view_size / 2.);

        let init_center = allowed_rect.left_top();
        let view_center = init_center.to_vec2();

        Self {
            direc: false,
            line_num,
            allowed_rect,
            outer_rect,
            view_center,
            view_rect,
            vhspace,
            disp_scale,
            item_size,
            img_num,
            row_num,
        }
    }
    pub fn get_multi_image<'b, T: GetImgInfo>(
        &self,
        image_array: &T,
    ) -> (Vec<(usize, Rect, Rect)>) {
        let vhspace = self.vhspace / self.disp_scale;
        let img_num = self.img_num;
        let line_num = self.line_num;
        let max_line = self.row_num;
        let unit = self.item_size + Vec2::splat(vhspace);
        let unit_y = unit.y;
        let unit_x = unit.x;
        let view_real_size = self.view_rect.size() / self.disp_scale;
        let rel_vec = self.view_center;
        let disp_rect = Rect::from_center_size(rel_vec.to_pos2(), view_real_size);
        // 需要注意这里并不保证disp_rect存在于self.outer_rect之内，因为outer_rect是图片的外包，那么如果图片排成单行，同时scale小到可以看见很多外部区域，disp_rect必定大于outer_rect
        let top_line = ((disp_rect.top().max(0.) / unit_y).floor() as usize).min(max_line);
        let bottom_line = ((disp_rect.bottom().max(0.) / unit_y).ceil() as usize).min(max_line);
        let left_vertial = ((disp_rect.left().max(0.) / unit_x).floor() as usize).min(line_num);
        let right_vertial = ((disp_rect.right().max(0.) / unit_x).ceil() as usize).min(line_num);
        // let mut img_slice_array =
        //     Vec::with_capacity((bottom_line - top_line) * (right_vertial - left_vertial));
        let vertial = bottom_line - top_line;
        let horizontal = right_vertial - left_vertial;
        let mut related_info = Vec::with_capacity(vertial * horizontal);
        for y_j in top_line..bottom_line {
            for x_i in left_vertial..right_vertial {
                let idx = y_j * line_num + x_i;
                if idx >= img_num {
                    continue;
                }
                let center_ij = (vec2(x_i as f32, y_j as f32) + Vec2::splat(0.5)) * unit;
                let rect_ij =
                    Rect::from_center_size(center_ij.to_pos2(), image_array.get_img_size(idx));
                let rect_intersert = rect_ij.intersect(disp_rect);
                if rect_intersert.area() >= 1e-2 {
                    let img_uv = part2uv(rect_intersert, rect_ij);
                    let pos_uv = part2uv(rect_intersert, disp_rect);
                    related_info.push((idx, img_uv, pos_uv));
                }
            }
        }
        related_info
    }
    pub fn view_center_img_pos(&self, image_array: &impl GetImgInfo) -> (usize, Vec2) {
        let vhspace = self.vhspace / self.disp_scale;
        let unit = self.item_size + Vec2::splat(vhspace);
        let xy_unit = self.view_center / unit;
        let x_i = xy_unit.x.floor();
        let y_j = xy_unit.y.floor();
        let idx = y_j as usize * self.line_num + x_i as usize; // 会出现idx超过上限的状况，如果超过number上限，那么使用最大大小作为img_size
        let rel_pos = if idx < self.img_num {
            vec2(xy_unit.x - x_i - 0.5, xy_unit.y - y_j - 0.5) * unit
                / image_array.get_img_size(idx)
        } else {
            // 如果2＊2f放置三幅图片，那么如果将视野中心移到第四张图片时，会出现这种情况
            vec2(xy_unit.x - x_i - 0.5, xy_unit.y - y_j - 0.5)
        };
        // let rel_pos = vec2(xy_unit.x - x_i - 0.5, xy_unit.y - y_j - 0.5); // 相对于item的中心的位移矢量
        (idx, rel_pos)
    }
    pub fn jump_to_special_img_pos(
        &mut self,
        idx: usize,
        vec: Vec2,
        image_array: &impl GetImgInfo,
    ) {
        // vec 为相对于中心的位移矢量，单位为img_size
        // 不允许通过跳动的方式移到outer_rect之外的地方。
        let capacity = self.line_num * self.row_num;
        if idx >= capacity {
            panic!(
                "不要跳到任意地方，当前目的地 {}，超过outer_rect的容量{}",
                idx, capacity
            )
        }
        let vec = vec
            * if idx >= self.img_num {
                self.item_size
            } else {
                image_array.get_img_size(idx)
            };
        let vhspace = self.vhspace / self.disp_scale;
        let unit = self.item_size + Vec2::splat(vhspace);
        let x_i = idx % self.line_num;
        let y_j = idx / self.line_num;
        let view_center = (vec2(x_i as f32, y_j as f32) + Vec2::splat(0.5)) * unit + vec;
        self.view_center = fit_vec_in_rect(view_center, self.allowed_rect);
    }
    // 尽可能保持视觉中心依旧在中心
    pub fn update_rescale(&mut self, new_scale: f32, img_array: &impl GetImgInfo) {
        let view_center_img = self.view_center_img_pos(img_array);

        let new_vhspace = self.vhspace / new_scale;
        let new_unit = self.item_size + Vec2::splat(new_vhspace);
        // self.outer_rect = Rect::from_min_size(Pos2::ZERO, number * new_unit);
        self.outer_rect = Rect::from_min_size(
            Pos2::ZERO,
            (vec2(self.line_num as f32, self.row_num as f32) * new_unit),
        );

        let view_size = self.view_rect.size();
        let real_view_size = view_size / new_scale;
        self.allowed_rect = shrink2_but_meaningful(self.outer_rect, real_view_size / 2.);
        // self.allowed_rect = self.outer_rect.shrink2(real_view_size / 2.); // 是否会出现意外呢？
        self.disp_scale = new_scale;
        self.jump_to_special_img_pos(view_center_img.0, view_center_img.1, img_array);
    }
    pub fn move_center(&mut self, vec: Vec2) {
        // vec为egui坐标
        let real_dist = vec / self.disp_scale;
        // let vhspace = self.vhspace / self.disp_scale;
        let mut center_pos = self.view_center;
        center_pos += real_dist;
        center_pos = fit_vec_in_rect(center_pos, self.allowed_rect);
        self.view_center = center_pos;
    }
}

fn part2uv(part_rect: Rect, rect: Rect) -> Rect {
    // 假定part_rect全部在rect中，返回part_rect在rect中的uv坐标，
    // 当然不满足上述假定也可，最后返回的应该是相交区域在后者中的uv坐标
    let total_size = rect.size();
    let lt = (part_rect.left_top() - rect.left_top()) / total_size;
    let size = part_rect.size() / total_size;
    let rb = lt + size;
    let lt = lt.min(REL_VEC2_MAX).max(REL_VEC2_MIN);
    let rb = rb.min(REL_VEC2_MAX).max(REL_VEC2_MIN);
    Rect::from_min_max(lt.to_pos2(), rb.to_pos2())
}

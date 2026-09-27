use crate::multi_image::{GetImgInfo, Images2DCursor, MultiImage};

use egui::{ColorImage, Image, Rect, TextureHandle, Vec2, pos2, vec2};
use egui_extras::Size::Relative;
use std::collections::HashMap;
use std::mem;
use std::rc::Rc;

use pdfium_render::prelude::{PdfDocument, PdfPage};

#[derive(Clone)]
struct PageImage {
    page_idx: usize,
    page_size: Vec2,
    scale: f32,
    bit_image: TextureHandle,
}
#[derive(Debug, Clone)]
pub enum Jump {
    PagePos(Vec2), // 相对于中心的图片的uv矢量
    Header,
    Footer,
}
#[derive(Debug, Default, Clone)]
pub enum ViewAction {
    ReSize(Vec2), //改变视图的大小
    Rescale(f32),
    Shift(Vec2), // egui坐标的位移矢量
    Jump(usize, Jump),
    Col(usize), // 改变浏览视图中columns的数目
    #[default]
    Empty,
}
impl ViewAction {
    pub fn take(&mut self) -> Self {
        mem::take(self)
    }
    pub fn is_empty(&self) -> bool {
        match self {
            ViewAction::Empty => true,
            _ => false,
        }
    }
    pub fn or(self, other: Self) -> Self {
        match (self, other) {
            (ViewAction::Empty, f) => f,
            (f, _) => f,
        }
    }
}
pub struct View<'a> {
    document: PdfDocument<'a>,
    pub page_num: usize,
    pub line_num: usize,
    view: Option<MultiImage<'a>>,
    min_outer_size: Vec2,
    load_hash: Rc<HashMap<usize, PageImage>>,
    pub disp_rect: Rect,
    pub disp_scale: f32,
    max_slide: f32,
    scale_low_bound: f32,
    center_pos: Vec2,
    reloaded: bool,
    pub page_info: (usize, Vec2),
    pub view_page_info: Vec<(usize, Rect, Rect)>,
}
impl<'a> View<'a> {
    pub fn new(document: PdfDocument<'a>, disp_rect: Rect, disp_scale: f32) -> Self {
        let mut vec = Vec2::ZERO;
        let pages = document.pages();
        let page_num = pages.len();
        for idx in 0..page_num {
            let rect = pages.get(idx).unwrap().page_size();
            let width = rect.width().value;
            let height = rect.height().value;
            vec = vec.max(vec2(width, height));
        }

        Self {
            document,
            page_num: page_num as usize,
            line_num: 2,
            view: None,
            min_outer_size: vec,
            load_hash: Rc::new(HashMap::new()),
            disp_rect,
            disp_scale,
            reloaded: false,
            center_pos: Vec2::ZERO,
            page_info: (0, Vec2::ZERO),
            max_slide: 2048.,
            scale_low_bound: 0.5,
            view_page_info: Vec::new(),
        }
    }
    pub fn max_page_index(&self) -> usize {
        self.page_num.max(1) - 1
    }
    fn get_multi_image(
        &mut self,
        related_info: &Vec<(usize, Rect, Rect)>,
        ctx: &egui::Context,
    ) -> MultiImage<'a> {
        let mut img_array = Vec::with_capacity(related_info.len());
        for (pidx, img_uv, pos_uv) in related_info.iter() {
            let img = Image::from_texture(self.get_img_with_special_scale_or_load(
                *pidx,
                self.disp_scale,
                ctx,
            ))
            .uv(*img_uv);
            img_array.push((img, *pos_uv));
        }
        MultiImage {
            images: img_array,
            disp_rect: self.disp_rect,
        }
    }
    fn get_image_matrix(&self, ppp: f32) -> Images2DCursor {
        Images2DCursor::new_with_center_pos(
            self.line_num,
            self.disp_scale * ppp,
            self.disp_rect,
            self,
            self.center_pos,
        )
    }
    fn get_view_info(&mut self, image_matrix: Images2DCursor, ctx: &egui::Context) {
        // 更新 center_pos, page_info, view_page_info
        // 并将image_matrix替代现有的view
        self.center_pos = image_matrix.view_center;
        let related_info = image_matrix.get_multi_image(self, ctx);
        self.page_info = image_matrix.view_center_img_pos(self);
        let multi_image = self.get_multi_image(&related_info, ctx);
        self.view_page_info = related_info;
        self.view = Some(multi_image);
    }
    pub fn show_with_action(&mut self, action: ViewAction, ui: &mut egui::Ui) -> bool {
        let ppp = ui.pixels_per_point();
        let ctx = ui.ctx();
        match action {
            ViewAction::ReSize(disp_size) => {
                let (page_idx, vec) = self.page_info;
                self.disp_rect.max = self.disp_rect.min + disp_size;
                let mut image_matrix = self.get_image_matrix(ppp);
                image_matrix.jump_to_special_img_pos(page_idx, vec, self);
                self.get_view_info(image_matrix, ctx);
            }
            ViewAction::Col(cols) if cols.min(self.page_num) != self.line_num => {
                let new_line_num = cols.min(self.page_num);
                let (page_idx, vec) = self.page_info;
                self.line_num = new_line_num;
                let mut image_matrix = self.get_image_matrix(ppp);
                image_matrix.jump_to_special_img_pos(page_idx, vec, self);
                self.get_view_info(image_matrix, ctx);
            }
            ViewAction::Jump(pidx, jump) if pidx < self.page_num => {
                let mut image_matrix = self.get_image_matrix(ppp);
                match jump {
                    Jump::PagePos(dist) => image_matrix.jump_to_special_img_pos(pidx, dist, self),
                    Jump::Footer => {
                        // 目标让至少左下角出现在屏幕中，
                        image_matrix.jump_to_special_img_pos(pidx, Vec2::ZERO, self);
                        let related_info = image_matrix.get_multi_image(self, ctx);
                        match related_info
                            .iter()
                            .position(|(page_idx, _, _)| *page_idx == pidx)
                        {
                            None => panic!("如果设计正常，那么一定存在一个等于pidx的page index"),
                            Some(idx) => {
                                let (_, img_uv, pos_uv) = &related_info[idx];
                                let aim_img_uv = pos2(0., 1.);
                                if !img_uv.contains(aim_img_uv) {
                                    let ratio = pos_uv.size() / img_uv.size();
                                    let lb_uv_dist = aim_img_uv - img_uv.left_bottom();
                                    let dist = lb_uv_dist * ratio * self.get_img_size(pidx);
                                    image_matrix.move_center(dist);
                                }
                            }
                        };
                    }
                    Jump::Header => {
                        // 目标让至少左上角出现在屏幕中，
                        image_matrix.jump_to_special_img_pos(pidx, Vec2::ZERO, self);
                        let related_info = image_matrix.get_multi_image(self, ctx);
                        match related_info
                            .iter()
                            .position(|(page_idx, _, _)| *page_idx == pidx)
                        {
                            None => panic!("如果设计正常，那么一定存在一个等于pidx的page index"),
                            Some(idx) => {
                                let (_, img_uv, pos_uv) = &related_info[idx];
                                let aim_img_uv = pos2(0., 0.);
                                if !img_uv.contains(aim_img_uv) {
                                    let ratio = pos_uv.size() / img_uv.size();
                                    let lt_uv_dist = aim_img_uv - img_uv.left_top();
                                    let dist = lt_uv_dist * ratio * self.get_img_size(pidx);
                                    image_matrix.move_center(dist);
                                }
                            }
                        };
                    }
                }
                self.get_view_info(image_matrix, ctx);
            }
            ViewAction::Rescale(scale) => {
                let long_slide = (self.min_outer_size.x).max(self.min_outer_size.y);
                let disp_scale_upper_bound = self.max_slide / long_slide / ppp;
                let scale = scale.min(disp_scale_upper_bound);
                if self.disp_scale != scale {
                    let mut image_matrix = self.get_image_matrix(ppp);
                    image_matrix.update_rescale(scale * ppp, self);
                    self.disp_scale = scale;
                    self.get_view_info(image_matrix, ctx);
                }
            }
            ViewAction::Shift(dist) => {
                let mut image_matrix = self.get_image_matrix(ppp);
                image_matrix.move_center(dist);
                self.get_view_info(image_matrix, ctx);
            }
            _ => (),
        }
        match &self.view {
            None => {
                let image_matrix = self.get_image_matrix(ppp);
                self.get_view_info(image_matrix, ctx);
                self.view.as_mut().unwrap().show(ui);
            }
            Some(view) => view.show(ui),
        }
        let reloaded = self.reloaded;
        self.reloaded = false;
        reloaded
    }
    pub fn required_clip_content<'b>(
        &mut self,
        page_idx: usize,
        content_rect: (Vec2, Vec2),
        disp_size: Vec2,
        ctx: &egui::Context,
    ) -> (Vec2, Image<'b>) {
        let texture_handle =
            self.get_img_with_special_scale_or_load(page_idx, self.disp_scale, ctx);
        let (min, max) = content_rect;
        let uv = Rect::from_min_max(min.to_pos2(), max.to_pos2());
        (disp_size, Image::from_texture(texture_handle).uv(uv))
    }
}
impl<'a> GetImgInfo for View<'a> {
    type Loader = egui::Context;
    fn get_img_number(self: &Self) -> usize {
        self.page_num
    }
    fn get_min_outer_size(self: &Self) -> Vec2 {
        self.min_outer_size
    }
    fn get_img_size(self: &Self, pix: usize) -> Vec2 {
        match self.load_hash.get(&pix) {
            Some(page_img) => page_img.page_size,
            None => {
                let rect = self.document.pages().get(pix as i32).unwrap().page_size();
                let width = rect.width().value;
                let height = rect.height().value;
                vec2(width, height)
            }
        }
    }
    fn get_img_with_special_scale_or_load(
        self: &mut Self,
        pidx: usize,
        disp_scale: f32, // self.disp_scale
        loader: &Self::Loader,
    ) -> &egui::TextureHandle {
        let long_slide = (self.min_outer_size.x).max(self.min_outer_size.y);
        let ppp = loader.pixels_per_point();
        let scale_upper_bound = self.max_slide / long_slide / ppp;
        let disp_scale = disp_scale.min(scale_upper_bound);
        &Rc::get_mut(&mut self.load_hash)
            .unwrap()
            .entry(pidx)
            .and_modify(|page_img| {
                if is_scale_reload(page_img.scale, disp_scale, self.scale_low_bound) {
                    self.reloaded = true;
                    let page = self.document.pages().get(pidx as i32).unwrap();
                    let load_scale = (disp_scale * 2.)
                        .max(self.scale_low_bound)
                        .min(scale_upper_bound);
                    let (page_size, texture_handle) =
                        load_page_image_with_special_scale(page, load_scale * ppp, loader);
                    *page_img = PageImage {
                        page_idx: pidx,
                        page_size,
                        bit_image: texture_handle,
                        scale: load_scale,
                    };
                }
            })
            .or_insert_with(|| {
                let page = self.document.pages().get(pidx as i32).unwrap();
                let load_scale = (disp_scale * 2.)
                    .max(self.scale_low_bound)
                    .min(scale_upper_bound);

                let (page_size, texture_handle) =
                    load_page_image_with_special_scale(page, load_scale * ppp, loader);
                PageImage {
                    page_idx: pidx,
                    page_size,
                    bit_image: texture_handle,
                    scale: load_scale,
                }
            })
            .bit_image
    }
}

fn is_scale_reload(scale: f32, disp_scale: f32, scale_lower_bound: f32) -> bool {
    (disp_scale > scale || (disp_scale * 2. < scale && disp_scale < scale_lower_bound))
        && scale != disp_scale
}
fn load_page_image_with_special_scale(
    page: PdfPage<'_>,
    load_ppp_scale: f32,
    ctx: &egui::Context,
) -> (Vec2, TextureHandle) {
    // 其中load_ppp_scale 指的是load_scale * pixels_per_pointer
    let page_width = page.page_size().width().value;
    let page_height = page.page_size().height().value;
    let page_size = vec2(page_width, page_height);
    let width = (page_size.x * load_ppp_scale).floor() as i32;
    let height = (page_size.y * load_ppp_scale).floor() as i32;
    let rgba_image = page.render(width, height, None).unwrap().as_rgba_bytes();
    let texture_handle = ctx.load_texture(
        "",
        ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            rgba_image.as_slice(),
        ),
        Default::default(),
    );
    (vec2(page_width, page_height), texture_handle)
}

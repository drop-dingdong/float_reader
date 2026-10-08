use crate::Line;
use crate::multi_image::{GetImgInfo, ImageShow, Images2DCursor, MultiImage};

use egui::{Color32, Image, Rect, TextureHandle, Vec2, pos2};
use std::collections::HashMap;
use std::rc::Rc;
use std::{fmt, mem};

#[derive(Debug, Clone, PartialEq, Copy)]
pub struct PageRenderInfo {
    pub page_idx: usize,
    pub page_size: Vec2,
    pub scale: f32,
}
impl Default for PageRenderInfo {
    fn default() -> Self {
        PageRenderInfo {
            page_idx: 0,
            page_size: Vec2::ZERO,
            scale: 0.,
        }
    }
}
#[derive(Clone)]
struct PageImage {
    rend_info: PageRenderInfo,
    bit_image: TextureHandle,
}
#[derive(Clone)]
pub enum RenderPage {
    First(PageRenderInfo),
    Stored(PageImage),
    OtherRender(PageRenderInfo, PageImage),
}
impl fmt::Debug for RenderPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = f.debug_struct("page render situation");
        match self {
            RenderPage::First(info) => res
                .field("first render", &())
                .field("render info", &info)
                .finish(),
            RenderPage::OtherRender(info, _) => res
                .field("try other render", &())
                .field("render info", &info)
                .finish(),
            RenderPage::Stored(img) => res
                .field("render finished", &())
                .field("render info", &img.rend_info)
                .finish(),
        }
    }
}
impl Default for RenderPage {
    fn default() -> Self {
        RenderPage::First(Default::default())
    }
}
impl RenderPage {
    fn scale(&self) -> f32 {
        match self {
            RenderPage::First(info) | RenderPage::OtherRender(info, _) => info.scale,
            RenderPage::Stored(page_img) => page_img.rend_info.scale,
        }
    }
    fn take(&mut self) -> Self {
        mem::take(self)
    }
    fn new(first_render: PageRenderInfo) -> Self {
        RenderPage::First(first_render)
    }
    pub fn updata_with_texture(
        &mut self,
        texture_handle: TextureHandle,
        render_info: PageRenderInfo,
    ) {
        match self {
            RenderPage::First(info) | RenderPage::OtherRender(info, _) if *info == render_info => {
                *self = RenderPage::Stored(PageImage {
                    rend_info: *info,
                    bit_image: texture_handle,
                });
            }
            RenderPage::Stored(_) => (),
            RenderPage::First(info) | RenderPage::OtherRender(info, _) => {
                *self = {
                    RenderPage::OtherRender(
                        *info,
                        PageImage {
                            rend_info: render_info,
                            bit_image: texture_handle,
                        },
                    )
                }
            }
        }
    }
    fn get_texture(&self) -> Option<&TextureHandle> {
        match self {
            RenderPage::First(_) => None,
            RenderPage::OtherRender(_, page_img) => Some(&page_img.bit_image),
            RenderPage::Stored(page_img) => Some(&page_img.bit_image),
        }
    }
    fn new_info(&mut self, render_info: PageRenderInfo) -> Option<(usize, f32)> {
        // 简而言之，所有状态一律保留最新信息
        let new_required = Some((render_info.page_idx, render_info.scale));
        match self.take() {
            RenderPage::First(info) => {
                if info == render_info {
                    *self = RenderPage::First(info);
                    None
                } else {
                    *self = RenderPage::First(render_info);
                    new_required
                }
            }
            RenderPage::Stored(page_img) => {
                if page_img.rend_info == render_info {
                    *self = RenderPage::Stored(page_img);
                    None
                } else {
                    *self = RenderPage::OtherRender(render_info, page_img);
                    new_required
                }
            }
            RenderPage::OtherRender(info, page_img) => {
                if page_img.rend_info == render_info {
                    *self = RenderPage::OtherRender(info, page_img);
                    None
                } else {
                    *self = RenderPage::OtherRender(render_info, page_img);
                    new_required
                }
            }
        }
    }
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

pub struct PagesInfoRc {
    pub page_num: usize,
    min_outer_size: Vec2,
    page_size_vec: Rc<Vec<Vec2>>,
}
// 一个cursor的包装器
pub struct View {
    // document: PdfDocument<'a>,
    pub all_page_size_info: PagesInfoRc,
    pub line_num: usize,
    // view: Option<MultiImage<'a>>, // 不再保留view，将view发送到上一级
    // load_hash: HashMap<usize, PageImage>,// 不再保留hash，将hash返回上一级
    pub disp_rect: Rect,
    pub disp_scale: f32,
    max_slide: f32,
    scale_low_bound: f32,
    center_pos: Vec2,
    pub view_center_info: (bool, (usize, Vec2)), // view_center_info.1指代用户看到的一个页标，不一定和cursor实际对应的页标相同。
    pub view_page_info: (usize, Vec<(usize, Rect, Rect)>), // 现在不再利用reloaded表征是否发生了变化，而是由第一位来表征发生变化与否
    pixels_per_point: f32,
    cursor: Images2DCursor,
}
impl View {
    pub fn new(
        page_num: usize,
        min_outer_size: Vec2,
        page_size_vec: Rc<Vec<Vec2>>,
        disp_rect: Rect,
        disp_scale: f32,
        ppp: f32,
    ) -> Self {
        let all_page_size_info = PagesInfoRc {
            page_num,
            min_outer_size,
            page_size_vec,
        };
        let center_pos = Vec2::ZERO;
        let cursor = Images2DCursor::new_with_center_pos(
            2,
            disp_scale * ppp,
            disp_rect,
            &all_page_size_info,
            center_pos,
        );
        let view_center_info = cursor.view_center_img_pos(&all_page_size_info);
        Self {
            all_page_size_info,
            line_num: 2,
            disp_rect,
            disp_scale,
            center_pos,
            view_center_info: (true, view_center_info),
            max_slide: 2048.,
            scale_low_bound: 0.5,
            view_page_info: (0, Vec::new()),
            cursor,
            pixels_per_point: ppp,
        }
    }
    pub fn max_page_index(&self) -> usize {
        self.all_page_size_info.page_num.max(1) - 1
    }
    fn update_img_cursor(&mut self) {
        self.cursor = Images2DCursor::new_with_center_pos(
            self.line_num,
            self.disp_scale * self.pixels_per_point,
            self.disp_rect,
            &self.all_page_size_info,
            self.center_pos,
        );
    }
    fn get_true_view_center(&mut self) -> (usize, Vec2) {
        if self.view_center_info.0 {
            self.view_center_info.1
        } else {
            self.cursor.view_center_img_pos(&self.all_page_size_info)
        }
    }
    fn update_view_center(&mut self) {
        if self.view_center_info.0 {
            ()
        } else {
            self.view_center_info = (
                true,
                self.cursor.view_center_img_pos(&self.all_page_size_info),
            );
        }
    }
    fn update_view_info(&mut self) {
        // 更新 center_pos, view_center_info.0, view_page_info
        // 并将image_matrix替代现有的view
        self.center_pos = self.cursor.view_center;
        let related_info = self.cursor.get_multi_image(&self.all_page_size_info);
        self.view_page_info = (self.view_page_info.0 + 1, related_info);
        self.view_center_info.0 = false;
    }
    pub fn update_img_cursor_with_action(&mut self, action: ViewAction) {
        match action {
            ViewAction::ReSize(disp_size) => {
                let (page_idx, vec) = self.view_center_info.1;
                self.disp_rect.max = self.disp_rect.min + disp_size;
                self.update_img_cursor();
                self.cursor
                    .jump_to_special_img_pos(page_idx, vec, &self.all_page_size_info);
                self.update_view_info();
            }
            ViewAction::Col(cols)
                if cols.min(self.all_page_size_info.page_num) != self.line_num =>
            {
                let new_line_num = cols.min(self.all_page_size_info.page_num);
                let (page_idx, vec) = self.view_center_info.1;
                self.line_num = new_line_num;
                self.update_img_cursor();
                self.cursor
                    .jump_to_special_img_pos(page_idx, vec, &self.all_page_size_info);
                self.update_view_info();
            }
            ViewAction::Jump(pidx, jump) if pidx < self.all_page_size_info.page_num => {
                match jump {
                    Jump::PagePos(dist) => {
                        self.cursor
                            .jump_to_special_img_pos(pidx, dist, &self.all_page_size_info);
                        self.view_center_info = (false, (pidx, dist));
                    }
                    Jump::Footer => {
                        // 目标让至少左下角出现在屏幕中，
                        self.cursor.jump_to_special_img_pos(
                            pidx,
                            Vec2::ZERO,
                            &self.all_page_size_info,
                        );
                        let related_info = self.cursor.get_multi_image(&self.all_page_size_info);
                        match related_info
                            .iter()
                            .position(|(page_idx, _, _)| *page_idx == pidx)
                        {
                            None => panic!("如果设计正常，那么一定存在一个等于pidx的page index"),
                            Some(idx) => {
                                let (_, img_uv, pos_uv) = &related_info[idx];
                                let aim_img_uv = pos2(0., 1.);
                                self.view_center_info = (false, (pidx, Vec2::ZERO));
                                if !img_uv.contains(aim_img_uv) {
                                    let ratio = pos_uv.size() / img_uv.size();
                                    let lb_uv_dist = aim_img_uv - img_uv.left_bottom();
                                    let dist = lb_uv_dist
                                        * ratio
                                        * self.all_page_size_info.get_img_size(pidx);
                                    self.cursor.move_center(dist);
                                    self.view_center_info.1.1 += lb_uv_dist; // 按理来说，如果pidx的中心出现在当前视图中，那么lb_uv_dist的绝对值一定小于vec2(0.5, 0.5)
                                }
                            }
                        };
                    }
                    Jump::Header => {
                        // 目标让至少左上角出现在屏幕中，
                        self.cursor.jump_to_special_img_pos(
                            pidx,
                            Vec2::ZERO,
                            &self.all_page_size_info,
                        );
                        let related_info = self.cursor.get_multi_image(&self.all_page_size_info);
                        match related_info
                            .iter()
                            .position(|(page_idx, _, _)| *page_idx == pidx)
                        {
                            None => panic!("如果设计正常，那么一定存在一个等于pidx的page index"),
                            Some(idx) => {
                                let (_, img_uv, pos_uv) = &related_info[idx];
                                let aim_img_uv = pos2(0., 0.);
                                self.view_center_info = (false, (pidx, Vec2::ZERO));
                                if !img_uv.contains(aim_img_uv) {
                                    let ratio = pos_uv.size() / img_uv.size();
                                    let lt_uv_dist = aim_img_uv - img_uv.left_top();
                                    let dist = lt_uv_dist
                                        * ratio
                                        * self.all_page_size_info.get_img_size(pidx);
                                    self.cursor.move_center(dist);
                                    self.view_center_info.1.1 += lt_uv_dist;
                                }
                            }
                        };
                    }
                }
                self.update_view_info();
            }
            ViewAction::Rescale(scale) => {
                let min_outer_size = self.all_page_size_info.min_outer_size;
                let long_slide = (min_outer_size.x).max(min_outer_size.y);
                let disp_scale_upper_bound = self.max_slide / long_slide / self.pixels_per_point;
                let scale = scale.min(disp_scale_upper_bound);
                if self.disp_scale != scale {
                    let (page_idx, vec) = self.view_center_info.1;
                    self.cursor
                        .update_rescale(scale * self.pixels_per_point, &self.all_page_size_info);
                    self.disp_scale = scale;
                    self.cursor
                        .jump_to_special_img_pos(page_idx, vec, &self.all_page_size_info);
                    self.update_view_info();
                }
            }
            ViewAction::Shift(dist) => {
                self.cursor.move_center(dist);
                self.update_view_info();
                self.update_view_center();
            }
            _ => (),
        }
    }
    pub fn get_view_img<'b>(
        &self,
        texture_hash_map: &mut HashMap<usize, RenderPage>,
        require_line: &mut Line<(usize, f32)>,
    ) -> MultiImage<'b> {
        let view_page_info = &self.view_page_info.1;
        let mut img_array = Vec::with_capacity(view_page_info.len());
        let size = self.all_page_size_info.min_outer_size;
        let long_slide = (size.x).max(size.y);
        let scale_upper_bound = self.max_slide / long_slide / self.pixels_per_point;
        let new_disp_scale = (self.disp_scale * 2.)
            .max(self.scale_low_bound)
            .min(scale_upper_bound)
            * self.pixels_per_point;
        for (pidx, img_uv, pos_uv) in view_page_info.iter() {
            let info = PageRenderInfo {
                page_idx: *pidx,
                page_size: self.all_page_size_info.page_size_vec[*pidx],
                scale: new_disp_scale,
            };
            let img = get_img(info, *img_uv, texture_hash_map, require_line);
            img_array.push((img, *pos_uv));
        }
        MultiImage {
            images: img_array,
            disp_rect: self.disp_rect,
        }
    }
}
impl GetImgInfo for PagesInfoRc {
    fn get_img_number(self: &Self) -> usize {
        self.page_num
    }
    fn get_min_outer_size(self: &Self) -> Vec2 {
        self.min_outer_size
    }
    fn get_img_size(self: &Self, pix: usize) -> Vec2 {
        self.page_size_vec[pix]
    }
}

fn is_scale_reload(scale: f32, disp_scale: f32) -> bool {
    (disp_scale > scale || disp_scale * 4. < scale)
}
pub fn get_img<'b>(
    rend_info: PageRenderInfo, // info内的scale一定要合法，比如不考虑ppp的时候，不会低于view的lower_bound，又或者不会大于max_slide，在函数的内部仅会对rend_info与当前的scale之间是否构成重绘进行判断。
    img_uv: Rect,
    texture_cache: &mut HashMap<usize, RenderPage>,
    require_line: &mut Line<(usize, f32)>,
) -> ImageShow<'b> {
    let pidx = rend_info.page_idx;
    let render_page = texture_cache
        .entry(pidx)
        .and_modify(|render_page| {
            if is_scale_reload(render_page.scale(), rend_info.scale) {
                let required = render_page.new_info(rend_info);
                if let Some(required) = required {
                    require_line.insert(required);
                };
            }
        })
        .or_insert_with(|| {
            let required = (rend_info.page_idx, rend_info.scale);
            require_line.insert(required);
            RenderPage::new(rend_info)
        });
    match render_page.get_texture() {
        Some(texture_handle) => {
            let img = Image::from_texture(texture_handle).uv(img_uv);
            ImageShow::I(img)
        }
        None => ImageShow::C(Color32::LIGHT_GRAY),
    }
}
// 用于preview之类的，只提供页码不对scale有要求，如果cache中存在对应的页面，那么直接按照现有的scale直接获取，如果没有取scale = 1.0 * ppp;
pub fn get_img_without_scale<'b>(
    pidx: usize,
    size: Vec2,
    img_uv: Rect,
    ppp: f32,
    texture_cache: &mut HashMap<usize, RenderPage>,
    require_line: &mut Line<(usize, f32)>,
) -> ImageShow<'b> {
    let scale = match texture_cache.get(&pidx) {
        None => 1.0 * ppp,
        Some(img_cache) => img_cache.scale(),
    };
    let rend_info = PageRenderInfo {
        page_idx: pidx,
        page_size: size,
        scale,
    };
    get_img(rend_info, img_uv, texture_cache, require_line)
}

use crate::backend::{BackEnd, DocumentInfo, SpecialPage};
use crate::constant::*;
use crate::link::{LinkContent, LinkIndex, SpecialPageLink, link_save_strnig};
use crate::multi_image::{ImageShow, MultiImage};
use crate::resizeable::{ResizeableRect, SelectZone};
use crate::view::{Jump, PageRenderInfo, RenderPage, View, ViewAction, get_img_without_scale};
use crate::{Line, allocate_and_fill, fit_vec_in_rect, painter_richtext, shrink2_but_meaningful};
use egui::{
    Button, Color32, ColorImage, CornerRadius, FontId, Key, Modifiers, Pos2, Rect, Response,
    RichText, Sense, Stroke, TextEdit, TextureHandle, Tooltip, Vec2, pos2, vec2,
};
use input_egui::{InputState, WarningInput};
use pdfium_render::prelude::Pdfium;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};

use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::mem;
use std::{fmt, thread};

#[derive(Debug, Default)]
enum HoverPreviewState<'a> {
    #[default]
    Empty,
    Instance(LinkIndex, Rect, (Vec2, ImageShow<'a>)), // page_index inner_index
    InHash(LinkIndex),
}
impl<'a> HoverPreviewState<'a> {
    fn take(&mut self) -> Self {
        mem::take(self)
    }
    fn if_clear(&mut self, target_idx: LinkIndex) {
        match self {
            HoverPreviewState::InHash(index) if *index == target_idx => {
                self.take();
            }
            _ => (),
        }
    }
}

#[derive(Debug, Default)]
enum ReaderState {
    #[default]
    Read,
    Select((usize, usize), SelectZone), // 第一个usize用于确定page_idx，并非一定是page_idx
}
impl ReaderState {
    fn take(&mut self) -> Self {
        mem::take(self)
    }
    fn change2select(&mut self, other_idx: usize, iidx: usize, outer: Rect, inner: (Vec2, Vec2)) {
        match self {
            ReaderState::Select(_, _) => panic!("不可在select的状态下再次进入select"),
            ReaderState::Read => {
                let lt = outer.min;
                let (min_vec, max_vec) = inner;
                let select_rect = Rect::from_min_max(lt + min_vec, lt + max_vec);
                *self = ReaderState::Select((other_idx, iidx), SelectZone::new(outer, select_rect))
            }
        }
    }
    fn back2read(&mut self) -> ((usize, usize), (Vec2, Vec2)) {
        match self.take() {
            ReaderState::Read => panic!("不可在read状态下返回read"),
            ReaderState::Select(idx, select_zone) => {
                let (outer_rect, select_rect) = select_zone.return_rect();
                let lt = outer_rect.left_top();
                let i_lt = select_rect.left_top();
                let i_rb = select_rect.right_bottom();
                (idx, (i_lt - lt, i_rb - lt))
            }
        }
    }
}
const READ_TOP_RECT_NUM: usize = 12;
fn top_panel_rect_calc() -> ([Rect; 2], [Rect; READ_TOP_RECT_NUM]) {
    let input_rect = Rect::from_min_size(pos2(4., 0.), vec2(400., 30.));
    let verify_rect = Rect::from_min_size(input_rect.right_top(), vec2(30., 30.));

    let left_rect = Rect::from_min_size(verify_rect.right_top() + vec2(10., 0.), vec2(30., 30.));
    let page_text_rect = Rect::from_min_size(left_rect.right_top(), vec2(60., 30.));
    let right_rect = Rect::from_min_size(page_text_rect.right_top(), vec2(30., 30.));

    let larger_rect = Rect::from_min_size(right_rect.right_top() + vec2(10., 0.), vec2(30., 30.));
    let reduce_rect = Rect::from_min_size(larger_rect.right_top(), vec2(30., 30.));
    let scale_text_rect = Rect::from_min_size(reduce_rect.right_top(), vec2(50., 30.));

    let save_rect =
        Rect::from_min_size(scale_text_rect.right_top() + vec2(10., 0.), vec2(130., 30.));

    let single_rect = Rect::from_min_size(save_rect.right_top() + vec2(10., 0.), vec2(30., 30.));
    let double_rect = Rect::from_min_size(single_rect.right_top(), vec2(40., 30.));
    let sl_rect = Rect::from_min_size(double_rect.right_top(), vec2(50., 30.));

    let horizontal_rect = Rect::from_min_size(sl_rect.right_top() + vec2(20., 0.), vec2(150., 30.));
    let link_disp_rect = Rect::from_min_size(horizontal_rect.right_top(), vec2(100., 30.));
    (
        [input_rect, verify_rect],
        [
            left_rect,
            page_text_rect,
            right_rect,
            larger_rect,
            reduce_rect,
            scale_text_rect,
            save_rect,
            single_rect,
            double_rect,
            sl_rect,
            horizontal_rect,
            link_disp_rect,
        ],
    )
}
struct Basic {
    scale: f32,
    disp_rect: Rect,
    read_mode: usize,
    rs: (
        Receiver<(PageRenderInfo, [usize; 2], Vec<u8>)>,
        Sender<SpecialPage>,
    ),
}
impl Basic {
    fn new(prr: Receiver<(PageRenderInfo, [usize; 2], Vec<u8>)>, cs: Sender<SpecialPage>) -> Self {
        let init_rect = Rect::from_min_max(pos2(0., 30.), pos2(INIT_UI_SIZE[0], INIT_UI_SIZE[1]));
        Self {
            scale: 1.,
            disp_rect: init_rect,
            read_mode: 1,
            rs: (prr, cs),
        }
    }
    fn show(&mut self, ui: &mut egui::Ui, available_size: Vec2) {
        self.disp_rect.max = self.disp_rect.min + available_size;
        ui.painter()
            .rect_filled(self.disp_rect, CornerRadius::same(0), Color32::GRAY);
    }
}
#[derive(Default)]
enum ReadPart<'a> {
    #[default]
    Empty,
    Basic(Basic),
    Reader(Reader<'a>),
}
impl<'a> ReadPart<'a> {
    fn is_changed(&self) -> bool {
        match self {
            ReadPart::Reader(reader) => reader.changed,
            _ => false,
        }
    }
    fn update_info(self, ppp: f32, document_info: DocumentInfo) -> Self {
        let basic = match self {
            ReadPart::Basic(basic) => basic,
            ReadPart::Reader(reader) => reader.get_basic(),
            ReadPart::Empty => panic!(),
        };
        ReadPart::Reader(Reader::new_with_document_info(document_info, ppp, basic))
    }
}
pub struct MyApp<'a> {
    input: (String, InputState, String), // 输入字符串的buffer，inputstate输入状态，warning信息
    top_panel_rect: ([Rect; 2], [Rect; READ_TOP_RECT_NUM]),
    file_path_sender: Sender<String>,
    file_state_reciver: Receiver<Option<DocumentInfo>>,
    end_sender: Sender<()>,
    reader: ReadPart<'a>,
}
impl<'a> Drop for MyApp<'a> {
    fn drop(&mut self) {
        self.end_sender.send(());
    }
}
impl<'a> MyApp<'a> {
    pub fn new() -> Self {
        let (fs, fr) = channel();
        let (cs, cr) = channel();
        let (fss, fsr) = channel();
        let (prs, prr) = channel();
        let (es, er) = channel();
        thread::spawn(move || {
            let root = env!["CARGO_MANIFEST_DIR"];
            let pdfium = Pdfium::new(
                Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&format!(
                    "{}/asset",
                    root
                )))
                .unwrap(),
            );
            let mut back_end = BackEnd::new(&pdfium, fr, cr, fss, prs, er);
            back_end.run();
        });
        let reader = ReadPart::Basic(Basic::new(prr, cs));
        let input = (String::new(), InputState::Active, String::new());
        Self {
            input,
            reader,
            top_panel_rect: top_panel_rect_calc(),
            file_path_sender: fs,
            file_state_reciver: fsr,
            end_sender: es,
        }
    }
    pub fn new_with_path(path: &String) -> Self {
        let mut app = Self::new();
        let input = (String::from(path), InputState::WaitingVerify, String::new());
        app.input = input;
        app
    }
    fn top_panel_input(&mut self, ui: &mut egui::Ui) {
        let input_related_rect = &self.top_panel_rect.0;
        let input_rect = input_related_rect[0];
        let verify_rect = input_related_rect[1];

        let verify_response = ui.put(
            verify_rect,
            Button::new(
                RichText::new(if self.input.1.is_active() {
                    "✅"
                } else {
                    "↩"
                })
                .color(Color32::BLACK),
            )
            .fill(Color32::GRAY),
        );
        let action = if verify_response.clicked() && self.input.1.is_active() {
            Some("get")
        } else {
            None
        };
        let reactiveable = match self.input.1 {
            InputState::WaitingVerify | InputState::InActive => true,
            _ => false,
        }; // 这里使用inputstate::verified作为一个中间情况等待子线程响应，在响应期间不能reactive
        let re_active = reactiveable && verify_response.clicked();
        if re_active {
            self.input.1 = InputState::Active;
            self.input.2.clear();
        }
        let mut warn_input = WarningInput::new(
            None,
            &mut self.input.0,
            &mut self.input.1,
            "file path input",
            20.,
            input_rect,
            "",
        );
        if self.input.2.len() != 0 {
            warn_input.insert_extern_warning(&self.input.2);
        }
        warn_input.show_with_action(ui, action);
    }

    fn top_panel(&mut self, ui: &mut egui::Ui, available_size: Vec2) -> (Vec2) {
        ui.painter().rect_filled(
            Rect::from_min_max(Pos2::ZERO, pos2(available_size.x, 30.)),
            CornerRadius::same(0),
            Color32::LIGHT_GRAY,
        );
        let mut rem_size = available_size - vec2(0., 30.);
        rem_size.y = rem_size.y.max(0.);
        self.top_panel_input(ui);

        ui.painter().line(
            vec![pos2(0., 30.), pos2(available_size.x, 30.)],
            Stroke::new(1., Color32::LIGHT_GRAY),
        );
        (rem_size)
    }
    fn verify_input(&mut self) -> Option<DocumentInfo> {
        // 判断输入是否合法
        // 由于现在读取document和主线程不在同一条线程上，所以
        // 两段式检验，waitingverify的时候，检验是否以pdf后缀为结。后将其置为verified
        // verified则需要等到消息，如果获取消息，显示读取成功，则跳转到inactive，在waiting verify和inactive阶段都能重新reactive
        let mut new_document_info = None;
        match self.input.1 {
            InputState::Active | InputState::InActive => (),
            InputState::WaitingVerify => {
                if self.input.0.ends_with(".pdf") {
                    self.input.1 = InputState::Verified;
                    self.file_path_sender.send(self.input.0.clone());
                } else {
                    self.input.1 = InputState::InActive;
                }
            }
            InputState::Verified => {
                if let Ok(recv) = self.file_state_reciver.try_recv() {
                    match recv {
                        None => {
                            self.input.1 = InputState::InActive;
                            self.input.2.clear();
                            self.input.2.insert_str(0, "can not load file");
                        }
                        Some(info) => {
                            self.input.1 = InputState::InActive;
                            self.input.2.clear();
                            new_document_info = Some(info);
                        }
                    }
                }
            }
        };
        new_document_info
    }
    fn show(&mut self, ui: &mut egui::Ui) {
        let total_size = ui.available_size();

        let (rem_size) = self.top_panel(ui, total_size);
        let dropped_file = ui.input_mut(|i| i.raw.dropped_files.pop());
        if let Some(file) = dropped_file
            && let Some(path) = file.path
            && let Some(path_str) = path.to_str()
        {
            self.input.0.clear();
            self.input.0.insert_str(0, path_str);
            self.input.1 = InputState::WaitingVerify;
        }
        let ppp = ui.ctx().pixels_per_point();
        if let Some(document_info) = self.verify_input() {
            let reader = mem::take(&mut self.reader);
            self.reader = reader.update_info(ppp, document_info);
        };
        match &mut self.reader {
            ReadPart::Basic(basis) => basis.show(ui, rem_size),
            ReadPart::Reader(reader) => {
                let action = reader.top_panel_button(ui, self.top_panel_rect.1);
                let view_action = reader.view_action.take();
                reader.view_action = view_action.or(action);
                reader.show(ui, rem_size);
            }
            ReadPart::Empty => panic!(),
        }
    }
}
impl<'a> eframe::App for MyApp<'a> {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
#[derive(Debug, Default)]
struct CustomFlag {
    horizontal_scrolling: bool,
    link_rect_disp: bool,
    page_num_input: bool,
}
pub struct Reader<'a> {
    link_name: String, // 保存文件使用的文字
    render_cache: HashMap<usize, RenderPage>,
    page_size_vec: Rc<Vec<Vec2>>,
    hover_preview: HoverPreviewState<'a>, // 从前向后依次为preview的页码，link码，link的rect， 可持续时间，hw_ratio，image
    // 其中可持续时间是想解决如何很好的控制多点hover的问题。一个hover_preview设定为，如果pointer_pos不处在link_rect上，或者不处在preview上，那么将消失。hover in则重置该量，
    fixed_preview: HashMap<LinkIndex, (usize, Rect, ImageShow<'a>)>,
    fix_preview_order: Vec<LinkIndex>,
    link: Vec<Vec<SpecialPageLink>>,
    ref_link: Vec<Vec<LinkContent>>,
    state: ReaderState,
    view_action: ViewAction,
    view: View,
    cs: Sender<SpecialPage>,
    prr: Receiver<(PageRenderInfo, [usize; 2], Vec<u8>)>,
    view_page_info: (usize, Vec<(usize, Rect, Rect)>),
    require_line: Line<(usize, f32)>,
    view_img: MultiImage<'a>,
    changed: bool,
    page_str: String,
    custom_flags: CustomFlag,
}
impl<'a> Reader<'a> {
    fn get_basic(self) -> Basic {
        let view = &self.view;
        Basic {
            disp_rect: view.disp_rect,
            scale: view.disp_scale,
            read_mode: view.line_num,
            rs: (self.prr, self.cs),
        }
    }
    fn new_with_document_info(document_info: DocumentInfo, ppp: f32, basic: Basic) -> Self {
        let pages = document_info.pages;
        let links = document_info.links;
        let link_file_name = document_info.link_file_name;

        Self::new(
            link_file_name,
            pages.num,
            pages.min_outer_size,
            pages.size_vec,
            basic.disp_rect,
            ppp,
            basic.scale,
            basic.read_mode,
            links.0,
            links.1,
            basic.rs.1,
            basic.rs.0,
        )
    }
    fn new(
        link_name: String,
        page_num: usize,
        min_outer_size: Vec2,
        page_size_vec: Vec<Vec2>,
        disp_rect: Rect,
        ppp: f32,
        scale: f32,
        read_mode: usize,
        link: Vec<Vec<SpecialPageLink>>,
        ref_link: Vec<Vec<LinkContent>>,
        cs: Sender<SpecialPage>,
        prr: Receiver<(PageRenderInfo, [usize; 2], Vec<u8>)>,
    ) -> Self {
        let mut render_cache = HashMap::new();
        let mut require_line = Line::new();
        let page_size_vec = Rc::new(page_size_vec);
        let mut view = View::new(
            page_num,
            min_outer_size,
            page_size_vec.clone(),
            disp_rect,
            scale,
            ppp,
        );
        view.update_img_cursor_with_action(ViewAction::Col(read_mode));
        let view_img = view.get_view_img(&mut render_cache, &mut require_line);
        let view_page_info = view.view_page_info.clone();
        Reader {
            link_name,
            render_cache: render_cache,
            page_size_vec,
            hover_preview: HoverPreviewState::Empty,
            fixed_preview: HashMap::new(),
            fix_preview_order: Vec::new(),
            link,
            ref_link,
            state: ReaderState::Read,
            view_action: ViewAction::Empty,
            view: view,
            cs,
            prr,
            view_page_info,
            require_line,
            view_img,
            changed: false,
            page_str: String::new(),
            custom_flags: CustomFlag::default(),
        }
    }
    fn save_link_info(&mut self) -> Result<(), String> {
        // 将size信息写回linkcentent
        for (target_idx, (_, rect, _)) in &self.fixed_preview {
            self.ref_link[target_idx.page_idx][target_idx.inner_idx].disp_size = rect.size();
        }
        let mut file = File::create(&self.link_name).map_err(|err| format!("{}", err))?;
        file.write_all(link_save_strnig(&self.link, &self.ref_link).as_bytes())
            .map_err(|err| format!("{}", err))?;
        Ok(())
    }
    fn send_required(&mut self) {
        let head = self.require_line.pop_head();
        match head {
            None => (),
            Some(head) => {
                self.cs.send(head).unwrap(); // 假定在view存续期间，总会有接收端
            }
        }
    }
    fn update_with_special_page(&mut self, page_idx: usize, texture: &TextureHandle) {
        // 根据页码标识，更新所有的需求相同page_idx的preview or view_img
        // 不设计为由self主动查询cache，而是由调用主动传入texture_handle，是为了方便
        if let Some(idx) = self.view_page_info.1.iter().position(|x| x.0 == page_idx) {
            let img_uv = self.view_page_info.1[idx].1;
            self.view_img.images[idx]
                .0
                .update_with_texture(texture, img_uv);
        }
        for (target_idx, val) in self
            .fixed_preview
            .iter_mut()
            .filter(|(key, _)| key.page_idx == page_idx)
        {
            let inner_idx = target_idx.inner_idx;
            let (min, max) = self.ref_link[page_idx][inner_idx].content_rect;
            val.2
                .update_with_texture(texture, Rect::from_min_max(min.to_pos2(), max.to_pos2()));
        }
        match &mut self.hover_preview {
            HoverPreviewState::Instance(target_idx, _, (_, img)) => {
                let inner_idx = target_idx.inner_idx;
                let (min, max) = self.ref_link[page_idx][inner_idx].content_rect;
                img.update_with_texture(texture, Rect::from_min_max(min.to_pos2(), max.to_pos2()));
            }
            _ => (),
        }
    }
    fn get_render(&mut self, ctx: &egui::Context) {
        // 通过prr获取渲染成功的页面的信息，并并加载到context中
        match self.prr.try_recv() {
            Ok((render_info, size, img)) => {
                let texture_handle = ctx.load_texture(
                    "",
                    ColorImage::from_rgba_unmultiplied(size, img.as_slice()),
                    Default::default(),
                );
                let pidx = render_info.page_idx;
                self.update_with_special_page(pidx, &texture_handle);
                match self.render_cache.get_mut(&render_info.page_idx) {
                    None => panic!(), // 在发送之前就需要对应的hash至少有内容
                    Some(img) => {
                        img.updata_with_texture(texture_handle, render_info);
                    }
                };
            }
            _ => (),
        }
    }
    fn disp_preview(&mut self, ui: &mut egui::Ui) {
        let mut remove_idx = None;
        for pi_idx in self.fix_preview_order.iter().rev() {
            match self.fixed_preview.get_mut(&pi_idx) {
                None => panic!("vec和hashmap共同变化，代码存在错误"),
                Some((_, rect, img)) => {
                    let (size_changed, close_response) = disp_image_in_rect_with_resized(
                        rect,
                        self.view.disp_rect,
                        img,
                        ui,
                        Color32::GRAY,
                        Color32::BLACK,
                    );
                    self.changed |= size_changed;
                    if close_response.clicked() {
                        remove_idx = remove_idx.or(Some(*pi_idx));
                    }
                }
            }
        }
        match self.hover_preview.take() {
            HoverPreviewState::Empty => (),
            HoverPreviewState::Instance(pi_idx, hover_rect, (disp_size, image)) => {
                // instance 状态下，显示preview，如果指针持续停留在该link的rect或者preview图像上，则状态重新为instance
                // 如果drag，那么该preview将会进入fixed_preview
                let preview_rect = Rect::from_min_size(hover_rect.center(), disp_size);
                image.paint_at(ui, preview_rect);
                ui.painter().rect_stroke(
                    preview_rect,
                    CornerRadius::same(0),
                    Stroke::new(2., Color32::GRAY),
                    egui::StrokeKind::Outside,
                );
                let response = ui.allocate_rect(preview_rect, Sense::click_and_drag());
                // 下面代码存在问题，即便pointer在rect内部，但是依旧返回false
                // println!(
                //     "rect {:?}, pointer {:?}, preview hovered? {}",
                //     response.rect,
                //     ui.input(|i| i.pointer.latest_pos()),
                //     response.contains_pointer()
                // );
                // if !response.contains_pointer() {
                //     *time -= 1;
                // }
                if response.drag_started() || response.clicked() {
                    self.fixed_preview.insert(
                        pi_idx,
                        (0, preview_rect.translate(response.drag_delta()), image), // 不知为何这里无法出现拖动的现象，
                    );
                    self.fix_preview_order.insert(0, pi_idx);
                } else if let Some(pointer_pos) = ui.input(|i| i.pointer.latest_pos())
                    && response.rect.contains(pointer_pos)
                {
                    self.hover_preview =
                        HoverPreviewState::Instance(pi_idx, hover_rect, (disp_size, image));
                }
            }
            HoverPreviewState::InHash(pi_idx) => match self.fixed_preview.get_mut(&pi_idx) {
                None => panic!("如果是inhash，前面在disp_one_page中已经验证过了"),
                Some((order, rect, image)) => {
                    let (size_changed, close_response) = disp_image_in_rect_with_resized(
                        rect,
                        self.view.disp_rect,
                        image,
                        ui,
                        Color32::RED,
                        Color32::RED,
                    );
                    self.changed |= size_changed;
                    if close_response.clicked() {
                        remove_idx = remove_idx.or(Some(pi_idx));
                    }

                    self.fix_preview_order[0..=*order].rotate_right(1);
                }
            },
        }
        match remove_idx {
            None => (),
            Some(pi_idx) => {
                self.hover_preview.if_clear(pi_idx);
                match self.fixed_preview.remove(&pi_idx) {
                    None => panic!("必须存在"),
                    Some((order, _, _)) => {
                        self.fix_preview_order.remove(order);
                    }
                };
            }
        }
        // 调整所有的fix_preview的顺序
        for (idx, pi_idx) in self.fix_preview_order.iter().enumerate() {
            self.fixed_preview.entry(*pi_idx).and_modify(|f| f.0 = idx);
        }
    }
    fn disp_link(&mut self, ui: &mut egui::Ui, detected_hover: bool) -> ViewAction {
        // 显示link相关内容
        // 首先显示超链接的位置
        // 其次是target的情况，
        let mut view_action = ViewAction::Empty;
        let mut hover = None;
        let view_page_info = &self.view_page_info;
        let disp_rect = self.view.disp_rect;
        let disp_lt = disp_rect.left_top();
        let disp_size = disp_rect.size();
        let painter = ui.painter_at(disp_rect);
        for (idx, (pidx, img_uv, pos_uv)) in view_page_info.1.iter().enumerate() {
            // img_uv 显示的图像占据原图像的uv
            // pos_uv 该图像显示区域占据总显示区域的uv
            let pos_uv_lt = pos_uv.left_top().to_vec2();
            let img_uv_lt = img_uv.left_top().to_vec2();
            let pos_uv_size = pos_uv.size();
            let pos_screen_size = pos_uv_size * disp_size;
            let disp_img_uv_ratio = pos_screen_size / img_uv.size();
            let pos_screen_lt = disp_lt + pos_uv_lt * disp_size;
            let img_lt_in_screen = pos_screen_lt - img_uv_lt * disp_img_uv_ratio;

            let link_pidx = &self.link[*pidx];
            for (iidx, spl) in link_pidx.iter().enumerate() {
                let (min, max) = spl.click_rect;

                let min_in_screen = img_lt_in_screen + min * disp_img_uv_ratio;
                let rect = Rect::from_min_size(min_in_screen, (max - min) * disp_img_uv_ratio);
                if self.custom_flags.link_rect_disp {
                    painter.rect_stroke(
                        rect,
                        CornerRadius::same(0),
                        Stroke::new(2., Color32::RED),
                        egui::StrokeKind::Middle,
                    );
                }
                let response = ui.allocate_rect(rect.intersect(disp_rect), Sense::click());
                if response.clicked() {
                    let target_idx = &spl.target_index;
                    let ref_link = &self.ref_link[target_idx.page_idx][target_idx.inner_idx];
                    view_action = view_action.or((ViewAction::Jump(
                        target_idx.page_idx,
                        Jump::PagePos(
                            (ref_link.content_rect.0 + ref_link.content_rect.1) / 2.
                                - Vec2::splat(0.5),
                        ),
                    )));
                } else if (response.hovered() && detected_hover) {
                    hover = hover.or(Some((*pidx, iidx, response.rect)));
                }
            }
            let ref_pidx = &mut self.ref_link[*pidx];
            for (iidx, lc) in ref_pidx.iter_mut().enumerate() {
                let (min, max) = lc.content_rect;
                let hint_pos = lc.get_pos();
                if img_uv.contains(hint_pos.to_pos2()) {
                    let hint_in_pos = img_lt_in_screen + hint_pos * disp_img_uv_ratio;
                    let response = ui.put(
                        Rect::from_center_size(hint_in_pos, vec2(5., 5.)),
                        Button::new("💡"),
                    );
                    if response.clicked() {
                        let min_in_screen_rel = (min - img_uv_lt) * disp_img_uv_ratio;
                        let max_in_screen_rel = (max - img_uv_lt) * disp_img_uv_ratio;
                        let outer_rect = Rect::from_min_size(pos_screen_lt, pos_screen_size);
                        self.state.change2select(
                            idx,
                            iidx,
                            outer_rect,
                            (min_in_screen_rel, max_in_screen_rel),
                        );
                    }
                }
            }
        }
        if let Some((pidx, iidx, hover_rect)) = hover {
            let link = &self.link[pidx][iidx];
            let target_idx = link.target_index;
            let page_idx = target_idx.page_idx;
            let inner_idx = target_idx.inner_idx;
            match &self.hover_preview {
                HoverPreviewState::InHash(hover_idx)
                | HoverPreviewState::Instance(hover_idx, _, _)
                    if target_idx == *hover_idx =>
                {
                    ()
                }
                _ => match self.fixed_preview.get(&target_idx) {
                    None => {
                        let ref_link = &self.ref_link[page_idx][inner_idx];
                        let (min, max) = ref_link.content_rect;
                        let img_uv = Rect::from_min_max(min.to_pos2(), max.to_pos2());
                        let new_hover_preview = get_img_without_scale(
                            page_idx,
                            self.page_size_vec[page_idx],
                            img_uv,
                            ui.ctx().pixels_per_point(),
                            &mut self.render_cache,
                            &mut self.require_line,
                        );
                        self.hover_preview = HoverPreviewState::Instance(
                            target_idx,
                            hover_rect,
                            (ref_link.disp_size, new_hover_preview),
                        );
                    }
                    Some(_) => {
                        self.hover_preview = HoverPreviewState::InHash(target_idx);
                    }
                },
            }
        }
        view_action
    }
    fn top_panel_button(
        &mut self,
        ui: &mut egui::Ui,
        view_related_rect: [Rect; READ_TOP_RECT_NUM],
    ) -> ViewAction {
        let [
            left_rect,
            page_text_rect,
            right_rect,
            larger_rect,
            reduce_rect,
            scale_text_rect,
            save_rect,
            single_rect,
            double_rect,
            sl_rect,
            horizontal_rect,
            link_disp_rect,
        ]: [Rect; READ_TOP_RECT_NUM] = view_related_rect;
        let mut action = ViewAction::default();

        let button =
            |s: &str| Button::new(RichText::new(s).color(Color32::BLACK)).fill(Color32::GRAY);
        let set_button = |button: Button, rect, ui: &mut egui::Ui| ui.put(rect, button);

        let left_button = button("◀").corner_radius(CornerRadius::same(0));
        let right_button = button("▶").corner_radius(CornerRadius::same(0));
        let scale_larger_button = button("🔍+");
        let scale_reduce_button = button("🔍-"); // 1f50d
        let save_button = button(if self.changed {
            ("💾 *SAVE LINK INFO")
        } else {
            ("💾 SAVE LINK INFO")
        }); //1f4be
        let single_button = button("📄");
        let double_button = button("📄📄");
        let single_line_button = button("📄📄...");

        let horizontal_scroll_check_box = egui::Checkbox::new(
            &mut self.custom_flags.horizontal_scrolling,
            RichText::new("only horizontal scroll").color(Color32::BLACK),
        );

        let link_rect_disp_check_box = egui::Checkbox::new(
            &mut self.custom_flags.link_rect_disp,
            RichText::new("Link Rect Disp").color(Color32::BLACK),
        );

        let left_response = set_button(left_button, left_rect, ui);
        if self.custom_flags.page_num_input {
            let response = ui.put(
                page_text_rect,
                TextEdit::singleline(&mut self.page_str)
                    .font(FontId::new(20., FontId::default().family))
                    .text_color(Color32::BLACK)
                    .background_color(Color32::WHITE),
            );
            if (response.lost_focus()) {
                self.custom_flags.page_num_input = false;
                match self.page_str.parse::<usize>() {
                    Ok(integer) if integer > 0 => {
                        action = action.or(ViewAction::Jump(integer - 1, Jump::Header));
                    }
                    _ => {
                        self.page_str.clear();
                        let page_idx = self.view.view_center_info.1.0;
                        let page_count = self.view.all_page_size_info.page_num;
                        fmt::write(
                            &mut self.page_str,
                            format_args!("{}/{}", (page_idx + 1).min(page_count), page_count),
                        );
                    }
                }
            }
        } else {
            painter_richtext(
                ui.painter_at(page_text_rect),
                page_text_rect,
                RichText::new(&self.page_str),
                egui::FontSelection::Default,
                Color32::BLACK,
                Color32::GRAY,
            );
            let response = ui.allocate_rect(page_text_rect, Sense::click());
            if response.double_clicked() {
                self.page_str.clear();
                let page_idx = self.view.view_center_info.1.0;
                let page_count = self.view.all_page_size_info.page_num;
                fmt::write(
                    &mut self.page_str,
                    format_args!("{}", (page_idx + 1).min(page_count)),
                );
                self.custom_flags.page_num_input = true;
            }
        }

        let right_response = set_button(right_button, right_rect, ui);
        Tooltip::for_enabled(&left_response).show(|ui| ui.label("Last Page"));
        Tooltip::for_enabled(&right_response).show(|ui| ui.label("Next Page"));

        let larger_response = set_button(scale_larger_button, larger_rect, ui);
        let reduce_response = set_button(scale_reduce_button, reduce_rect, ui);
        Tooltip::for_enabled(&larger_response).show(|ui| ui.label("Zoom In"));
        Tooltip::for_enabled(&reduce_response).show(|ui| ui.label("Zoom Out"));
        painter_richtext(
            ui.painter_at(scale_text_rect),
            scale_text_rect,
            RichText::new(format!("{:.1}%", self.view.disp_scale * 100.))
                .background_color(Color32::WHITE),
            egui::FontSelection::Default,
            Color32::BLACK,
            Color32::WHITE,
        );

        let save_response = set_button(save_button, save_rect, ui);
        Tooltip::for_enabled(&save_response).show(|ui| ui.label("Save Preview Info"));

        let single_response = set_button(single_button, single_rect, ui);
        let double_response = set_button(double_button, double_rect, ui);
        let single_line_response = set_button(single_line_button, sl_rect, ui);
        Tooltip::for_enabled(&single_response).show(|ui| ui.label("One Page per Line"));
        Tooltip::for_enabled(&double_response).show(|ui| ui.label("Two Page per Line"));
        Tooltip::for_enabled(&single_line_response).show(|ui| ui.label("Single Line"));

        ui.scope_builder(egui::UiBuilder::new().max_rect(horizontal_rect), |ui| {
            ui.add(horizontal_scroll_check_box)
        });

        ui.scope_builder(egui::UiBuilder::new().max_rect(link_disp_rect), |ui| {
            ui.add(link_rect_disp_check_box)
        });

        if left_response.clicked() {
            // let (page_idx, center_pos) = self.viewer.page_info;
            // action = action.or(ViewAction::Jump(page_idx.max(1) - 1, center_pos));
            let (page_idx, _) = self.view.view_center_info.1;
            action = action.or(ViewAction::Jump(page_idx.max(1) - 1, Jump::Footer));
        }
        if right_response.clicked() {
            let (page_idx, _) = self.view.view_center_info.1;
            let max_page_idx = self.view.max_page_index();
            action = action.or(ViewAction::Jump(
                (page_idx + 1).min(max_page_idx),
                Jump::Header,
            ));
        }
        if larger_response.clicked() {
            let disp_scale = self.view.disp_scale;
            action = action.or(ViewAction::Rescale((disp_scale * 1.25).max(MIN_SCALE)));
        }
        if reduce_response.clicked() {
            let disp_scale = self.view.disp_scale;
            action = action.or(ViewAction::Rescale((disp_scale * 0.8).max(MIN_SCALE)));
        }
        if single_response.clicked() {
            action = action.or(ViewAction::Col(1));
        }
        if double_response.clicked() {
            action = action.or(ViewAction::Col(2));
        }
        if single_line_response.clicked() {
            action = action.or(ViewAction::Col(self.view.all_page_size_info.page_num));
        }
        if save_response.clicked() {
            match self.save_link_info() {
                Ok(()) => {
                    self.changed = false;
                }
                Err(err) => {
                    println!("{err}");
                }
            };
        }

        action
    }
    fn show(&mut self, ui: &mut egui::Ui, available_size: Vec2) {
        // println!("render_cache {:#?}", self.render_cache);
        println!("view img info {:#?}", self.view_page_info);
        // println!("view img {:#?}", self.view_img.images);
        self.send_required();
        self.get_render(ui.ctx());
        let disp_rect = self.view.disp_rect;
        let mut old_action = self.view_action.take();
        if disp_rect.size() != available_size {
            old_action = old_action.or(ViewAction::ReSize(available_size));
            ui.request_repaint();
        }
        ui.painter()
            .rect_filled(disp_rect, CornerRadius::same(0), Color32::GRAY);

        // 主视图绘制
        let pos_change = !old_action.is_empty();
        self.view.update_img_cursor_with_action(old_action);
        if self.view.view_page_info.0 != self.view_page_info.0 {
            self.view_page_info = self.view.view_page_info.clone();
            self.view_img = self
                .view
                .get_view_img(&mut self.render_cache, &mut self.require_line);
        }
        self.view_img.show(ui);
        if pos_change && !self.custom_flags.page_num_input {
            self.page_str.clear();
            let page_idx = self.view.view_center_info.1.0;
            let page_count = self.view.all_page_size_info.page_num;
            fmt::write(
                &mut self.page_str,
                format_args!("{}/{}", (page_idx + 1).min(page_count), page_count),
            );
        }
        // self.reloaded |= reload;

        let mut action = ViewAction::Empty;

        let mut detected_hover = true;
        // 主视图动作捕捉
        let disp_response = ui.allocate_rect(self.view.disp_rect, Sense::drag());
        if disp_response.dragged() {
            action = action.or(ViewAction::Shift(disp_response.drag_delta() * -1.));
            detected_hover &= false;
        }
        action = action.or({
            let zoom_delta = ui.input(|i| i.zoom_delta());
            let scroll = ui.input(|i| i.smooth_scroll_delta());
            if zoom_delta != 1.0 {
                detected_hover &= false;
                ViewAction::Rescale((self.view.disp_scale * zoom_delta).max(MIN_SCALE))
            } else if scroll != Vec2::ZERO {
                detected_hover &= false;
                ViewAction::Shift(if self.custom_flags.horizontal_scrolling {
                    vec2(scroll.y, 0.)
                } else {
                    scroll
                })
            } else {
                ViewAction::Empty
            }
        });

        match &mut self.state {
            ReaderState::Read => {
                if self.view.disp_scale > 0.5 {
                    action = action.or(self.disp_link(ui, detected_hover));
                    self.disp_preview(ui);
                }
            }
            ReaderState::Select(_, select_zone) => {
                let changed = select_zone.is_changed();
                select_zone.show(ui);
                ui.input_mut(|i| {
                    if i.key_pressed(Key::Enter) {
                        i.consume_key(Modifiers::NONE, Key::Enter);
                        let ((idx, iidx), (min, max)) = self.state.back2read();
                        let ith_view_page_info = &self.view_page_info.1[idx];
                        let disp_size = self.view.disp_rect.size();
                        // 从屏幕坐标变为对应页的uv坐标，
                        let mut min_screen_uv = min / disp_size;
                        let mut max_screen_uv = max / disp_size;
                        min_screen_uv = min_screen_uv.min(REL_VEC2_MAX).max(REL_VEC2_MIN);
                        max_screen_uv = max_screen_uv.min(REL_VEC2_MAX).max(REL_VEC2_MIN);
                        let img_uv = ith_view_page_info.1;
                        let pos_uv = ith_view_page_info.2;
                        let img_uv_lt = img_uv.left_top().to_vec2();
                        let disp_img_uv_ratio = img_uv.size() / pos_uv.size();
                        let mut min_img_uv = img_uv_lt + (min_screen_uv) * disp_img_uv_ratio;
                        let mut max_img_uv = img_uv_lt + (max_screen_uv) * disp_img_uv_ratio;
                        min_img_uv = min_img_uv.min(REL_VEC2_MAX).max(REL_VEC2_MIN);
                        max_img_uv = max_img_uv.min(REL_VEC2_MAX).max(REL_VEC2_MIN);
                        let ref_link = &mut self.ref_link[ith_view_page_info.0][iidx];
                        ref_link.content_rect = (min_img_uv, max_img_uv);
                        ref_link.disp_size = max - min;
                        self.changed |= changed;
                    }
                });
            }
        }
        self.view_action = action;
    }
}
impl<'a> eframe::App for Reader<'a> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui, vec2(INIT_UI_SIZE[0], INIT_UI_SIZE[1]));
    }
}
fn disp_image_in_rect_with_resized(
    rect: &mut Rect,
    max_rect: Rect,
    image: &mut ImageShow,
    ui: &mut egui::Ui,
    edge_color: Color32,
    corn_color: Color32,
) -> (bool, Response) {
    // 一个带有关闭按钮的可拖动以及调整大小的浮动图像显示。
    // 返回的response为关闭按钮的response
    // 要求该rect始终置于max_rect中

    image.paint_at(ui, *rect);
    ui.painter().rect_stroke(
        *rect,
        CornerRadius::same(0),
        Stroke::new(2., Color32::GRAY),
        egui::StrokeKind::Outside,
    );

    let size = rect.size();
    let center = rect.center();
    let allowed_rect = shrink2_but_meaningful(max_rect, (size / 2.)).translate(-center.to_vec2()); // 以center为原点的max_rect，
    let drag_rect = rect.shrink(2.);
    let painter = ui.painter_at(drag_rect);
    let drag_response = allocate_and_fill(
        drag_rect,
        Sense::click_and_drag(),
        Color32::TRANSPARENT,
        ui,
        &painter,
    );
    if drag_response.hovered() {
        ui.output_mut(|o| {
            o.cursor_icon = egui::CursorIcon::Grab;
        });
    } else if drag_response.dragged() {
        ui.output_mut(|o| {
            o.cursor_icon = egui::CursorIcon::Grabbing;
        });
    }
    *rect = rect.translate(fit_vec_in_rect(drag_response.drag_delta(), allowed_rect));

    let size: f32 = 20.; // close按钮的最大尺寸
    let close_button_size = size.min(rect.height()).min(rect.width());
    let close_button_rect = Rect::from_min_size(
        rect.right_top() - vec2(close_button_size, 0.),
        Vec2::splat(close_button_size),
    );
    let close_response = ui.put(
        close_button_rect,
        Button::new(RichText::new("❎").color(Color32::BLACK))
            .fill(Color32::WHITE)
            .min_size(Vec2::splat(close_button_size)),
    );

    let mut resized = ResizeableRect::new_with_max_rect(rect, 2., edge_color, corn_color, max_rect);
    let size_changed = resized.show(ui);
    return (size_changed, close_response);
}

use crate::constant::*;
use crate::link::{
    LinkContent, LinkIndex, SpecialPageLink, get_link_and_target, link_save_strnig,
    load_from_string,
};
use crate::resizeable::{ResizeableRect, SelectZone};
use crate::view::{Jump, View, ViewAction};
use crate::{allocate_and_fill, fit_vec_in_rect, painter_richtext, shrink2_but_meaningful};

use egui::{
    Button, Color32, CornerRadius, FontId, Image, Key, Modifiers, Pos2, Rect, Response, RichText,
    Sense, Stroke, TextEdit, Tooltip, Vec2, pos2, vec2,
};
use input_egui::{InputState, WarningInput};
use pdfium_render::prelude::{PdfDocument, Pdfium};

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::prelude::*;
use std::mem;

#[derive(Debug, Default)]
enum HoverPreviewState<'a> {
    #[default]
    Empty,
    Instance(LinkIndex, Rect, (Vec2, Image<'a>)), // page_index inner_index
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
}
impl Basic {
    fn new() -> Self {
        let init_rect = Rect::from_min_max(pos2(0., 30.), pos2(INIT_UI_SIZE[0], INIT_UI_SIZE[1]));
        Self {
            scale: 1.,
            disp_rect: init_rect,
            read_mode: 0,
        }
    }
    fn show(&mut self, ui: &mut egui::Ui, available_size: Vec2) {
        self.disp_rect.max = self.disp_rect.min + available_size;
        ui.painter()
            .rect_filled(self.disp_rect, CornerRadius::same(0), Color32::GRAY);
    }
}
enum ReadPart<'a> {
    Empty(Basic),
    Reader(Reader<'a>),
}
impl<'a> ReadPart<'a> {
    fn new() -> Self {
        ReadPart::Empty(Basic::new())
    }
    fn is_empty(&self) -> bool {
        match self {
            ReadPart::Empty(_) => true,
            _ => false,
        }
    }
    fn is_changed(&self) -> bool {
        match self {
            ReadPart::Empty(_) => false,
            ReadPart::Reader(reader) => reader.changed,
        }
    }
}
pub struct MyApp<'a> {
    input: (String, InputState, String), // 输入字符串的buffer，inputstate输入状态，warning信息
    back_end: &'a Pdfium,
    top_panel_rect: ([Rect; 2], [Rect; READ_TOP_RECT_NUM]),
    reader: ReadPart<'a>,
}
impl<'a> MyApp<'a> {
    pub fn new(pdfium: &'a Pdfium) -> Self {
        let reader = ReadPart::new();
        let input = (String::new(), InputState::Active, String::new());
        Self {
            input,
            back_end: pdfium,
            reader,
            top_panel_rect: top_panel_rect_calc(),
        }
    }
    pub fn new_with_path(pdfium: &'a Pdfium, path: &String) -> Self {
        let reader = ReadPart::new();
        let input = (String::from(path), InputState::WaitingVerify, String::new());
        Self {
            input,
            back_end: pdfium,
            reader,
            top_panel_rect: top_panel_rect_calc(),
        }
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
        let re_active = self.input.1.is_inactive() && verify_response.clicked();
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
    fn verify_input(&mut self) -> Option<PdfDocument<'a>> {
        // 在is_waiting的状态下验证输入是否合法
        // 如果合法则输出读取的pdfdocument，否则会显示警告信息。
        // 如果输出some，那么inputstate一定inactive
        if self.input.0.ends_with(".pdf") {
            match self.back_end.load_pdf_from_file(&self.input.0, None) {
                Ok(document) => {
                    self.input.1 = InputState::InActive;
                    Some(document)
                }
                Err(err) => {
                    self.input.1 = InputState::InActive;
                    self.input.2.clear();
                    self.input.2.insert_str(0, &err.to_string());
                    None
                }
            }
        } else {
            self.input.1 = InputState::InActive;
            self.input.2.clear();
            self.input.2.insert_str(0, "input pdf file full path");
            None
        }
    }
    fn back_empty(&mut self, scale: f32, disp_rect: Rect, read_mode: usize) {
        self.reader = ReadPart::Empty(Basic {
            scale,
            disp_rect,
            read_mode,
        })
    }
    fn show(&mut self, ui: &mut egui::Ui) {
        let total_size = ui.available_size();

        let (rem_size) = self.top_panel(ui, total_size);
        let dropped_file = ui.input_mut(|i| i.raw.dropped_files.pop());
        let document = if self.input.1.is_waiting() {
            self.verify_input()
        } else if let Some(file) = dropped_file
            && let Some(path) = file.path
            && let Some(path_str) = path.to_str()
        {
            self.input.0.clear();
            self.input.0.insert_str(0, path_str);
            self.input.1 = InputState::WaitingVerify;
            self.verify_input()
        } else {
            None
        };
        match &mut self.reader {
            ReadPart::Empty(basis) => {
                basis.show(ui, rem_size);
                if let Some(document) = document {
                    self.reader = ReadPart::Reader(Reader::new_with_document(
                        document,
                        basis.scale,
                        basis.disp_rect,
                        basis.read_mode,
                        &self.input.0,
                    ));
                }
            }
            ReadPart::Reader(reader) => {
                let action = reader.top_panel_button(ui, self.top_panel_rect.1);
                let view_action = reader.view_action.take();
                reader.view_action = view_action.or(action);
                reader.show(ui, rem_size);
                if let Some(document) = document {
                    let scale = reader.viewer.disp_scale;
                    let disp_rect = reader.viewer.disp_rect;
                    let read_mode = reader.viewer.line_num;
                    self.reader = ReadPart::Reader(Reader::new_with_document(
                        document,
                        scale,
                        disp_rect,
                        read_mode,
                        &self.input.0,
                    ));
                }
            }
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
    link_name: String,                    // 保存文件使用的文字
    hover_preview: HoverPreviewState<'a>, // 从前向后依次为preview的页码，link码，link的rect， 可持续时间，hw_ratio，image
    // 其中可持续时间是想解决如何很好的控制多点hover的问题。一个hover_preview设定为，如果pointer_pos不处在link_rect上，或者不处在preview上，那么将消失。hover in则重置该量，
    fixed_preview: HashMap<LinkIndex, (usize, Rect, Image<'a>)>,
    fix_preview_order: Vec<LinkIndex>,
    link: Vec<Vec<SpecialPageLink>>,
    ref_link: Vec<Vec<LinkContent>>,
    reloaded: bool,
    state: ReaderState,
    view_action: ViewAction,
    viewer: View<'a>,
    changed: bool,
    page_str: String,
    custom_flags: CustomFlag,
}
impl<'a> Reader<'a> {
    fn new_with_document(
        document: PdfDocument<'a>,
        scale: f32,
        disp_rect: Rect,
        read_mode: usize,
        file_name: &str,
    ) -> Self {
        let pages = document.pages();
        let page_num = pages.len() as usize;
        let mut link_file_name = String::new();
        link_file_name.insert_str(0, ".json");
        link_file_name.insert_str(0, file_name);
        let (link, ref_link) =
            match (|| -> Option<(Vec<Vec<SpecialPageLink>>, Vec<Vec<LinkContent>>)> {
                let mut buf = String::new();
                let mut file = File::open(&link_file_name).ok()?;
                file.read_to_string(&mut buf).ok()?;
                load_from_string(buf.as_str()).ok()
            })() {
                None => get_link_and_target(pages),
                Some(data) => data,
            };
        let viewer = View::new(document, disp_rect, scale);
        let action = match read_mode {
            1 => ViewAction::Col(1),
            2 => ViewAction::Col(2),
            _ => ViewAction::Col(page_num),
        };
        Self {
            link,
            ref_link,
            viewer,
            reloaded: false,
            state: Default::default(),
            hover_preview: HoverPreviewState::Empty,
            fixed_preview: HashMap::new(),
            fix_preview_order: Vec::new(),
            view_action: action,
            changed: false,
            link_name: link_file_name,
            custom_flags: Default::default(),
            page_str: String::new(),
        }
    }
    pub fn new_with_info(
        scale: f32,
        disp_rect: Rect,
        read_mode: usize,
        file_name: impl Into<String>,
        back_end: &'a Pdfium,
    ) -> Option<Self> {
        let file_name = file_name.into();
        let link_file_name: String = match file_name.rsplit_once('.') {
            None => None?,
            Some((prefix, _)) => Some(prefix.into())?,
        };
        let document = back_end.load_pdf_from_file(&file_name, None).ok()?;
        let pages = document.pages();
        let page_num = pages.len() as usize;
        let (link, ref_link) =
            match (|| -> Option<(Vec<Vec<SpecialPageLink>>, Vec<Vec<LinkContent>>)> {
                let mut buf = String::new();
                let mut file = File::open(&link_file_name).ok()?;
                file.read_to_string(&mut buf).ok()?;
                load_from_string(buf.as_str()).ok()
            })() {
                None => get_link_and_target(pages),
                Some(data) => data,
            };
        let viewer = View::new(document, disp_rect, scale);
        let action = match read_mode {
            1 => ViewAction::Col(1),
            2 => ViewAction::Col(2),
            _ => ViewAction::Col(page_num),
        };
        Some(Self {
            link,
            ref_link,
            viewer,
            reloaded: false,
            state: Default::default(),
            hover_preview: HoverPreviewState::Empty,
            fixed_preview: HashMap::new(),
            fix_preview_order: Vec::new(),
            view_action: action,
            changed: false,
            link_name: link_file_name,
            custom_flags: Default::default(),
            page_str: String::new(),
        })
    }
    pub fn new(
        file_name: impl Into<String>,
        back_end: &'a Pdfium,
        link_file_name: Option<&String>,
    ) -> Option<Self> {
        let document = back_end.load_pdf_from_file(&file_name.into(), None).ok()?;
        let pages = document.pages();
        let (link, ref_link) = match link_file_name {
            None => get_link_and_target(pages),
            Some(link_file_name) => {
                match (|| -> Option<(Vec<Vec<SpecialPageLink>>, Vec<Vec<LinkContent>>)> {
                    let mut buf = String::new();
                    let mut file = File::open(link_file_name).ok()?;
                    file.read_to_string(&mut buf).ok()?;
                    load_from_string(buf.as_str()).ok()
                })() {
                    None => get_link_and_target(pages),
                    Some(data) => data,
                }
            }
        };
        let viewer = View::new(
            document,
            Rect::from_min_max(pos2(0., 0.), pos2(1000., 1000.)),
            1.,
        );
        Some(Self {
            link,
            ref_link,
            viewer,
            reloaded: false,
            state: Default::default(),
            hover_preview: HoverPreviewState::Empty,
            fixed_preview: HashMap::new(),
            fix_preview_order: Vec::new(),
            view_action: ViewAction::Rescale(1.),
            changed: false,
            link_name: String::new(),
            custom_flags: Default::default(),
            page_str: String::new(),
        })
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
    fn preview_reload(&mut self, ui: &mut egui::Ui) {
        // 如果存在某一页因为rescale 重新加载了，那么所有的preview重新获取image
        for (target_idx, (_, _, img)) in self.fixed_preview.iter_mut() {
            let ref_link = &self.ref_link[target_idx.page_idx][target_idx.inner_idx];
            *img = self
                .viewer
                .required_clip_content(
                    target_idx.page_idx,
                    ref_link.content_rect,
                    ref_link.disp_size,
                    ui.ctx(),
                )
                .1;
        }
        match &mut self.hover_preview {
            HoverPreviewState::Instance(target_idx, _, (_, img)) => {
                let ref_link = &self.ref_link[target_idx.page_idx][target_idx.inner_idx];
                *img = self
                    .viewer
                    .required_clip_content(
                        target_idx.page_idx,
                        ref_link.content_rect,
                        ref_link.disp_size,
                        ui.ctx(),
                    )
                    .1;
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
                        self.viewer.disp_rect,
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
                        self.viewer.disp_rect,
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
        let view_page_info = &self.viewer.view_page_info;
        let disp_rect = self.viewer.disp_rect;
        let disp_lt = disp_rect.left_top();
        let disp_size = disp_rect.size();
        let painter = ui.painter_at(disp_rect);
        for (idx, (pidx, img_uv, pos_uv)) in view_page_info.iter().enumerate() {
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
                        let new_hover_preview = self.viewer.required_clip_content(
                            page_idx,
                            ref_link.content_rect,
                            ref_link.disp_size,
                            ui.ctx(),
                        );
                        self.hover_preview =
                            HoverPreviewState::Instance(target_idx, hover_rect, new_hover_preview);
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
                        let page_idx = self.viewer.page_info.0;
                        let page_count = self.viewer.page_num;
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
                let page_idx = self.viewer.page_info.0;
                let page_count = self.viewer.page_num;
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
            RichText::new(format!("{:.1}%", self.viewer.disp_scale * 100.))
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
            let (page_idx, _) = self.viewer.page_info;
            action = action.or(ViewAction::Jump(page_idx.max(1) - 1, Jump::Footer));
        }
        if right_response.clicked() {
            let (page_idx, _) = self.viewer.page_info;
            let max_page_idx = self.viewer.max_page_index();
            action = action.or(ViewAction::Jump(
                (page_idx + 1).min(max_page_idx),
                Jump::Header,
            ));
        }
        if larger_response.clicked() {
            let disp_scale = self.viewer.disp_scale;
            action = action.or(ViewAction::Rescale((disp_scale * 1.25).max(MIN_SCALE)));
        }
        if reduce_response.clicked() {
            let disp_scale = self.viewer.disp_scale;
            action = action.or(ViewAction::Rescale((disp_scale * 0.8).max(MIN_SCALE)));
        }
        if single_response.clicked() {
            action = action.or(ViewAction::Col(1));
        }
        if double_response.clicked() {
            action = action.or(ViewAction::Col(2));
        }
        if single_line_response.clicked() {
            action = action.or(ViewAction::Col(self.viewer.page_num));
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
        let disp_rect = self.viewer.disp_rect;
        let mut old_action = self.view_action.take();
        if disp_rect.size() != available_size {
            old_action = old_action.or(ViewAction::ReSize(available_size));
            ui.request_repaint();
        }
        ui.painter()
            .rect_filled(disp_rect, CornerRadius::same(0), Color32::GRAY);

        // 主视图绘制
        let pos_change = !old_action.is_empty();
        let reload = self.viewer.show_with_action(old_action, ui);
        if pos_change && !self.custom_flags.page_num_input {
            self.page_str.clear();
            let page_idx = self.viewer.page_info.0;
            let page_count = self.viewer.page_num;
            fmt::write(
                &mut self.page_str,
                format_args!("{}/{}", (page_idx + 1).min(page_count), page_count),
            );
        }
        self.reloaded |= reload;

        let mut action = ViewAction::Empty;

        let mut detected_hover = true;
        // 主视图动作捕捉
        let disp_response = ui.allocate_rect(self.viewer.disp_rect, Sense::drag());
        if disp_response.dragged() {
            action = action.or(ViewAction::Shift(disp_response.drag_delta() * -1.));
            detected_hover &= false;
        }
        action = action.or({
            let zoom_delta = ui.input(|i| i.zoom_delta());
            let scroll = ui.input(|i| i.smooth_scroll_delta());
            if zoom_delta != 1.0 {
                detected_hover &= false;
                ViewAction::Rescale((self.viewer.disp_scale * zoom_delta).max(MIN_SCALE))
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

        // 重新加载preview
        if self.reloaded {
            self.preview_reload(ui);
            self.reloaded = false;
        }

        match &mut self.state {
            ReaderState::Read => {
                if self.viewer.disp_scale > 0.5 {
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
                        let ith_view_page_info = &self.viewer.view_page_info[idx];
                        let disp_size = self.viewer.disp_rect.size();
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
    image: &mut Image,
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

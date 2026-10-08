use float_reader::multi_image::{GetImgInfo, ImageShow, Images2DCursor, MultiImage};
use std::vec::Vec;
fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1500.0, 1040.0])
            // .with_max_inner_size([1500.0, 1040.0])
            .with_min_inner_size([1500.0, 1040.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Multi Image Viewer Test",
        options,
        Box::new(|cc| Ok(Box::new(MultiImageTest::new(cc)))),
    )
    .unwrap();
}
use std::rc::Rc;
struct ImgArray {
    vec: Rc<Vec<(TextureHandle, Vec2)>>,
}
impl GetImgInfo for ImgArray {
    fn get_img_number(self: &Self) -> usize {
        self.vec.len()
    }
    fn get_img_size(self: &Self, pix: usize) -> Vec2 {
        self.vec[pix].1
    }
    fn get_min_outer_size(self: &Self) -> Vec2 {
        let mut vec = Vec2::ZERO;
        for i in 0..self.vec.len() {
            vec = vec.max(self.vec[i].1)
        }
        vec
    }
}
use egui::{
    Button, ColorImage, Image, Pos2, Rect, TextureHandle, TextureOptions, Vec2, pos2, vec2,
};
use std::fs::File;
use std::io::BufReader;
pub struct MultiImageTest<'b> {
    img_vec: ImgArray,
    disp_rect: Rect,
    disp_scale: f32,
    center_pos: Vec2,
    img_disp: Option<MultiImage<'b>>,
}
use eframe::CreationContext;
use image::{self, RgbaImage};
impl<'b> MultiImageTest<'b> {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        let img_1_name = "test_file/chatgpt_image_generate_sample.png";
        let img_2_name = "test_file/chatgpt_image_generate_sample.png";
        let img_3_name = "test_file/chatgpt_image_generate_sample.png";
        let img_1 = RgbaImage::from(
            image::load(
                &mut BufReader::new(File::open(img_1_name).unwrap()),
                image::ImageFormat::Png,
            )
            .unwrap(),
        );
        let img_1_dim = img_1.dimensions();
        let img_1_size = vec2(img_1_dim.0 as f32, img_1_dim.1 as f32);
        let color_image_1 = ColorImage::from_rgba_unmultiplied(
            [img_1_dim.0 as usize, img_1_dim.1 as usize],
            img_1.as_raw().as_slice(),
        );

        let img_2 = RgbaImage::from(
            image::load(
                &mut BufReader::new(File::open(img_2_name).unwrap()),
                image::ImageFormat::Png,
            )
            .unwrap(),
        );
        let img_2_dim = img_2.dimensions();
        let img_2_size = vec2(img_2_dim.0 as f32, img_2_dim.1 as f32);
        let color_image_2 = ColorImage::from_rgba_unmultiplied(
            [img_2_dim.0 as usize, img_2_dim.1 as usize],
            img_2.as_raw().as_slice(),
        );

        let img_3 = RgbaImage::from(
            image::load(
                &mut BufReader::new(File::open(img_3_name).unwrap()),
                image::ImageFormat::Png,
            )
            .unwrap(),
        );
        let img_3_dim = img_3.dimensions();
        let img_3_size = vec2(img_3_dim.0 as f32, img_3_dim.1 as f32);
        let color_image_3 = ColorImage::from_rgba_unmultiplied(
            [img_3_dim.0 as usize, img_3_dim.1 as usize],
            img_3.as_raw().as_slice(),
        );

        let handle_1 = cc
            .egui_ctx
            .load_texture("1", color_image_1, TextureOptions::default());
        let handle_2 = cc
            .egui_ctx
            .load_texture("2", color_image_2, TextureOptions::default());
        let handle_3 = cc
            .egui_ctx
            .load_texture("3", color_image_3, TextureOptions::default());

        let img_array = vec![
            (handle_1, img_1_size),
            (handle_2, img_2_size),
            (handle_3, img_3_size),
        ];
        Self {
            img_vec: ImgArray {
                vec: Rc::new(img_array),
            },
            disp_rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(1000.)),
            disp_scale: 2.,
            center_pos: Vec2::ZERO,
            img_disp: None,
        }
    }
}
use eframe;

impl<'b> eframe::App for MultiImageTest<'b> {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let mut change = false;
        match &self.img_disp {
            None => {
                change = true;
            }
            Some(img_disp) => img_disp.show(ui),
        }
        let rect = self.disp_rect;
        let line_num = 2;
        let mut img_mat = Images2DCursor::new_with_center_pos(
            line_num,
            self.disp_scale,
            rect,
            &self.img_vec,
            self.center_pos,
        );

        let up_button = Button::new("↑");
        let down_button = Button::new("↓");
        let left_button = Button::new("←");
        let right_button = Button::new("→");

        let scale_larger_button = Button::new("🔍+");
        let scale_reduce_button = Button::new("🔍-"); // 1f50d

        let jump_2rd_center_button = Button::new("🖻2"); //1f5bb
        let jump_1st_quarter_button = Button::new("🖻1");

        let center = pos2(1200., 500.);
        let button_size = Vec2::splat(40.);
        let up_rect = Rect::from_center_size(center - vec2(0., 40.), button_size);
        let left_rect = Rect::from_center_size(center - vec2(40., 0.), button_size);
        let right_rect = Rect::from_center_size(center + vec2(40., 0.), button_size);
        let down_rect = Rect::from_center_size(center + vec2(0., 40.), button_size);

        let up_response = ui.put(up_rect, up_button.min_size(button_size));
        let left_response = ui.put(left_rect, left_button.min_size(button_size));
        let right_response = ui.put(right_rect, right_button.min_size(button_size));
        let down_response = ui.put(down_rect, down_button.min_size(button_size));
        if up_response.clicked() {
            img_mat.move_center(vec2(0., -100.));
            change = true;
        }
        if down_response.clicked() {
            img_mat.move_center(vec2(0., 100.));
            change = true;
        }
        if left_response.clicked() {
            img_mat.move_center(vec2(-100., 0.));
            change = true;
        }
        if right_response.clicked() {
            img_mat.move_center(vec2(100., 0.));
            change = true;
        }

        let center = pos2(1400., 500.);
        let larger_rect = Rect::from_center_size(center - vec2(50., 0.), vec2(60., 40.));
        let reduce_rect = Rect::from_center_size(center + vec2(50., 0.), vec2(60., 40.));
        let larger_response = ui.put(larger_rect, scale_larger_button.min_size(vec2(60., 40.)));
        let reduce_response = ui.put(reduce_rect, scale_reduce_button.min_size(vec2(60., 40.)));

        let center = pos2(1400., 600.);
        let jump_1st_rect = Rect::from_center_size(center - vec2(50., 0.), vec2(60., 40.));
        let jump_2rd_rect = Rect::from_center_size(center + vec2(50., 0.), vec2(60., 40.));
        let jump_1st_response = ui.put(
            jump_1st_rect,
            jump_1st_quarter_button.min_size(vec2(60., 40.)),
        );
        let jump_2rd_response = ui.put(
            jump_2rd_rect,
            jump_2rd_center_button.min_size(vec2(60., 40.)),
        );

        if larger_response.clicked() {
            change = true;
            img_mat.update_rescale(img_mat.disp_scale * 1.25, &mut self.img_vec);
        }
        if reduce_response.clicked() {
            change = true;
            img_mat.update_rescale(img_mat.disp_scale * 0.8, &mut self.img_vec);
        }
        if jump_1st_response.clicked() {
            change = true;
            img_mat.jump_to_special_img_pos(0, vec2(-0.25, -0.25), &mut self.img_vec);
            println!(
                "jump to 1st image with center as 1/4 diag, center_pos {}",
                img_mat.view_center
            );
        }
        if jump_2rd_response.clicked() {
            change = true;
            img_mat.jump_to_special_img_pos(1, vec2(0., 0.), &mut self.img_vec);
            println!(
                "jump to the center of 2rd image, center_pos {}",
                img_mat.view_center
            );
        }

        if change {
            let related_page_info = img_mat.get_multi_image(&mut self.img_vec);
            let mut img_array = Vec::with_capacity(related_page_info.len());
            for (pidx, img_uv, pos_uv) in related_page_info.iter() {
                let img = Image::from_texture(&self.img_vec.vec[*pidx].0).uv(*img_uv);
                img_array.push((ImageShow::I(img), *pos_uv));
            }
            self.img_disp = Some(MultiImage {
                images: img_array,
                disp_rect: self.disp_rect,
            });
        }

        self.disp_scale = img_mat.disp_scale;
        self.center_pos = img_mat.view_center;
    }
}

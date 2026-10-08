use crate::link::{
    LinkContent, LinkIndex, SpecialPageLink, get_link_and_target, link_save_strnig,
    load_from_string,
};
use crate::view::PageRenderInfo;
use egui::{Vec2, vec2};
use pdfium_render::prelude::{PdfDocument, Pdfium};
use std::fs::File;
use std::io::Read;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
pub struct PagesInfo {
    pub size_vec: Vec<Vec2>,
    pub num: usize,
    pub min_outer_size: Vec2,
}
pub struct DocumentInfo {
    pub pages: PagesInfo,
    pub links: (Vec<Vec<SpecialPageLink>>, Vec<Vec<LinkContent>>),
    pub link_file_name: String,
}
pub struct BackEnd<'a> {
    backend: &'a Pdfium,
    document: Option<PdfDocument<'a>>,
    file_reciver: Receiver<String>,
    page_require_reciver: Receiver<SpecialPage>,
    file_state_sender: Sender<Option<DocumentInfo>>,
    page_render_sender: Sender<(PageRenderInfo, [usize; 2], Vec<u8>)>,
    end_reciver: Receiver<()>,
}
impl<'a> BackEnd<'a> {
    pub fn new(
        backend: &'a Pdfium,
        fr: Receiver<String>,
        cr: Receiver<SpecialPage>,
        fss: Sender<Option<DocumentInfo>>,
        prs: Sender<(PageRenderInfo, [usize; 2], Vec<u8>)>,
        er: Receiver<()>,
    ) -> Self {
        // let root = env!["CARGO_MANIFEST_DIR"];
        // let pdfium = Pdfium::new(
        //     Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&format!(
        //         "{}/asset",
        //         root
        //     )))
        //     .unwrap(),
        // );
        BackEnd {
            backend,
            document: None,
            file_reciver: fr,
            page_require_reciver: cr,
            file_state_sender: fss,
            page_render_sender: prs,
            end_reciver: er,
        }
    }
    fn open_file(&mut self, path: String) -> Option<PdfDocument<'a>> {
        match self.backend.load_pdf_from_file(&path, None) {
            Ok(document) => {
                let pages = document.pages();
                let page_num = pages.len() as usize;
                let mut vec = Vec::with_capacity(page_num);
                let mut outer_vec = Vec2::ZERO;
                for pidx in 0..page_num {
                    let rect = pages.get(pidx as i32).unwrap().page_size();
                    let width = rect.width().value;
                    let height = rect.height().value;
                    let vec_n = vec2(width, height);
                    outer_vec = outer_vec.max(vec_n);
                    vec.push(outer_vec);
                }
                let mut link_file_name = String::new();
                link_file_name.insert_str(0, ".json");
                link_file_name.insert_str(0, path.as_str());
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
                let document_info = DocumentInfo {
                    pages: PagesInfo {
                        size_vec: vec,
                        num: page_num,
                        min_outer_size: outer_vec,
                    },
                    links: (link, ref_link),
                    link_file_name,
                };
                println!("file {} opened!", path);
                self.file_state_sender.send(Some(document_info));
                Some(document)
            }
            Err(_) => {
                println!("file {} open failed!", path);
                self.file_state_sender.send(None);
                None
            }
        }
    }
    fn get_special_page_with_scale(&mut self, index: usize, load_ppp_scale: f32) {
        match &self.document {
            None => todo!(),
            Some(document) => {
                let page = document.pages().get(index as i32).unwrap();
                let page_width = page.page_size().width().value;
                let page_height = page.page_size().height().value;
                let page_size = vec2(page_width, page_height);
                let width = (page_size.x * load_ppp_scale).floor() as i32;
                let height = (page_size.y * load_ppp_scale).floor() as i32;
                let rgba_image = page.render(width, height, None).unwrap().as_rgba_bytes();
                self.page_render_sender.send((
                    PageRenderInfo {
                        page_idx: index,
                        page_size,
                        scale: load_ppp_scale,
                    },
                    [width as usize, height as usize],
                    rgba_image,
                ));
            }
        }
    }
    fn document_run(&mut self) {
        match self.file_reciver.try_recv() {
            Ok(file_path) => {
                while let Ok(_) = self.page_require_reciver.try_recv() {}
                self.document = self.open_file(file_path);
            }
            Err(_) => match self.page_require_reciver.try_recv() {
                Ok((index, load_ppp_scale)) => {
                    self.get_special_page_with_scale(index, load_ppp_scale)
                }
                _ => (),
            },
        }
    }
    pub fn run(&mut self) {
        //主循环，假定如果出现丢失，那么其他的reciver应该也为disconnected，所以，应该只有这里执行是说的过去的。
        while true {
            match self.end_reciver.try_recv() {
                Ok(()) | Err(TryRecvError::Disconnected) => {
                    break;
                }
                _ => self.document_run(),
            }
        }
    }
}
pub type SpecialPage = (usize, f32);
enum FileState {}
enum Command {
    RequireLink,
    SpecialPage(usize, f32),
}
enum CommandResult {
    FileState(bool),
}

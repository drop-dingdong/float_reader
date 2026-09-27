use egui::{Rect, Vec2, vec2};
use pdfium_render::prelude::{PdfDestinationViewSettings, PdfLink, PdfPages};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::{fmt, iter, vec};
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct LinkIndex {
    pub page_idx: usize,
    pub inner_idx: usize,
}
impl LinkIndex {
    fn new(pidx: usize, iidx: usize) -> Self {
        LinkIndex {
            page_idx: pidx,
            inner_idx: iidx,
        }
    }
}
#[derive(Debug)]
pub struct SpecialPageLink {
    pub click_rect: (Vec2, Vec2),
    pub target_index: LinkIndex,
}
impl SpecialPageLink {
    fn get_store(&self) -> SpecialPageLinkStore {
        let min = self.click_rect.0;
        let max = self.click_rect.1;
        SpecialPageLinkStore {
            click_rect: (
                (min.x.to_bits(), min.y.to_bits()),
                (max.x.to_bits(), max.y.to_bits()),
            ),
            target_page_idx: self.target_index.page_idx,
            target_inner_idx: self.target_index.inner_idx,
        }
    }
}
impl From<SpecialPageLinkStore> for SpecialPageLink {
    fn from(value: SpecialPageLinkStore) -> Self {
        let min = value.click_rect.0;
        let max = value.click_rect.1;
        Self {
            click_rect: (
                vec2(f32::from_bits(min.0), f32::from_bits(min.1)),
                vec2(f32::from_bits(max.0), f32::from_bits(max.1)),
            ),
            target_index: LinkIndex::new(value.target_page_idx, value.target_inner_idx),
        }
    }
}
#[derive(Debug, Deserialize, Serialize)]
struct SpecialPageLinkStore {
    click_rect: ((u32, u32), (u32, u32)),
    target_page_idx: usize,
    target_inner_idx: usize,
}
// 指的是有价值的link形式，
// 具有click rect，指向文章内部的某个地方
// 具有指向页的页标，具有指向位置
// scale 不知道具体有什么用，现在先不考虑
// // 当然pdf的坐标和egui的坐标系不同
#[derive(Debug)]
struct ContentLink {
    self_page_idx: usize,
    click_rect: (Vec2, Vec2),
    target_page_idx: usize,
    target: LinkContent,
}
impl ContentLink {
    fn from_pdf_link<'a>(
        page_idx: usize,
        pdf_link: PdfLink<'a>,
        page_size: (f32, f32),
        pages: &PdfPages<'a>,
    ) -> Option<Self> {
        let rect = pdf_link.rect().ok()?;
        let (width, height) = page_size;
        let click_rect = (
            rect.left().value,
            rect.right().value,
            rect.top().value,
            rect.bottom().value,
        ); // 由于pdfrect内部的坐标方向与egui的rect不同，导致top > bottom所以上述rect是非法的。当然也仅仅将其作为一个记录使用，不直接用来绘制图像。
        let destination = pdf_link.destination()?;
        let target_page_idx = destination.page_index().ok()? as usize;
        let target_page_size = pages.get(target_page_idx as i32).ok()?.page_size();
        let (target_width, target_height) = (
            target_page_size.width().value,
            target_page_size.height().value,
        );
        let view_mode = destination.view_settings().ok()?;
        let mut click_lt = vec2(click_rect.0 / width, 1. - click_rect.2 / height);
        let mut click_rb = vec2(click_rect.1 / width, 1. - click_rect.3 / height);
        click_lt = click_lt.min(vec2(1., 1.)).max(vec2(0., 0.));
        click_rb = click_rb.min(vec2(1., 1.)).max(vec2(0., 0.));
        match view_mode {
            PdfDestinationViewSettings::SpecificCoordinatesAndZoom(Some(x), Some(y), _) => {
                let mut target_lt = vec2(x.value / target_width, 1. - y.value / target_height);
                target_lt = target_lt.min(vec2(1., 1.)).max(vec2(0., 0.));
                let link_content = LinkContent {
                    content_rect: (target_lt, target_lt + Vec2::splat(0.05)),
                    disp_size: Vec2::splat(100.),
                    pos: target_lt,
                };
                Some(Self {
                    target: link_content,
                    target_page_idx,
                    self_page_idx: page_idx,
                    click_rect: (click_lt, click_rb),
                })
            }
            _ => None,
        }
    }
    fn full_from_pages<'a>(pages: &PdfPages<'a>) -> Vec<Self> {
        let acc = Vec::new();
        pages.iter().enumerate().fold(acc, |acc, (page_idx, page)| {
            let size = page.page_size();
            let size = (size.width().value, size.height().value);
            page.links().iter().fold(acc, |mut acc, pdf_link| {
                match ContentLink::from_pdf_link(page_idx, pdf_link, size, &pages) {
                    None => (),
                    Some(link) => {
                        acc.push(link);
                    }
                }
                acc
            })
        })
    }
    fn collect_as_page(
        content_links: Vec<Self>,
        page_num: usize,
    ) -> (Vec<Vec<SpecialPageLink>>, Vec<Vec<LinkContent>>) {
        let mut origin_vec: Vec<Vec<SpecialPageLink>> =
            iter::repeat_with(Vec::default).take(page_num).collect();
        let mut dest_vec: Vec<Vec<LinkContent>> =
            iter::repeat_with(Vec::default).take(page_num).collect();
        let mut hash_sets_vec: Vec<HashMap<(u32, u32), usize>> =
            iter::repeat_with(HashMap::default).take(page_num).collect();
        for ContentLink {
            self_page_idx,
            click_rect,
            target_page_idx,
            target,
        } in content_links.into_iter()
        {
            let hash_map = &mut hash_sets_vec[target_page_idx];
            let target_pos = target.pos;
            let hash_able = (target_pos.x.to_bits(), target_pos.y.to_bits());
            let len = hash_map.len();
            let res = hash_map.entry(hash_able).or_insert(len);
            if *res == len {
                dest_vec[target_page_idx].push(target);
            }
            let spl = SpecialPageLink {
                click_rect: click_rect,
                target_index: LinkIndex {
                    page_idx: target_page_idx,
                    inner_idx: *res,
                },
            };
            origin_vec[self_page_idx].push(spl);
        }

        (origin_vec, dest_vec)
    }
}
pub fn get_link_and_target(
    pages: &PdfPages<'_>,
) -> (Vec<Vec<SpecialPageLink>>, Vec<Vec<LinkContent>>) {
    let page_num = pages.len() as usize;
    let content_links = ContentLink::full_from_pages(pages);
    ContentLink::collect_as_page(content_links, page_num)
}
// 先不支持跨页索引
#[derive(Debug)]
pub struct LinkContent {
    pub content_rect: (Vec2, Vec2),
    pub disp_size: Vec2,
    pos: Vec2, // 解析给出的位置，不要变化
}
impl LinkContent {
    pub fn get_pos(&self) -> Vec2 {
        self.pos
    }
    fn get_store(&self) -> LinkContentStore {
        let content_rect = self.content_rect;
        let pos = self.pos;
        let disp_size = self.disp_size;
        LinkContentStore {
            content_rect: (
                (content_rect.0.x.to_bits(), content_rect.0.y.to_bits()),
                (content_rect.1.x.to_bits(), content_rect.1.y.to_bits()),
            ),
            disp_size: (disp_size.x.to_bits(), disp_size.y.to_bits()),
            pos: (pos.x.to_bits(), pos.y.to_bits()),
        }
    }
}
impl From<LinkContentStore> for LinkContent {
    fn from(value: LinkContentStore) -> Self {
        let content_rect = value.content_rect;
        let pos = value.pos;
        let disp_size = value.disp_size;
        Self {
            content_rect: (
                vec2(
                    f32::from_bits(content_rect.0.0),
                    f32::from_bits(content_rect.0.1),
                ),
                vec2(
                    f32::from_bits(content_rect.1.0),
                    f32::from_bits(content_rect.1.1),
                ),
            ),
            disp_size: vec2(f32::from_bits(disp_size.0), f32::from_bits(disp_size.1)),
            pos: vec2(f32::from_bits(pos.0), f32::from_bits(pos.1)),
        }
    }
}
#[derive(Debug, Deserialize, Serialize)]
struct LinkContentStore {
    pub content_rect: ((u32, u32), (u32, u32)),
    disp_size: (u32, u32),
    pos: (u32, u32), // 解析给出的位置，不要变化
}
#[derive(Debug, Deserialize, Serialize)]
struct Store {
    targets_store: Vec<Vec<LinkContentStore>>,
    links_store: Vec<Vec<SpecialPageLinkStore>>,
}
impl Store {
    fn get_data(self) -> (Vec<Vec<SpecialPageLink>>, Vec<Vec<LinkContent>>) {
        let Store {
            targets_store,
            links_store,
        } = self;
        (
            links_store
                .into_iter()
                .map(|v| v.into_iter().map(|spl_store| spl_store.into()).collect())
                .collect(),
            targets_store
                .into_iter()
                .map(|v| v.into_iter().map(|lc_store| lc_store.into()).collect())
                .collect(),
        )
    }
}
pub fn link_save_strnig(
    links: &Vec<Vec<SpecialPageLink>>,
    targets: &Vec<Vec<LinkContent>>,
) -> String {
    let mut links_store: Vec<Vec<SpecialPageLinkStore>> = Vec::with_capacity(links.len());
    for i in 0..links.len() {
        let links_i = &links[i];
        links_store.push(Vec::with_capacity(links_i.len()));
        let ls_i = &mut links_store[i];
        for link in links_i {
            ls_i.push(link.get_store())
        }
    }
    let mut targets_store: Vec<Vec<LinkContentStore>> = Vec::with_capacity(targets.len());
    for i in 0..targets.len() {
        let targets_i = &targets[i];
        targets_store.push(Vec::with_capacity(targets_i.len()));
        let ts_i = &mut targets_store[i];
        for target in targets_i {
            ts_i.push(target.get_store());
        }
    }
    json!(Store {
        links_store,
        targets_store
    })
    .to_string()
}
pub fn load_from_string(
    input: &str,
) -> Result<(Vec<Vec<SpecialPageLink>>, Vec<Vec<LinkContent>>), String> {
    let v: Store = serde_json::from_str(input).map_err(|err| format!("{}", err))?;
    Ok(v.get_data())
}

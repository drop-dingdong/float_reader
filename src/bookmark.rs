use pdfium_render::prelude::{PdfBookmark, PdfBookmarks};
//Item和 AnotherItem定义之间的区别在于前者要求必须同时有page_idx和title，否则不算数
#[derive(Debug)]
struct AnotherItem {
    page_idx: Option<usize>,
    title: Option<String>,
    children: Vec<AnotherItem>,
}
#[derive(Debug)]
pub struct Item {
    page_idx: usize,
    title: String,
    children: Vec<Item>,
}
impl AnotherItem {
    fn from_bookmarks(bookmarks: &PdfBookmarks) -> Vec<Self> {
        let first_top = bookmarks.root();
        match first_top {
            None => vec![],
            Some(first_top) => {
                let vec = vec![AnotherItem::from_bookmark(&first_top)];
                let vec = first_top.iter_siblings().fold(vec, |mut acc, bookmark| {
                    acc.push(AnotherItem::from_bookmark(&bookmark));
                    acc
                });
                vec
            }
        }
    }
    fn from_bookmark(bookmark: &PdfBookmark) -> Self {
        let children = bookmark
            .iter_direct_children()
            .map(|bookmark| AnotherItem::from_bookmark(&bookmark))
            .collect::<Vec<_>>();
        let title = bookmark.title();
        let page_idx = match bookmark.destination() {
            Some(destination) => destination.page_index().map(|x| x as usize).ok(),
            None => None,
        };
        Self {
            page_idx,
            title,
            children,
        }
    }
}
impl Item {
    pub fn full_from_bookmarks(bookmarks: &PdfBookmarks) -> Vec<Self> {
        let first_top = bookmarks.root();
        match first_top {
            None => vec![],
            Some(first_top) => {
                let res = match Item::full_from_bookmark(&first_top) {
                    Some(item) => vec![item],
                    None => vec![],
                };
                let res = first_top.iter_siblings().fold(res, |mut acc, bookmark| {
                    match Item::full_from_bookmark(&bookmark) {
                        None => acc,
                        Some(item) => {
                            acc.push(item);
                            acc
                        }
                    }
                });
                res
            }
        }
    }
    fn full_from_bookmark(bookmark: &PdfBookmark) -> Option<Self> {
        let children = bookmark
            .iter_direct_children()
            .map(|bookmark| Item::full_from_bookmark(&bookmark))
            .flatten()
            .collect::<Vec<_>>();
        let title = bookmark.title()?;
        let page_idx = match bookmark.destination() {
            Some(destination) => destination.page_index().map(|x| x as usize).ok(),
            None => None,
        }?;
        Some(Self {
            page_idx,
            title,
            children,
        })
    }
}

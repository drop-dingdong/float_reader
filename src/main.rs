use float_reader::constant::*;
use float_reader::reader::MyApp;
use pdfium_render::prelude::*;

fn main() -> Result<(), String> {
    let root = env!["CARGO_MANIFEST_DIR"];
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(INIT_UI_SIZE),
        ..Default::default()
    };
    let pdfium = Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&format!(
            "{}/asset",
            root
        )))
        .unwrap(),
    );
    let args: Vec<String> = std::env::args().collect();
    let app = match args.len() {
        1 => Some(MyApp::new(&pdfium)),
        2 => Some(MyApp::new_with_path(&pdfium, &args[1])),
        _ => {
            println!("至多接受pdf文件的路径");
            None
        }
    };
    match app {
        Some(app) => eframe::run_native("Float Reader", options, Box::new(|_| Ok(Box::new(app))))
            .map_err(|err| format!("{:?}", err)),
        None => Err("至多接受pdf文件的路径".to_string()),
    }
}

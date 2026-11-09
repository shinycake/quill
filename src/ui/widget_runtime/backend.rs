//! What the renderer's backend calls reach (`twlite::Backend`): one
//! widget's fonts, images and canvases, which the painter reads too.
use super::canvas::Canvases;
use super::fonts::Fonts;
use super::images::Images;

#[derive(Default)]
pub struct Resources {
    pub fonts: Fonts,
    pub images: Images,
    pub canvases: Canvases,
}

impl Resources {
    pub fn run_canvas(&mut self, id: i32, width: i32, height: i32, ops: &str) {
        let Self { fonts, images, canvases } = self;
        canvases.run(id, width, height, ops, fonts, images);
    }
}

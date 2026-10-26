//! The texture browser's view.

use crate::*;

impl ViewerApp {
    /// Decodes texture `id` and shows it in place of the 3D view.
    pub(crate) fn show_texture(&mut self, ctx: &egui::Context, id: u32) {
        let name = self
            .texture_names
            .iter()
            .find(|(i, _)| *i == id)
            .map_or_else(|| id.to_string(), |(_, n)| n.clone());
        match self.textures.load(id) {
            Ok(img) => {
                let size = [img.width as usize, img.height as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, &img.rgba);
                let handle = ctx.load_texture(&name, image, egui::TextureOptions::LINEAR);
                self.session.status = format!("texture {id}: {name}, {}x{}", size[0], size[1]);
                self.texture_view = Some(TextureView {
                    id,
                    name,
                    handle,
                    size,
                });
            }
            Err(e) => self.session.status = format!("texture {id}: {e}"),
        }
    }
}

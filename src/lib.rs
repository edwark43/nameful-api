use magick_rust::{magick_wand_genesis, MagickWand, PixelWand};
use std::sync::Once;

static START: Once = Once::new();

pub struct Render {
    skin_filepath: String,
    size: usize,
    old: bool,
    skin: MagickWand,
    render: MagickWand,
}

impl Render {
    pub fn new(skin_filepath: String, size: usize) -> Render {
        let skin = MagickWand::new();
        let _ = skin.read_image(&skin_filepath);

        let old = skin.get_image_height() == 32;

        let mut background = PixelWand::new();
        let _ = background.set_color("transparent");

        let render = MagickWand::new();
        let _ = render.new_image(16 * size, 32 * size, &background);
        Render { skin_filepath, size, old, skin, render }
    }

    pub fn render_body_part(&self, skin_box_sizes: [usize; 2], skin_box_offsets: [usize; 2], output_offsets: [usize; 2], old: bool) -> &Render {
        START.call_once(|| {
            magick_wand_genesis();
        });

        let _ = &self.skin.read_image(&self.skin_filepath);
        let _ = &self.skin.crop_image(skin_box_sizes[0], skin_box_sizes[1], skin_box_offsets[0] as isize, skin_box_offsets[1] as isize);
        let _ = &self.skin.resize_image(skin_box_sizes[0] * self.size, skin_box_sizes[1] * self.size, magick_rust::FilterType::Box);
        if old {
            let _ = &self.skin.flop_image();
        }

        let _ = &self.render.compose_images(&self.skin, magick_rust::CompositeOperator::Over, true, output_offsets[0] as isize * self.size as isize, output_offsets[1] as isize * self.size as isize);
        self
    }

    pub fn render_body(&self, render_type: String, armored: bool) -> &Render {
        let mut head: bool = false;
        let mut bust: bool = false;
        let crop: [usize; 4];

        let _ = &self.render_body_part([8, 8], [8, 8], [4, 0], false);

        match render_type.as_str() {
            "head" => {
                head = true;
                crop = [8 * &self.size, 8 * &self.size, 4 * &self.size, 0]
            }
            "bust" => {
                bust = true;
                crop = [self.render.get_image_width(), self.render.get_image_width(), 0, 0]
            }
            _ => {
                crop = [self.render.get_image_width(), self.render.get_image_height(), 0, 0]
            }
        }


        if !head {
            let _ = &self.render_body_part([8, 12], [20, 20], [4, 8], false)
            .render_body_part([4, 12], [44, 20], [0, 8], false);
            if self.old {
                let _ = &self.render_body_part([4, 12], [44, 20], [12, 8], true);
            } else {
                let _ = &self.render_body_part([4, 12], [36, 52], [12, 8], false);
            }
        }

        if !bust {
            let _ = &self.render_body_part([4, 12], [4, 20], [4, 20], false);

            if self.old {
                let _ = &self.render_body_part([4, 12], [4, 20], [8, 20], true);
            } else {
                let _ = &self.render_body_part([4, 12], [20, 52], [8, 20], false);
            }
        }

        if armored && !&self.old{
            let _ = &self.render_body_part([8, 8], [40, 8], [4, 0], false);
            
            if !head {
                let _ = &self.render_body_part([8, 12], [20, 36], [4, 8], false)
                .render_body_part([4, 12], [44, 36], [0, 8], false)
                .render_body_part([4, 12], [52, 52], [12, 8], false);
            }

            if !bust {
                let _ = &self.render_body_part([4, 12], [4, 36], [4, 20], false)
                .render_body_part([4, 12], [4, 52], [8, 20], false);
            }
        }
        let _ = &self.render.crop_image(crop[0], crop[1], crop[2] as isize, crop[3] as isize);
        &self
    }

    pub fn write_image(&self) -> () {
        let _ = self.render.write_image("render.png");
        ()
    }
}

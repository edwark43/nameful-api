use nameful_api_rs;

fn main() {
    nameful_api_rs::Render::new(String::from("skin.png"), 6)
        .render_body(String::from(""), true)
        .write_image();
}

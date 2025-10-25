use axum::response::Json;
use base64::{Engine, prelude::BASE64_STANDARD};
use image;
use magick_rust::{MagickWand, PixelWand, magick_wand_genesis};
use maxminddb::geoip2;
use reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{boxed::Box, error::Error, fs, io::Write, net::IpAddr, path, sync::Once};
use toml;
use xdg::BaseDirectories;

static START: Once = Once::new();

#[derive(Deserialize)]
pub struct Config {
    pub port: u16,
    pub token: String,
    pub maxmind_db: String,
    pub propaganda_path: String,
    pub cache_path: String,
    pub data_path: String,
}

impl Config {
    pub fn init() -> Result<(), Box<dyn Error>> {
        let error_base64 = "iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAMAAACdt4HsAAAAdVBMVEUAAAD///+qclmbY0mQWT+PXj6BUzl2SzM/KhUzJBErHg0kGAgmGgo6MYlBNZtGOqUFiIgElZUApKQAr68KvLw3Nzc/Pz9KSkpVVVUAzMyUYD5qQDB3QjVJJRBCHQooKCgAf38AaGg0JRIDenqzeV63g2tSPYnw8BGEAAAAAXRSTlMAQObYZgAAAo5JREFUSA3t1oWOM1cAQ+HPNymlzMx9/ycqV/Qz0eJ1GTKFya642iOy6MjXgwHYaVTOACcOYwugTRsEdSgDQMNgBjIvLkiqKcAlBDStQrnoBpVQYw7MXGKD0qSjKrlwg6CJ9B1cqwPJTiVVRUJaoZE2j1eP0CTaIaSdlaSYxLQmaKuzevL+hx8cn76bzppKlKwfIa1Rx/3M1+QZmZJWGvpk9QiTHB+nHV/ni/bnzGRExRrbInUUX77ry+RIUxrSyOoGIzk9Gpu3s3k/+WCzeXszjk6TAbFOnp465sgbN99peq3NHDMGT+Nk9bEOvIhn7XwHePdaon37IeAB4HV6r8x/uhN3HgOQEP9IJf8oeGxnlRSt1QYVrSVFut8gL2nCTEPMLUbvRl/JxKiSoMSLeIBbgK0CEIaKvHzPy6Ei0kAJoioAg0AA+vrrrbz6UvTnPFGmSnSCqOUGoPI6r99OJPpzvkORFwH5Ld/+q6AVaSaFNlBAgx9I04AurkJISyq3XneLBn7OCaFFtUnVQiAQCW6RqIZbIFSqyHKDIfYIKY0mBBRQEC2AbQUlBYQZUVJBtaSg9tjuPFawAyeBtkkJ1bQKNHRPMLMjRZvoeVPifDP0XIxUzweQghTAVhtUKUVEx2SKdHYzh7RRSdMQV/yfiAN5A2iRcNMVV6zcSC/iNY/dALwW7pBzLNmy5J8/96OtgwXLz710wuUbtKPNQRu8pInSCH0ND5QAt+wz/AOVUIgEoWg5TIAAMCtRlcM2yH6VvLyQJwC6FNAwBSb9AZqmIGfrDRqRIpVKSxV0fYMgARkjEJRILjCigIAC7fqIFRoUUCCFdcHOY1rswCMU6EGC5f8CSAHpqsDyfyENoqJiwY8icHkmoi9YwQAAAABJRU5ErkJggg==";
        let error: &[u8] = &BASE64_STANDARD.decode(error_base64)?;
        let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
        let config_path = xdg_dirs
            .place_config_file("config.toml")
            .expect("cannot create configuration directory");
        let cache_path = xdg_dirs
            .cache_home
            .clone()
            .unwrap()
            .into_os_string()
            .into_string()
            .unwrap()
            + "/nameful-api";
        let data_path = xdg_dirs
            .data_home
            .clone()
            .unwrap()
            .into_os_string()
            .into_string()
            .unwrap()
            + "/nameful-api";
        if xdg_dirs.find_config_file(&config_path) == None {
            let mut config_file = fs::File::create(&config_path)?;
            write!(
                &mut config_file,
                "port = 3568\ntoken = \"\"\nmaxmind_db = \"path/to/db\"\npropaganda_path = \"path/to/propaganda\"\ncache_path = \"{}\"\ndata_path = \"{}\"",
                cache_path, data_path
            )?;
        }
        let _ = fs::create_dir_all(format!("{}/", data_path));
        let _ = fs::create_dir_all(format!("{}/armor/head", cache_path));
        let _ = fs::create_dir_all(format!("{}/armor/bust", cache_path));
        let _ = fs::create_dir_all(format!("{}/armor/body", cache_path));
        let _ = fs::create_dir_all(format!("{}/armorless/head", cache_path));
        let _ = fs::create_dir_all(format!("{}/armorless/bust", cache_path));
        let _ = fs::create_dir_all(format!("{}/armorless/body", cache_path));
        let _ = fs::create_dir_all(format!("{}/skins", cache_path));
        if !fs::exists(format!("{}/skins/error.png", cache_path))? {
            let img = image::load_from_memory(&error)?;
            let _ = img.save(format!("{}/skins/error.png", cache_path))?;
        }
        Ok(())
    }
    pub fn new() -> Config {
        let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
        let config_path = xdg_dirs
            .place_config_file("config.toml")
            .expect("cannot create configuration directory");
        let content = fs::read_to_string(&config_path).unwrap();
        let config: Config = toml::from_str(&content).unwrap();
        config
    }
}

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
        Render {
            skin_filepath,
            size,
            old,
            skin,
            render,
        }
    }

    fn render_body_part(
        &self,
        skin_box_sizes: [usize; 2],
        skin_box_offsets: [usize; 2],
        output_offsets: [usize; 2],
        old: bool,
    ) -> &Render {
        START.call_once(|| {
            magick_wand_genesis();
        });

        let _ = &self.skin.read_image(&self.skin_filepath);
        let _ = &self.skin.crop_image(
            skin_box_sizes[0],
            skin_box_sizes[1],
            skin_box_offsets[0] as isize,
            skin_box_offsets[1] as isize,
        );
        let _ = &self.skin.resize_image(
            skin_box_sizes[0] * self.size,
            skin_box_sizes[1] * self.size,
            magick_rust::FilterType::Box,
        );
        if old {
            let _ = &self.skin.flop_image();
        }

        let _ = &self.render.compose_images(
            &self.skin,
            magick_rust::CompositeOperator::Over,
            true,
            output_offsets[0] as isize * self.size as isize,
            output_offsets[1] as isize * self.size as isize,
        );
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
                crop = [
                    self.render.get_image_width(),
                    self.render.get_image_width(),
                    0,
                    0,
                ]
            }
            _ => {
                crop = [
                    self.render.get_image_width(),
                    self.render.get_image_height(),
                    0,
                    0,
                ]
            }
        }

        if !head {
            let _ = &self
                .render_body_part([8, 12], [20, 20], [4, 8], false)
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

        if armored && !&self.old {
            let _ = &self.render_body_part([8, 8], [40, 8], [4, 0], false);

            if !head {
                let _ = &self
                    .render_body_part([8, 12], [20, 36], [4, 8], false)
                    .render_body_part([4, 12], [44, 36], [0, 8], false)
                    .render_body_part([4, 12], [52, 52], [12, 8], false);
            }

            if !bust {
                let _ = &self
                    .render_body_part([4, 12], [4, 36], [4, 20], false)
                    .render_body_part([4, 12], [4, 52], [8, 20], false);
            }
        }
        let _ = &self
            .render
            .crop_image(crop[0], crop[1], crop[2] as isize, crop[3] as isize);
        &self
    }

    pub fn write_image(&self, path: String) -> () {
        let _ = self.render.write_image(&path);
        ()
    }
}

pub fn read_json_from_file(path: String) -> Result<Value, Box<dyn Error>> {
    let file = fs::read_to_string(path)?;
    let json_object = serde_json::from_str::<serde_json::Value>(&file)?;

    Ok(json!(json_object))
}

pub fn json_at_key(path: String, key: String) -> Json<Value> {
    match read_json_from_file(path) {
        Ok(j) => Json(j.get(key).unwrap().clone()),
        Err(e) => Json(json!({"error":e.to_string()})),
    }
}

pub async fn download_skin(username: String, path: String) -> Result<(), Box<dyn Error>> {
    if fs::exists(&path)? {
        let metadata = fs::metadata(&path)?;
        if username == "error" {
            return Err("Username Disallowed".into());
        }
        if let Ok(time) = metadata.created() {
            let difference = time
                .duration_since(time)
                .expect("Something went horribly wrong.");
            if difference.as_secs() < 604800 {
                return Ok(());
            }
        }
    }
    let resp = reqwest::get(format!(
        "https://micro.os-mc.net/cosmetics/skin/{}",
        username
    ))
    .await?;
    match resp.error_for_status() {
        Ok(resp) => {
            if resp.headers()[CONTENT_TYPE] != "image/png" {
                return Err("Username Disallowed".into());
            }
            let body = resp.bytes().await?;
            let mut out = fs::File::create(&path)?;
            let _ = out.write_all(&body);
            Ok(())
        }
        Err(err) => {
            return Err(Box::new(err));
        }
    }
}

pub async fn read_json_from_url(url: String) -> Result<Value, Box<dyn Error>> {
    let resp = reqwest::get(url).await?;
    let text = resp.text().await?;
    let json_object = serde_json::from_str::<serde_json::Value>(&text)?;
    Ok(json!(json_object))
}

pub fn dir_to_json(path: String) -> Result<Value, Box<dyn Error>> {
    let dir = path::Path::new(&path);
    let mut result = vec![];
    if dir.is_dir() {
        for file in fs::read_dir(dir)? {
            let file = file?;
            let full_path = file.path();

            if full_path.is_file() {
                result.push(format!(
                    "{}",
                    full_path.file_name().unwrap().to_str().unwrap()
                ));
            }
        }
    }
    Ok(json!(result))
}

pub async fn get_nickname(config: Config, username: &String) -> Result<String, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let resp = client.get(format!("https://micro.os-mc.net/profile_service/ess/{}", username))
        .header(HeaderName::from_lowercase(b"content-type").unwrap(), HeaderValue::from_str("application/json").unwrap())
        .body(format!("{{\"token\":\"{}\"}}", config.token))
        .send()
        .await?;
    let json_object = serde_json::from_str::<serde_json::Value>(&resp.text().await?)?;
    match json!(json_object).get("nickname") {
        Some(j) => Ok(j.as_str().unwrap_or_else(|| username).to_string()),
        None => Ok(username.as_str().to_string())
    }
}

pub fn get_geoip_data(ip: IpAddr) -> Result<Value, Box<dyn Error>> {
    let reader = maxminddb::Reader::open_readfile("/etc/nginx/GeoIP2/GeoLite2-City.mmdb")?;
    let city = reader.lookup::<geoip2::City>(ip).unwrap().unwrap();
    Ok(json!(city))
}

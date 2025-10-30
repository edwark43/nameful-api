use base64::{Engine, prelude::BASE64_STANDARD};
use image;
use magick_rust::{MagickWand, PixelWand, magick_wand_genesis};
use maxminddb::geoip2;
use reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{boxed::Box, error::Error, fs, io::Write, net::IpAddr, path::PathBuf, sync::Once};
use toml;
use xdg::BaseDirectories;

static START: Once = Once::new();

#[derive(Deserialize, Debug)]
pub struct Config {
    pub port: u16,
    pub token: String,
    pub propaganda_path: PathBuf,
    pub online_url: String,
}

impl Config {
    pub async fn init() -> Result<(), Box<dyn Error>> {
        let error_base64 = "iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAMAAACdt4HsAAAAdVBMVEUAAAD///+qclmbY0mQWT+PXj6BUzl2SzM/KhUzJBErHg0kGAgmGgo6MYlBNZtGOqUFiIgElZUApKQAr68KvLw3Nzc/Pz9KSkpVVVUAzMyUYD5qQDB3QjVJJRBCHQooKCgAf38AaGg0JRIDenqzeV63g2tSPYnw8BGEAAAAAXRSTlMAQObYZgAAAo5JREFUSA3t1oWOM1cAQ+HPNymlzMx9/ycqV/Qz0eJ1GTKFya642iOy6MjXgwHYaVTOACcOYwugTRsEdSgDQMNgBjIvLkiqKcAlBDStQrnoBpVQYw7MXGKD0qSjKrlwg6CJ9B1cqwPJTiVVRUJaoZE2j1eP0CTaIaSdlaSYxLQmaKuzevL+hx8cn76bzppKlKwfIa1Rx/3M1+QZmZJWGvpk9QiTHB+nHV/ni/bnzGRExRrbInUUX77ry+RIUxrSyOoGIzk9Gpu3s3k/+WCzeXszjk6TAbFOnp465sgbN99peq3NHDMGT+Nk9bEOvIhn7XwHePdaon37IeAB4HV6r8x/uhN3HgOQEP9IJf8oeGxnlRSt1QYVrSVFut8gL2nCTEPMLUbvRl/JxKiSoMSLeIBbgK0CEIaKvHzPy6Ei0kAJoioAg0AA+vrrrbz6UvTnPFGmSnSCqOUGoPI6r99OJPpzvkORFwH5Ld/+q6AVaSaFNlBAgx9I04AurkJISyq3XneLBn7OCaFFtUnVQiAQCW6RqIZbIFSqyHKDIfYIKY0mBBRQEC2AbQUlBYQZUVJBtaSg9tjuPFawAyeBtkkJ1bQKNHRPMLMjRZvoeVPifDP0XIxUzweQghTAVhtUKUVEx2SKdHYzh7RRSdMQV/yfiAN5A2iRcNMVV6zcSC/iNY/dALwW7pBzLNmy5J8/96OtgwXLz710wuUbtKPNQRu8pInSCH0ND5QAt+wz/AOVUIgEoWg5TIAAMCtRlcM2yH6VvLyQJwC6FNAwBSb9AZqmIGfrDRqRIpVKSxV0fYMgARkjEJRILjCigIAC7fqIFRoUUCCFdcHOY1rswCMU6EGC5f8CSAHpqsDyfyENoqJiwY8icHkmoi9YwQAAAABJRU5ErkJggg==";
        let error = &BASE64_STANDARD.decode(error_base64)?;
        let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
        let config_path = xdg_dirs
            .place_config_file("config.toml")
            .expect("cannot create configuration directory");
        let maxmind_db_path = xdg_dirs
            .place_data_file("GeoLite2-City.mmdb")
            .expect("cannot create maxmind db");
        let fallback_path = xdg_dirs
            .place_cache_file("skins/.fallback.png")
            .expect("cannot create fallback skin");
        if xdg_dirs.find_config_file(&config_path) == None {
            let mut config_file = fs::File::create(&config_path)?;
            write!(
                &mut config_file,
                "port = 3568\ntoken = \"\"\npropaganda_path = \"path/to/propaganda\"\n\"online_url\" = \"http://127.0.0.1:PORT\"",
            )?;
        }
        if xdg_dirs.find_data_file(&maxmind_db_path) == None {
            println!("Downloading MaxMind GeoLite2 DB");
            let mut db_file = fs::File::create(&maxmind_db_path)?;
            let resp = reqwest::get(
                "https://github.com/P3TERX/GeoLite.mmdb/raw/download/GeoLite2-City.mmdb",
            )
            .await?;
            let body = resp.bytes().await?;
            let _ = db_file.write_all(&body);
            println!("Finished Downloading");
        }
        if xdg_dirs.find_cache_file(&fallback_path) == None {
            let img = image::load_from_memory(&error)?;
            let _ = img.save(&fallback_path)?;
        }
        Ok(())
    }
    pub fn new() -> Config {
        let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
        let config_path = xdg_dirs
            .find_config_file("config.toml")
            .expect("coudn't find data.json");
        let content = fs::read_to_string(&config_path).unwrap();
        let config = toml::from_str(&content).unwrap();
        config
    }
}

pub struct Render {
    skin_filepath: PathBuf,
    size: usize,
    old: bool,
    skin: MagickWand,
    render: MagickWand,
}

impl Render {
    pub fn new(skin_filepath: PathBuf, size: usize) -> Render {
        let skin = MagickWand::new();
        let _ = skin.read_image(skin_filepath.to_str().unwrap());

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

        let _ = &self.skin.read_image(&self.skin_filepath.to_str().unwrap());
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

    pub fn render_body(&self, render_type: &str, armored: bool) -> &Render {
        let mut head = false;
        let mut bust = false;
        let crop: [usize; 4];

        let _ = &self.render_body_part([8, 8], [8, 8], [4, 0], false);

        match render_type {
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

    pub fn write_image(&self, path: &PathBuf) -> () {
        let _ = self.render.write_image(path.to_str().unwrap());
        ()
    }
}

pub fn read_json_from_file(path: PathBuf) -> Value {
    let file = match fs::read_to_string(path) {
        Ok(p) => p,
        Err(e) => return json!({"error":e.to_string()}),
    };
    let json_object = match serde_json::from_str(&file) {
        Ok(j) => j,
        Err(e) => json!({"error":e.to_string()}),
    };

    json_object
}

pub fn get_value_from_key_path(json: Value, key_path: Vec<&str>) -> Value {
    let mut value: &Value = &json;
    for key in key_path.into_iter().filter(|s| *s != "") {
        if let Ok(n) = key.parse::<usize>() {
            if let Some(j) = value.get(n) {
                value = j
            } else {
                return json!({"error":"DNE"});
            }
        } else {
            if let Some(j) = value.get(key) {
                value = j
            } else {
                return json!({"error":"DNE"});
            }
        }
    }
    value.clone()
}

fn _write_json_to_file(json: Value, path: PathBuf) -> Result<(), Box<dyn Error>> {
    let mut out = fs::File::create(path)?;
    write!(&mut out, "{}", serde_json::to_string(&json).unwrap())?;
    Ok(())
}

pub async fn download_skin(username: &str) -> Result<PathBuf, Box<dyn Error>> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let skin_path = xdg_dirs
        .place_cache_file(format!("skins/{}.png", username))
        .expect("cannot create skin");
    if xdg_dirs.find_cache_file(&skin_path) != None {
        let metadata = fs::metadata(&skin_path)?;
        if let Ok(time) = metadata.created() {
            let difference = time
                .duration_since(time)
                .expect("Something went horribly wrong.");
            if difference.as_secs() < 604800 {
                return Ok(skin_path);
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
            let mut out = fs::File::create(&skin_path)?;
            let _ = out.write_all(&body);
            Ok(skin_path)
        }
        Err(err) => {
            return Err(Box::new(err));
        }
    }
}

pub async fn read_json_from_url(url: String) -> Result<Value, Box<dyn Error>> {
    let resp = reqwest::get(url).await?;
    let text = resp.text().await?;
    let json_object = serde_json::from_str(&text)?;
    Ok(json_object)
}

pub fn dir_to_json(path: PathBuf) -> Result<Value, Box<dyn Error>> {
    let mut result = vec![];
    if path.is_dir() {
        for file in fs::read_dir(path)? {
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

pub async fn get_nickname(config: Config, username: &str) -> Result<String, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "https://micro.os-mc.net/profile_service/ess/{}",
            username
        ))
        .header(
            HeaderName::from_lowercase(b"content-type").unwrap(),
            HeaderValue::from_str("application/json").unwrap(),
        )
        .body(format!("{{\"token\":\"{}\"}}", config.token))
        .send()
        .await?;
    let json_object: Value = serde_json::from_str(&resp.text().await?)?;
    match json_object.get("nickname") {
        Some(j) => Ok(j.as_str().unwrap_or_else(|| username).to_string()),
        None => Ok(username.to_string()),
    }
}

pub fn get_geoip_data(ip: IpAddr, db: PathBuf) -> Result<Value, Box<dyn Error>> {
    let reader = maxminddb::Reader::open_readfile(db)?;
    let city: geoip2::City = reader.lookup(ip).unwrap().unwrap();
    Ok(json!(city))
}

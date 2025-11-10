use api_key::{
    self,
    types::{ApiKeyResults, Default, StringGenerator},
};
use base64::{Engine, prelude::BASE64_STANDARD};
use chrono::{DateTime, Utc};
use image;
use magick_rust::{MagickWand, PixelWand, magick_wand_genesis};
use maxminddb::geoip2;
use reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    boxed::Box,
    error::Error,
    fs,
    io::{Read, Write},
    net::{IpAddr, TcpStream},
    path::PathBuf,
    sync::Once,
    time::SystemTime,
};
use toml;
use xdg::BaseDirectories;

static START: Once = Once::new();

#[derive(Deserialize)]
pub struct Config {
    pub api_key: String,
    pub port: u16,
    pub osm_token: String,
    pub propaganda_path: PathBuf,
}

impl Config {
    pub async fn init() -> Result<(), Box<dyn Error>> {
        let error_base64 = "iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAMAAACdt4HsAAAAdVBMVEUAAAD///+qclmbY0mQWT+PXj6BUzl2SzM/KhUzJBErHg0kGAgmGgo6MYlBNZtGOqUFiIgElZUApKQAr68KvLw3Nzc/Pz9KSkpVVVUAzMyUYD5qQDB3QjVJJRBCHQooKCgAf38AaGg0JRIDenqzeV63g2tSPYnw8BGEAAAAAXRSTlMAQObYZgAAAo5JREFUSA3t1oWOM1cAQ+HPNymlzMx9/ycqV/Qz0eJ1GTKFya642iOy6MjXgwHYaVTOACcOYwugTRsEdSgDQMNgBjIvLkiqKcAlBDStQrnoBpVQYw7MXGKD0qSjKrlwg6CJ9B1cqwPJTiVVRUJaoZE2j1eP0CTaIaSdlaSYxLQmaKuzevL+hx8cn76bzppKlKwfIa1Rx/3M1+QZmZJWGvpk9QiTHB+nHV/ni/bnzGRExRrbInUUX77ry+RIUxrSyOoGIzk9Gpu3s3k/+WCzeXszjk6TAbFOnp465sgbN99peq3NHDMGT+Nk9bEOvIhn7XwHePdaon37IeAB4HV6r8x/uhN3HgOQEP9IJf8oeGxnlRSt1QYVrSVFut8gL2nCTEPMLUbvRl/JxKiSoMSLeIBbgK0CEIaKvHzPy6Ei0kAJoioAg0AA+vrrrbz6UvTnPFGmSnSCqOUGoPI6r99OJPpzvkORFwH5Ld/+q6AVaSaFNlBAgx9I04AurkJISyq3XneLBn7OCaFFtUnVQiAQCW6RqIZbIFSqyHKDIfYIKY0mBBRQEC2AbQUlBYQZUVJBtaSg9tjuPFawAyeBtkkJ1bQKNHRPMLMjRZvoeVPifDP0XIxUzweQghTAVhtUKUVEx2SKdHYzh7RRSdMQV/yfiAN5A2iRcNMVV6zcSC/iNY/dALwW7pBzLNmy5J8/96OtgwXLz710wuUbtKPNQRu8pInSCH0ND5QAt+wz/AOVUIgEoWg5TIAAMCtRlcM2yH6VvLyQJwC6FNAwBSb9AZqmIGfrDRqRIpVKSxV0fYMgARkjEJRILjCigIAC7fqIFRoUUCCFdcHOY1rswCMU6EGC5f8CSAHpqsDyfyENoqJiwY8icHkmoi9YwQAAAABJRU5ErkJggg==";
        let error = &BASE64_STANDARD.decode(error_base64)?;
        let options = StringGenerator {
            length: 24,
            prefix: String::from("nmfl"),
            ..StringGenerator::default()
        };
        let ApiKeyResults::String(key) = api_key::string(options) else {
            return Err("could not generate api key".into());
        };
        let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
        let config_path = xdg_dirs
            .place_config_file("config.toml")?;
        let maxmind_db_path = xdg_dirs
            .place_data_file("GeoLite2-City.mmdb")?;
        let fallback_path = xdg_dirs
            .place_cache_file("skins/.fallback.png")?;
        if xdg_dirs.find_config_file(&config_path) == None {
            let mut config_file = fs::File::create(&config_path)?;
            write!(
                &mut config_file,
                "api_key = \"{}\"\nport = 3568\nosm_token = \"\"\npropaganda_path = \"path/to/propaganda\"",
                key
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
            db_file.write_all(&body)?;
            println!("Finished Downloading");
        }
        if xdg_dirs.find_cache_file(&fallback_path) == None {
            let img = image::load_from_memory(&error)?;
            img.save(&fallback_path)?;
        }
        Ok(())
    }
    pub fn new() -> Result<Config, Box<dyn Error>> {
        let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
        let config_path = xdg_dirs
            .find_config_file("config.toml")
            .ok_or("could not find config toml")?;
        let content = fs::read_to_string(&config_path)?;
        Ok(toml::from_str(&content)?)
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
    pub fn new(skin_filepath: String, size: usize) -> Result<Render, Box<dyn Error>> {
        let skin = MagickWand::new();
        skin.read_image(&skin_filepath)?;

        let old = skin.get_image_height() == 32;

        let mut background = PixelWand::new();
        background.set_color("transparent")?;

        let render = MagickWand::new();
        render.new_image(16 * size, 32 * size, &background)?;
        Ok(Render {
            skin_filepath,
            size,
            old,
            skin,
            render,
        })
    }

    fn render_body_part(
        &self,
        skin_box_sizes: [usize; 2],
        skin_box_offsets: [usize; 2],
        output_offsets: [usize; 2],
        old: bool,
    ) -> Result<&Render, Box<dyn Error>> {
        START.call_once(|| {
            magick_wand_genesis();
        });

        self.skin.read_image(&self.skin_filepath)?;
        self.skin.crop_image(
            skin_box_sizes[0],
            skin_box_sizes[1],
            skin_box_offsets[0] as isize,
            skin_box_offsets[1] as isize,
        )?;
        self.skin.resize_image(
            skin_box_sizes[0] * self.size,
            skin_box_sizes[1] * self.size,
            magick_rust::FilterType::Box,
        )?;
        if old {
            self.skin.flop_image()?;
        }

        self.render.compose_images(
            &self.skin,
            magick_rust::CompositeOperator::Over,
            true,
            output_offsets[0] as isize * self.size as isize,
            output_offsets[1] as isize * self.size as isize,
        )?;
        Ok(self)
    }

    pub fn render_body(&self, render_type: &str, armored: bool) -> Result<&Render, Box<dyn Error>> {
        let mut head = false;
        let mut bust = false;
        let crop: [usize; 4];

        self.render_body_part([8, 8], [8, 8], [4, 0], false)?;

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
            self.render_body_part([8, 12], [20, 20], [4, 8], false)?
                .render_body_part([4, 12], [44, 20], [0, 8], false)?;
            if self.old {
                self.render_body_part([4, 12], [44, 20], [12, 8], true)?;
            } else {
                self.render_body_part([4, 12], [36, 52], [12, 8], false)?;
            }
        }

        if !bust {
            self.render_body_part([4, 12], [4, 20], [4, 20], false)?;

            if self.old {
                self.render_body_part([4, 12], [4, 20], [8, 20], true)?;
            } else {
                self.render_body_part([4, 12], [20, 52], [8, 20], false)?;
            }
        }

        if armored && !&self.old {
            self.render_body_part([8, 8], [40, 8], [4, 0], false)?;

            if !head {
                self.render_body_part([8, 12], [20, 36], [4, 8], false)?
                    .render_body_part([4, 12], [44, 36], [0, 8], false)?
                    .render_body_part([4, 12], [52, 52], [12, 8], false)?;
            }

            if !bust {
                self.render_body_part([4, 12], [4, 36], [4, 20], false)?
                    .render_body_part([4, 12], [4, 52], [8, 20], false)?;
            }
        }
        self.render
            .crop_image(crop[0], crop[1], crop[2] as isize, crop[3] as isize)?;
        Ok(&self)
    }

    pub fn write_image(&self, path: &str) -> Result<(), Box<dyn Error>> {
        self.render.write_image(path)?;
        Ok(())
    }
}

pub fn read_json_from_file(path: &PathBuf) -> Result<Value, Box<dyn Error>> {
    let file = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&file)?)
}

pub fn write_json_to_file(json: &mut Value, path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let mut out = fs::File::create(&path)?;
    write!(&mut out, "{}", serde_json::to_string(&json)?)?;
    Ok(())
}

pub fn backup(json: &mut Value) -> Result<(), Box<dyn Error>> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let now: DateTime<Utc> = SystemTime::now().try_into()?;
    let data_backup = xdg_dirs
        .place_data_file(format!("data-{}.json", now.format("%s")))?;
    write_json_to_file(json, &data_backup)?;
    Ok(())
}

pub fn fetch_osm_info() -> Result<Value, Box<dyn Error>> {
    let mut message_length: usize = 0;
    let mut header_read = false;
    let mut stream = TcpStream::connect("os-mc.net:8283")?;
    let mut buffer = Vec::new();
    let header: [u8; 4];
    stream.read_to_end(&mut buffer)?;

    if buffer.len() >= 4 {
        header_read = true;
        header = buffer[0..4].try_into()?;
        message_length = u32::from_be_bytes(header).try_into()?;
    }

    if header_read && message_length != 0 && buffer.len() >= message_length * 2 + 8 {
        let data_str = String::from_utf8(buffer[4..4 + message_length * 2].to_vec())?;
        let data_json: Value = serde_json::from_str(&data_str.replace(" ", ""))?;
        Ok(data_json)
    } else {
        Err("invalid response".into())
    }
}

pub async fn download_skin(username: &str) -> Result<String, Box<dyn Error>> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let skin_path = xdg_dirs
        .place_cache_file(format!("skins/{}.png", username))?;
    if xdg_dirs.find_cache_file(&skin_path) != None {
        let metadata = fs::metadata(&skin_path)?;
        if let Ok(time) = metadata.created() {
            let difference = time
                .duration_since(time)?;
            if difference.as_secs() < 604800 {
                return Ok(skin_path
                    .to_str()
                    .ok_or("problem parsing path")?
                    .to_string());
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
            out.write_all(&body)?;
            Ok(skin_path
                .to_str()
                .ok_or("problem parsing path")?
                .to_string())
        }
        Err(err) => {
            return Err(err.into());
        }
    }
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
                    full_path
                        .file_name()
                        .ok_or("error retrieving file name")?
                        .to_str()
                        .ok_or("error converting to string")?
                ));
            }
        }
    }
    Ok(json!(result))
}

pub async fn get_nickname(config: &Config, username: &str) -> Result<Value, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "https://micro.os-mc.net/profile_service/ess/{}",
            username
        ))
        .header(
            HeaderName::from_lowercase(b"content-type")?,
            HeaderValue::from_str("application/json")?,
        )
        .body(format!("{{\"token\":\"{}\"}}", config.osm_token))
        .send()
        .await?;
    let json_object: Value = serde_json::from_str(&resp.text().await?)?;
    json_object
        .get("nickname")
        .ok_or("could not retrieve nickname".into())
        .map(|j| j.clone())
        .and_then(|j| {
            if j.is_null() {
                Err("nickname is null".into())
            } else {
                Ok(j)
            }
        })
}

pub async fn cache_nicks() -> Result<(), Box<dyn Error>> {
    let config = Config::new()?;
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let Some(data) = xdg_dirs.find_data_file("data.json") else {
        return Err("could not find data json".into());
    };
    let nick = match xdg_dirs.find_data_file("nick-cache.json") {
        Some(d) => d,
        None => xdg_dirs.place_data_file("nick-cache.json")?,
    };
    let json = read_json_from_file(&data)?;

    let leaders: &Vec<Value> = {
        let Some(leaders) = json.pointer("/leadership") else {
            return Err("could not find value".into());
        };
        let Some(leaders_array) = leaders.as_array() else {
            return Err("could not convert value to array".into());
        };
        leaders_array
    };
    let members: &Vec<Value> = {
        let Some(members) = json.pointer("/member_list") else {
            return Err("could not find value".into());
        };
        let Some(members_array) = members.as_array() else {
            return Err("could not convert value to array".into());
        };
        members_array
    };

    let mut leaders_vec = Vec::new();
    for leader in leaders {
        let Some(title) = leader["title"].as_str() else {
            return Err("could not convert value to str slice".into());
        };
        let Some(username) = leader["username"].as_str() else {
            return Err("could not convert value to str slice".into());
        };
        leaders_vec.push(match get_nickname(&config, username).await {
            Ok(n) => json!({"title":title,"nickname":n,"username":username}),
            Err(..) => json!({"title":title,"nickname":username,"username":username}),
        });
    }

    let mut members_vec = Vec::new();
    for member in members {
        let Some(username) = member["username"].as_str() else {
            return Err("could not convert value to str slice".into());
        };
        members_vec.push(match get_nickname(&config, username).await {
            Ok(n) => json!({"username":n}),
            Err(..) => json!({"username":username}),
        });
    }

    let now: DateTime<Utc> = SystemTime::now().try_into()?;

    write_json_to_file(
        &mut json!({"last_updated":now.to_string(),"leadership":leaders_vec,"member_list":members_vec}),
        &nick,
    )?;
    Ok(())
}

pub fn get_geoip_data(ip: IpAddr, db: PathBuf) -> Result<Value, Box<dyn Error>> {
    let reader = maxminddb::Reader::open_readfile(db)?;
    let city: geoip2::City = match reader.lookup(ip)? {
        Some(j) => j,
        None => return Err("invalid ip".into()),
    };
    Ok(json!(city))
}

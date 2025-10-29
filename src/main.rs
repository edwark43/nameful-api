use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Json},
    routing::get,
};
use axum_client_ip::XRealIp;
use nameful_api::*;
use rand::random_range;
use serde_json::{Value, json};
use std::net::SocketAddr;
use tokio_util::io::ReaderStream;
use xdg::BaseDirectories;

#[tokio::main]
async fn main() {
    let _ = Config::init().await;
    let config = Config::new();
    let app = Router::new()
        .route(
            "/",
            get(|| async { Json(json!({"commit":env!("GIT_HASH")})) }),
        )
        .route("/data", get(data))
        .route("/data{*key_path}", get(data_path))
        .route("/splash", get(splash))
        .route("/propaganda", get(propaganda))
        .route("/online", get(online))
        .route("/ip", get(ip))
        .route("/geoip", get(geoip))
        .route("/nickname/{username}", get(nickname))
        .route(
            "/render/{armored}/{render_type}/{username}/{width}",
            get(render),
        );
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port))
        .await
        .unwrap();

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

async fn data() -> Json<Value> {
    let data = BaseDirectories::with_prefix("nameful-api")
        .find_data_file("data.json")
        .expect("couldn't find data.json");
    Json(get_value_from_key_path(read_json_from_file(data), vec![]))
}

async fn data_path(Path(key_path): Path<String>) -> Json<Value> {
    let key_path_decoded: Vec<&str> = key_path.split("/").collect();
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let data = xdg_dirs
        .find_data_file("data.json")
        .expect("coudn't find data.json");
    let nick = xdg_dirs
        .find_data_file("nick-cache.json")
        .expect("couldn't find data.json");
    match key_path_decoded[1] {
        "nicked" => Json(get_value_from_key_path(
            read_json_from_file(nick),
            key_path_decoded[2..].to_vec(),
        )),
        _ => Json(get_value_from_key_path(
            read_json_from_file(data),
            key_path_decoded,
        )),
    }
}

async fn render(
    Path((armored, render_type, username, width)): Path<(String, String, String, isize)>,
) -> impl IntoResponse {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let skin_path = match download_skin(&username).await {
        Ok(p) => p,
        Err(..) => xdg_dirs.find_cache_file("skins/.fallback.png").unwrap(),
    };
    let armored = match armored.as_str() {
        "armored" => "armored",
        "armorless" => "armorless",
        _ => return Err((StatusCode::BAD_REQUEST, String::from("Bad request"))),
    };
    let mut size = width / 16;
    if width < 1 {
        return Err((StatusCode::BAD_REQUEST, String::from("Bad request")));
    } else if width < 16 {
        size = 1;
    } else if width > 576 {
        size = 36;
    }
    let render_type = match render_type.as_str() {
        "head" => "head",
        "bust" => "bust",
        "body" => "body",
        _ => return Err((StatusCode::BAD_REQUEST, String::from("Bad request"))),
    };
    let render_path = xdg_dirs
        .place_cache_file(format!("{}/{}/{}.png", armored, render_type, username))
        .expect("cannot create skin");

    Render::new(skin_path, size.try_into().unwrap())
        .render_body(render_type, armored == "armored")
        .write_image(&render_path);

    let file = match tokio::fs::File::open(render_path).await {
        Ok(file) => file,
        Err(err) => return Err((StatusCode::NOT_FOUND, format!("File not found: {}", err))),
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let headers = [
        (header::CONTENT_TYPE, "image/png"),
        (header::CONTENT_DISPOSITION, "filename=\"render.png\""),
    ];

    Ok((headers, body))
}

async fn propaganda() -> Json<Value> {
    let config = Config::new();
    match dir_to_json(config.propaganda_path) {
        Ok(j) => Json(j),
        Err(e) => Json(json!({"error":e.to_string()})),
    }
}

async fn online() -> Json<Value> {
    let config = Config::new();
    match read_json_from_url(config.online_url).await {
        Ok(j) => Json(j),
        Err(e) => Json(json!({"error":e.to_string()})),
    }
}

async fn ip(XRealIp(ip): XRealIp) -> Json<Value> {
    Json(json!({"ip":ip}))
}

async fn geoip(XRealIp(ip): XRealIp) -> Json<Value> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let db_path = xdg_dirs
        .find_data_file("GeoLite2-City.mmdb")
        .expect("couldn't find mmdb file");
    match get_geoip_data(ip, db_path) {
        Ok(j) => Json(j),
        Err(e) => Json(json!({"error":e.to_string()})),
    }
}

async fn splash(XRealIp(ip): XRealIp) -> Json<Value> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let data = xdg_dirs
        .find_data_file("data.json")
        .expect("coudn't find data.json");
    let splashes = get_value_from_key_path(read_json_from_file(data), vec!["splashes"]);
    let splashes_array = splashes.as_array().unwrap();

    let rand_num = random_range(0..splashes_array.len() + 1);

    if rand_num == splashes_array.len() {
        return Json(json!({"splash":ip.to_string()}));
    } else {
        return Json(json!({"splash":splashes.get(rand_num).unwrap().as_str()}));
    }
}

async fn nickname(Path(username): Path<String>) -> Json<Value> {
    let config = Config::new();
    match get_nickname(config, &username).await {
        Ok(j) => Json(json!({"nickname":j})),
        Err(..) => Json(json!({"nickname":username})),
    }
}

use axum::{
    Router,
    body::Body,
    extract,
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

#[tokio::main]
async fn main() {
    let f = Config::init().await;
    print!("{:?}", f);
    let config = Config::new();
    let app = Router::new()
        .route(
            "/",
            get(|| async { Json(json!({"commit":env!("GIT_HASH")})) }),
        )
        .route("/leadership", get(leadership))
        .route("/leadership/nicked", get(leadership_nicked))
        .route("/elections", get(elections))
        .route("/constitution", get(constitution))
        .route("/member_list", get(member_list))
        .route("/member_list/nicked", get(member_list_nicked))
        .route("/news_notice", get(news_notice))
        .route("/splash", get(splash))
        .route("/propaganda", get(propaganda))
        .route("/online", get(online))
        .route("/ip", get(ip))
        .route("/geoip", get(geoip))
        .route("/nickname/{username}", get(nickname))
        .route("/render/{armored}/{render_type}/{username}", get(render));
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

async fn leadership() -> Json<Value> {
    let config = Config::new();
    json_at_key(
        format!("{}/data.json", config.data_path),
        String::from("leadership"),
    )
}

async fn leadership_nicked() -> Json<Value> {
    let config = Config::new();
    json_at_key(
        format!("{}/nick-cache.json", config.data_path),
        String::from("leadership"),
    )
}

async fn elections() -> Json<Value> {
    let config = Config::new();
    json_at_key(
        format!("{}/data.json", config.data_path),
        String::from("elections"),
    )
}

async fn constitution() -> Json<Value> {
    let config = Config::new();
    json_at_key(
        format!("{}/data.json", config.data_path),
        String::from("constitution"),
    )
}

async fn member_list() -> Json<Value> {
    let config = Config::new();
    json_at_key(
        format!("{}/data.json", config.data_path),
        String::from("member_list"),
    )
}

async fn member_list_nicked() -> Json<Value> {
    let config = Config::new();
    json_at_key(
        format!("{}/nick-cache.json", config.data_path),
        String::from("member_list"),
    )
}

async fn news_notice() -> Json<Value> {
    let config = Config::new();
    json_at_key(
        format!("{}/data.json", config.data_path),
        String::from("news_notice"),
    )
}

async fn render(
    extract::Path((armored, render_type, username)): extract::Path<(String, String, String)>,
) -> impl IntoResponse {
    let config = Config::new();

    let filename = match download_skin(
        username.clone(),
        format!("{}/skins/{}.png", config.cache_path, username),
    )
    .await
    {
        Ok(..) => format!("{}.png", username),
        Err(..) => String::from("error.png"),
    };

    let armored = armored == "armored";
    let path = match render_type.as_str() {
        "head" => {
            if armored {
                format!("{}/armor/head/{}.png", config.cache_path, username)
            } else {
                format!("{}/armorless/head/{}.png", config.cache_path, username)
            }
        }
        "bust" => {
            if armored {
                format!("{}/armor/bust/{}.png", config.cache_path, username)
            } else {
                format!("{}/armorless/bust/{}.png", config.cache_path, username)
            }
        }
        _ => {
            if armored {
                format!("{}/armor/body/{}.png", config.cache_path, username)
            } else {
                format!("{}/armorless/body/{}.png", config.cache_path, username)
            }
        }
    };

    Render::new(
        String::from(format!("{}/skins/{}", config.cache_path, filename)),
        6,
    )
    .render_body(render_type, armored)
    .write_image(path.clone());

    let file = match tokio::fs::File::open(&path).await {
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
    match dir_to_json(String::from(config.propaganda_path)) {
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
    match get_geoip_data(ip) {
        Ok(j) => Json(j),
        Err(e) => Json(json!({"error":e.to_string()})),
    }
}

async fn splash(XRealIp(ip): XRealIp) -> Json<Value> {
    let config = Config::new();
    let splashes: Vec<Value> = match read_json_from_file(format!("{}/data.json", config.data_path))
    {
        Ok(j) => j.get("splashes").unwrap().as_array().unwrap().clone(),
        Err(e) => vec![json!({"error":e.to_string()})],
    };

    let rand_num = random_range(0..splashes.len() + 1);

    if rand_num == splashes.len() {
        return Json(json!({"splash":ip.to_string()}));
    } else {
        return Json(json!({"splash":splashes.get(rand_num).unwrap().as_str()}));
    }
}

async fn nickname(extract::Path(username): extract::Path<String>) -> Json<Value> {
    let config = Config::new();
    match get_nickname(config, &username).await {
        Ok(j) => Json(json!({"nickname":j})),
        Err(..) => Json(json!({"nickname":username})),
    }
}

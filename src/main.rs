use axum::{
    Router,
    body::Body,
    extract::{Path, Request},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post, put},
};
use axum_client_ip::XRealIp;
use json_value_remove::Remove;
use nameful_api::*;
use rand::random_range;
use serde_json::{Value, json};
use std::{time::Duration, net::{IpAddr, SocketAddr}};
use tokio_util::io::ReaderStream;
use xdg::BaseDirectories;
use clokwerk::{AsyncScheduler, TimeUnits};

#[tokio::main]
async fn main() {
    Config::init().await.expect("couldn't initialize config");
    let config = Config::new(); 
    let mut scheduler = AsyncScheduler::new();
    scheduler.every(6.hours()).run(async || if let Err(e) = cache_nicks().await {
       println!("Caching Error: {}", e)
    });
    tokio::spawn(async move {
      loop {
        scheduler.run_pending().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
      }
    });
    let get_routes = Router::new()
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
        .route("/geoip/{ip}", get(geoip_with_ip))
        .route("/nickname/{username}", get(nickname))
        .route(
            "/render/{armored}/{render_type}/{username}/{width}",
            get(render),
        );
    let put_routes = Router::new()
        .route("/data{*key_path}", put(edit_data_path))
        .route_layer(middleware::from_fn(auth));
    let post_routes = Router::new()
        .route("/data{*key_path}", post(add_data_path))
        .route_layer(middleware::from_fn(auth));
    let delete_routes = Router::new()
        .route("/data{*key_path}", delete(delete_data_path))
        .route_layer(middleware::from_fn(auth));
    let app = get_routes
        .merge(put_routes)
        .merge(post_routes)
        .merge(delete_routes);
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

async fn auth(req: Request, next: Next) -> Result<Response, StatusCode> {
    let config = Config::new();
    let headers = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());
    let header = if let Some(auth) = headers {
        auth
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    if header == format!("Bearer {}", config.api_key) {
        Ok(next.run(req).await)
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    }
}

async fn data() -> Result<Json<Value>, StatusCode> {
    let data = BaseDirectories::with_prefix("nameful-api")
        .find_data_file("data.json")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(read_json_from_file(&data)))
}

async fn data_path(Path(key_path): Path<String>) -> Result<Json<Value>, StatusCode> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let data = xdg_dirs
        .find_data_file("data.json")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let nick = xdg_dirs
        .find_data_file("nick-cache.json")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    if key_path.len() < 7 {
        return match read_json_from_file(&data).pointer(&key_path) {
            Some(j) => Ok(Json(j.clone())),
            None => Err(StatusCode::NOT_FOUND),
        };
    }
    match &key_path[1..7] {
        "nicked" => match read_json_from_file(&nick).pointer(&key_path[7..]) {
            Some(j) => Ok(Json(j.clone())),
            None => Err(StatusCode::NOT_FOUND),
        },
        _ => match read_json_from_file(&data).pointer(&key_path) {
            Some(j) => Ok(Json(j.clone())),
            None => Err(StatusCode::NOT_FOUND),
        },
    }
}

async fn edit_data_path(
    Path(key_path): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let data = xdg_dirs
        .find_data_file("data.json")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = &mut read_json_from_file(&data);
    if let Err(e) = backup(json) {
        eprintln!("{}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let value: &mut Value = json.pointer_mut(&key_path).ok_or(StatusCode::NOT_FOUND)?;
    *value = body;
    match write_json_to_file(json, &data) {
        Ok(s) => Ok(Json(json!({"success":s}))),
        Err(e) => {
            eprintln!("{}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn add_data_path(
    Path(key_path): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let data = BaseDirectories::with_prefix("nameful-api")
        .find_data_file("data.json")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = &mut read_json_from_file(&data);
    if let Err(e) = backup(json) {
        eprintln!("{}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let value: &mut Value = json.pointer_mut(&key_path).ok_or(StatusCode::NOT_FOUND)?;
    if value.is_array() {
        let _ = value
            .as_array_mut()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
            .push(body);
    } else {
        return Err(StatusCode::BAD_REQUEST);
    }
    match write_json_to_file(json, &data) {
        Ok(s) => Ok(Json(json!({"success":s}))),
        Err(e) => {
            eprintln!("{}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn delete_data_path(Path(key_path): Path<String>) -> Result<Json<Value>, StatusCode> {
    let data = BaseDirectories::with_prefix("nameful-api")
        .find_data_file("data.json")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = &mut read_json_from_file(&data);
    if let Err(e) = backup(json) {
        eprintln!("{}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    if let None = json.remove(&key_path).unwrap_or_else(|e| {
            eprintln!("{}", e);
            None
        }) {
        return Err(StatusCode::NOT_FOUND);
    }
    match write_json_to_file(json, &data) {
        Ok(s) => Ok(Json(json!({"success":s}))),
        Err(e) => {
            eprintln!("{}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn render(
    Path((armored, render_type, username, width)): Path<(String, String, String, isize)>,
) -> impl IntoResponse {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let skin_path = download_skin(&username).await.unwrap_or(xdg_dirs.find_cache_file("skins/.fallback.png").ok_or(StatusCode::INTERNAL_SERVER_ERROR)?);
    let armored = match armored.as_str() {
        "armored" => "armored",
        "armorless" => "armorless",
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let mut size = width / 16;
    if width < 1 {
        return Err(StatusCode::BAD_REQUEST);
    } else if width < 16 {
        size = 1;
    } else if width > 576 {
        size = 36;
    }
    let render_type = match render_type.as_str() {
        "head" => "head",
        "bust" => "bust",
        "body" => "body",
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let render_path = xdg_dirs
        .place_cache_file(format!("{}/{}/{}.png", armored, render_type, username))
        .expect("cannot create skin");

    Render::new(
        skin_path,
        match size.try_into() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        },
    )
    .render_body(render_type, armored == "armored")
    .write_image(&render_path);

    let file = match tokio::fs::File::open(render_path).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}", e);
            return Err(StatusCode::NOT_FOUND);
        }
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let headers = [
        (header::CONTENT_TYPE, "image/png"),
        (header::CONTENT_DISPOSITION, "filename=\"render.png\""),
    ];

    Ok((headers, body))
}

async fn propaganda() -> Result<Json<Value>, StatusCode> {
    let config = Config::new();
    match dir_to_json(config.propaganda_path) {
        Ok(j) => Ok(Json(j)),
        Err(e) => {
            eprintln!("{}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn online() -> Result<Json<Value>, StatusCode> {
    match fetch_osm_info() {
        Ok(j) => Ok(Json(match j.pointer("/players") {
            Some(j) => j.clone(),
            None => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        })),
        Err(e) => {
            eprintln!("{}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn ip(XRealIp(ip): XRealIp) -> Json<Value> {
    Json(json!({"ip":ip}))
}

async fn geoip(XRealIp(ip): XRealIp) -> Result<Json<Value>, StatusCode> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let db_path = xdg_dirs
        .find_data_file("GeoLite2-City.mmdb")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    match get_geoip_data(ip, db_path) {
        Ok(j) => Ok(Json(j)),
        Err(e) => {
            eprintln!("{}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn geoip_with_ip(Path(ip): Path<String>) -> Result<Json<Value>, StatusCode> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let db_path = xdg_dirs
        .find_data_file("GeoLite2-City.mmdb")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let ip: IpAddr = match (ip).parse() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    match get_geoip_data(ip, db_path) {
        Ok(j) => Ok(Json(j)),
        Err(e) => {
            eprintln!("{}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn splash(XRealIp(ip): XRealIp) -> Result<Json<Value>, StatusCode> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let data = xdg_dirs
        .find_data_file("data.json")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = read_json_from_file(&data);
    let splashes = match json.pointer("/splashes") {
        Some(j) => j,
        None => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let splashes_array = match splashes.as_array() {
        Some(a) => a,
        None => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let rand_num = random_range(0..splashes_array.len() + 1);

    if rand_num == splashes_array.len() {
        return Ok(Json(json!({"splash":ip.to_string()})));
    } else {
        return Ok(Json(json!({"splash":match splashes.get(rand_num) {
            Some(s) => s.as_str(),
            None => {return Err(StatusCode::INTERNAL_SERVER_ERROR)},
        }})));
    }
}

async fn nickname(Path(username): Path<String>) -> Json<Value> {
    let config = Config::new();
    match get_nickname(&config, &username).await {
        Ok(j) => Json(json!({"nickname":j})),
        Err(..) => Json(json!({"nickname":username})),
    }
}

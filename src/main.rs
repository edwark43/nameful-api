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
use clokwerk::{AsyncScheduler, TimeUnits};
use json_value_remove::Remove;
use nameful_api::*;
use rand::random_range;
use serde_json::{Value, json};
use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};
use tokio_util::io::ReaderStream;
use xdg::BaseDirectories;

#[tokio::main]
async fn main() {
    if let Err(e) = Config::init().await {
        eprintln!("API Crashed due to: {e}");
        return;
    }
    let config = match Config::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("API Crashed due to: {e}");
            return;
        }
    };
    let mut scheduler = AsyncScheduler::new();
    scheduler.every(config.cache_time.hours()).run(async || {
        if let Err(e) = cache_nicks().await {
            println!("Caching Error: {}", e)
        }
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
    let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("API Crashed due to: {e}");
            return;
        }
    };

    if let Err(e) = {
        println!("API successfully started");
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
    } {
        eprintln!("API Crashed due to: {e}");
        return;
    }
}

async fn auth(req: Request, next: Next) -> Result<Response, StatusCode> {
    let config = Config::new().map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
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
    Ok(Json(read_json_from_file(&data).map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?))
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
        return read_json_from_file(&data)
            .map_err(|e| {
                eprintln!("{}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .pointer(&key_path)
            .ok_or(StatusCode::NOT_FOUND)
            .map(|j| Json(j.clone()));
    }
    match &key_path[1..7] {
        "nicked" => read_json_from_file(&nick)
            .map_err(|e| {
                eprintln!("{}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .pointer(&key_path[7..])
            .ok_or(StatusCode::NOT_FOUND)
            .map(|j| Json(j.clone())),
        _ => read_json_from_file(&data)
            .map_err(|e| {
                eprintln!("{}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .pointer(&key_path)
            .ok_or(StatusCode::NOT_FOUND)
            .map(|j| Json(j.clone())),
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
    let json = &mut read_json_from_file(&data).map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if let Err(e) = backup(json) {
        eprintln!("{}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let value: &mut Value = json.pointer_mut(&key_path).ok_or(StatusCode::NOT_FOUND)?;
    *value = body;
    write_json_to_file(json, &data)
        .map_err(|e| {
            eprintln!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .map(|s| Json(json!({"success":s})))
}

async fn add_data_path(
    Path(key_path): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let data = BaseDirectories::with_prefix("nameful-api")
        .find_data_file("data.json")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = &mut read_json_from_file(&data).map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
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
    write_json_to_file(json, &data)
        .map_err(|e| {
            eprintln!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .map(|s| Json(json!({"success":s})))
}

async fn delete_data_path(Path(key_path): Path<String>) -> Result<Json<Value>, StatusCode> {
    let data = BaseDirectories::with_prefix("nameful-api")
        .find_data_file("data.json")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = &mut read_json_from_file(&data).map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
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
    write_json_to_file(json, &data)
        .map_err(|e| {
            eprintln!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .map(|s| Json(json!({"success":s})))
}

async fn render(
    Path((armored, render_type, username, width)): Path<(String, String, String, isize)>,
) -> impl IntoResponse {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let skin_path = download_skin(&username).await.unwrap_or(
        xdg_dirs
            .find_cache_file("skins/.fallback.png")
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
            .to_str()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
            .to_string(),
    );
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
        .map_err(|e| {
            eprintln!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .to_str()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
        .to_string();

    Render::new(
        skin_path,
        size.try_into().map_err(|e| {
            eprintln!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?,
    )
    .map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .render_body(render_type, armored == "armored")
    .map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .write_image(&render_path)
    .map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let file = tokio::fs::File::open(&render_path).await.map_err(|e| {
        eprintln!("{}", e);
        StatusCode::NOT_FOUND
    })?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let headers = [
        (header::CONTENT_TYPE, "image/png"),
        (header::CONTENT_DISPOSITION, "filename=\"render.png\""),
    ];

    Ok((headers, body))
}

async fn propaganda() -> Result<Json<Value>, StatusCode> {
    let config = Config::new().map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    dir_to_json(config.propaganda_path)
        .map_err(|e| {
            eprintln!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .map(|j| Json(j))
}

async fn online() -> Result<Json<Value>, StatusCode> {
    fetch_osm_info()
        .map_err(|e| {
            eprintln!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .map(|j| {
            j.pointer("/players")
                .map(|j| j.clone())
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)
        })
        .flatten()
        .map(|j| Json(j))
}

async fn ip(XRealIp(ip): XRealIp) -> Json<Value> {
    Json(json!({"ip":ip}))
}

async fn geoip(XRealIp(ip): XRealIp) -> Result<Json<Value>, StatusCode> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let db_path = xdg_dirs
        .find_data_file("GeoLite2-City.mmdb")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    get_geoip_data(ip, db_path)
        .map_err(|e| {
            eprintln!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .map(|j| Json(j))
}

async fn geoip_with_ip(Path(ip): Path<String>) -> Result<Json<Value>, StatusCode> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let db_path = xdg_dirs
        .find_data_file("GeoLite2-City.mmdb")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let ip: IpAddr = ip.parse().map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    get_geoip_data(ip, db_path)
        .map_err(|e| {
            eprintln!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .map(|j| Json(j))
}

async fn splash(XRealIp(ip): XRealIp) -> Result<Json<Value>, StatusCode> {
    let xdg_dirs = BaseDirectories::with_prefix("nameful-api");
    let data = xdg_dirs
        .find_data_file("data.json")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = read_json_from_file(&data).map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let splashes = json
        .pointer("/splashes")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let splashes_array = splashes
        .as_array()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let rand_num = random_range(0..splashes_array.len() + 1);

    Ok(Json(match rand_num {
        len if len == splashes_array.len() => json!({"splash":ip.to_string()}),
        _ => json!({"splash":splashes.get(rand_num).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?}),
    }))
}

async fn nickname(Path(username): Path<String>) -> Result<Json<Value>, Json<Value>> {
    let config = Config::new().map_err(|e| {
        eprintln!("{}", e);
        Json(json!({"nickname":username}))
    })?;
    get_nickname(&config, &username)
        .await
        .map_err(|_| Json(json!({"nickname":username})))
        .map(|j| Json(json!({"nickname":j})))
}

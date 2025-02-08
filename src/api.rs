use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use serde::Serialize;

use crate::s3_querier::S3Querier;

#[derive(Serialize)]
pub struct Clip {
    pub id: String,
    pub s3_url: String,
}


/// List all clips in the S3 bucket
/// 
/// # Example
/// ```shell
/// curl http://localhost:8080/clips
/// ```
/// 
/// # Returns
/// ```json
///[{
///    "id": "clip1",
///    "s3_url": "http://
///}]
/// ```
#[get("/clips")]
pub async fn list_clips() -> impl Responder {
    let querier = S3Querier::new("clips", Some("http://127.0.0.1:9000")).await;
    match querier {
        Ok(q) => {
            let clips = q.list_clips().await;

            match clips {
                Ok(clips) => HttpResponse::Ok().json(clips),
                Err(err) => HttpResponse::InternalServerError().body(err.to_string()),
            }
        },
        Err(err) => return HttpResponse::InternalServerError().body(err.to_string()),
    }
}

/// Get a specific clip by ID
/// 
/// # Example
/// ```shell
/// curl http://localhost:8080/clips/clip1
/// ```
/// 
/// # Returns
/// ```json
/// {
///    "id": "clip1",   
///   "s3_url": "http://<endpoint>/clips/clip1"
/// }
#[get("/clips/{id}")]
pub async fn get_clip(path: web::Path<String>) -> impl Responder {
    let querier = S3Querier::new("clips", Some("http://127.0.0.1:9000")).await;
    match querier {
        Ok(q) => {
            match q.get_clip(&path).await {
                Ok(clip) => HttpResponse::Ok().json(clip),
                Err(err) => HttpResponse::InternalServerError().body(err.to_string()),
            }
        },
        Err(err) =>  return HttpResponse::InternalServerError().body(err.to_string()),
    }
}

/// Run the API server
pub async fn run_api_server() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(list_clips)
            .service(get_clip)
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
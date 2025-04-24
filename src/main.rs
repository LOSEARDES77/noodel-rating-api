#[macro_use]
extern crate rocket;
use base64::{engine::general_purpose, Engine as _};
use cors::CORS;
use db::{Db, StorableNoodle};
use rocket::{
    data::{Limits, ToByteUnit},
    form::Form,
    response::content::RawJson,
    Config,
};
use std::sync::Mutex;
mod cors;
mod db;

#[derive(FromForm, Clone)]
pub struct Noodle<'r> {
    pub name: &'r str,
    pub description: Option<&'r str>,
    pub img: &'r str,
    pub rating: usize,
}

#[derive(FromForm, Clone)]
pub struct RateNoodle {
    pub noodle_id: usize,
    pub rating: usize,
}

#[derive(serde::Serialize)]
pub struct ApiNoodle {
    pub id: usize,
    pub name: String,
    pub description: Option<String>,
    pub img: String, // base64
    pub current_rating: Option<usize>,
    pub ratings: Vec<usize>,
}

/// Decode an image from a base64 string or data URL
/// This function will handle both plain base64 and data URLs (data:image/jpeg;base64,...)
fn decode_image_for_processing(image_data: &str) -> Result<Vec<u8>, String> {
    // Check if the image is a data URL (starts with data:)
    let base64_data = if image_data.starts_with("data:") {
        // Split at the comma to get the base64 part
        match image_data.split(',').nth(1) {
            Some(data) => data,
            None => return Err("Invalid data URL format".to_string()),
        }
    } else {
        // Assume it's already a base64 string
        image_data
    };

    // Decode the base64 data
    general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| format!("Failed to decode base64 data: {}", e))
}

#[post("/api/noodle", data = "<noodle>")]
fn create_noodle(noodle: Form<Noodle<'_>>, db: &rocket::State<Mutex<Db>>) -> String {
    let noodle = noodle.into_inner();

    // Properly decode the image data
    let img_data = match decode_image_for_processing(noodle.img) {
        Ok(data) => data,
        Err(e) => return format!("Error processing image: {}", e),
    };

    let storable = StorableNoodle::new(
        noodle.name.to_string(),
        noodle.description.map(|desc| desc.to_string()),
        img_data,
        noodle.rating,
    );

    let result = db.lock().unwrap().store_noodle(&storable);

    match result {
        Ok(_) => "Ok".to_string(),
        Err(e) => format!("Failed to store noodle: {}", e),
    }
}

#[post("/api/rate", data = "<rating>")]
fn rate_noodle(rating: Form<RateNoodle>, db: &rocket::State<Mutex<Db>>) -> String {
    let rating = rating.into_inner();

    let result = db
        .lock()
        .unwrap()
        .rate_noodle(rating.noodle_id, rating.rating);

    match result {
        Ok(_) => "Ok".to_string(),
        Err(e) => format!("Failed to rate noodle: {}", e),
    }
}

#[get("/api/noodles")]
fn get_noodles(db: &rocket::State<Mutex<Db>>) -> RawJson<String> {
    let db = db.lock().unwrap();
    let noodles = match db.fetch_noodles() {
        Ok(noodles) => noodles,
        Err(e) => {
            return RawJson(format!(
                "{{\"error\": \"Failed to fetch noodles from database: {}\"}}",
                e
            ))
        }
    };

    let api_noodles: Vec<ApiNoodle> = noodles
        .into_iter()
        .map(|n| ApiNoodle {
            id: n.id,
            name: n.name,
            description: n.description,
            img: general_purpose::STANDARD.encode(&n.img),
            current_rating: n.current_rating,
            ratings: n.ratings,
        })
        .collect();

    match serde_json::to_string(&api_noodles) {
        Ok(json) => RawJson(json),
        Err(e) => RawJson(format!(
            "{{\"error\": \"Failed to serialize noodles: {}\"}}",
            e
        )),
    }
}

#[get("/health")]
fn health() -> &'static str {
    "OK"
}

#[launch]
fn rocket() -> _ {
    // let auth = Auth::from_env().unwrap();
    // let openai = OpenAI::new(auth, "https://api.openai.com/v1/");

    let db = Db::new("noodles.db").expect("Failed to connect to database");

    let config = Config::figment().merge(("limits", Limits::new().limit("form", 10.mebibytes())));

    rocket::custom(config)
        .manage(Mutex::new(db))
        // .manage(Mutex::new(openai))
        .mount(
            "/",
            routes![create_noodle, health, get_noodles, rate_noodle],
        )
        .attach(CORS)
}

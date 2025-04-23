#[macro_use]
extern crate rocket;
use cors::CORS;
use db::{Db, StorableNoodle};
// use openai_api_rust::chat::*;
// use openai_api_rust::completions::*;
// use openai_api_rust::*;
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
    pub img: &'r str,
    pub rating: usize,
}

#[derive(FromForm, Clone)]
pub struct RateNoodle {
    pub noodle_id: usize,
    pub rating: usize,
}

#[post("/api/noodle", data = "<noodle>")]
fn create_noodle(noodle: Form<Noodle<'_>>, db: &rocket::State<Mutex<Db>>) -> String {
    let noodle = noodle.into_inner();
    let storable = StorableNoodle::new(noodle.img.to_string(), noodle.rating);

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

    // Convert noodles to JSON string
    match serde_json::to_string(&noodles) {
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

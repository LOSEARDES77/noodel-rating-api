use std::io::Cursor;

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use zstd::{decode_all, encode_all};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableNoodle {
    pub id: usize,
    pub name: String,
    pub description: Option<String>,
    pub img: Vec<u8>,
    pub current_rating: Option<usize>,
    pub ratings: Vec<usize>,
}

impl StorableNoodle {
    pub fn new(name: String, description: Option<String>, img: Vec<u8>, rating: usize) -> Self {
        StorableNoodle {
            id: 0,
            name,
            description,
            img,
            current_rating: Some(rating),
            ratings: vec![],
        }
    }
}

pub struct Db {
    connection: Connection,
}

impl Db {
    pub fn new(path: &str) -> Result<Self> {
        let connection = Connection::open(path)?;
        let db = Db { connection };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        self.connection.execute(
            "CREATE TABLE IF NOT EXISTS noodle_images (
                noodle_id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                description TEXT,
                img BLOB NOT NULL
            )",
            [],
        )?;
        self.connection.execute(
            "CREATE TABLE IF NOT EXISTS noodle_ratings (
                rating_id INTEGER PRIMARY KEY AUTOINCREMENT,
                noodle_id INTEGER NOT NULL,
                rating INTEGER NOT NULL,
                FOREIGN KEY (noodle_id) REFERENCES noodle_images (noodle_id)
            )",
            [],
        )?;
        Ok(())
    }

    pub fn store_noodle(&self, noodle: &StorableNoodle) -> Result<()> {
        // Create a temporary file for the input image
        let input_file = tempfile::Builder::new().tempfile().map_err(|e| {
            rusqlite::Error::InvalidParameterName(format!("Failed to create temp file: {}", e))
        })?;
        std::fs::write(input_file.path(), &noodle.img).map_err(|e| {
            rusqlite::Error::InvalidParameterName(format!("Failed to write temp file: {}", e))
        })?;

        // Create a temporary file for the output WebP image
        let output_file = tempfile::Builder::new()
            .suffix(".webp")
            .tempfile()
            .map_err(|e| {
                rusqlite::Error::InvalidParameterName(format!("Failed to create temp file: {}", e))
            })?;

        // Use ffmpeg to convert the image to WebP
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-i",
                input_file.path().to_str().unwrap(),
                "-vf",
                "scale=800:-1",
                "-y", // Overwrite output files without asking
                output_file.path().to_str().unwrap(),
            ])
            .status()
            .map_err(|e| {
                rusqlite::Error::InvalidParameterName(format!("Failed to run ffmpeg: {}", e))
            })?;

        if !status.success() {
            return Err(rusqlite::Error::InvalidParameterName(
                "ffmpeg conversion failed".to_string(),
            ));
        }

        // Read the WebP image
        let webp_img = std::fs::read(output_file.path()).map_err(|e| {
            rusqlite::Error::InvalidParameterName(format!("Failed to read WebP image: {}", e))
        })?;

        // Compress the WebP image with zstd
        let compressed_img = encode_all(Cursor::new(&webp_img), 0).unwrap_or_default();

        self.connection.execute(
            "INSERT INTO noodle_images (name, description, img) VALUES (?1, ?2, ?3)",
            params![noodle.name, noodle.description, compressed_img],
        )?;

        // Get the last inserted noodle_id
        let noodle_id = self.connection.last_insert_rowid();

        // Insert initial rating if provided
        if let Some(rating) = noodle.current_rating {
            self.connection.execute(
                "INSERT INTO noodle_ratings (noodle_id, rating) VALUES (?1, ?2)",
                params![noodle_id, rating],
            )?;
        }

        Ok(())
    }

    pub fn rate_noodle(&self, noodle_id: usize, rating: usize) -> Result<()> {
        self.connection.execute(
            "INSERT INTO noodle_ratings (noodle_id, rating) VALUES (?1, ?2)",
            params![noodle_id, rating],
        )?;
        Ok(())
    }

    pub fn fetch_noodles(&self) -> Result<Vec<StorableNoodle>> {
        let mut noodles_map = std::collections::HashMap::new();

        let mut stmt = self.connection.prepare(
            "SELECT n.noodle_id, n.name, n.description, n.img, r.rating 
             FROM noodle_images n 
             LEFT JOIN noodle_ratings r ON n.noodle_id = r.noodle_id 
             ORDER BY n.noodle_id",
        )?;

        let rows = stmt.query_map([], |row| {
            let id: usize = row.get(0)?;
            let name: String = row.get(1)?;
            let description: Option<String> = row.get(2)?;
            let compressed_img: Vec<u8> = row.get(3)?;
            let img = decode_all(Cursor::new(compressed_img)).unwrap_or_default();
            let rating: Option<usize> = row.get(4).ok();

            Ok((id, name, description, img, rating))
        })?;

        for row_result in rows {
            let (id, name, description, img, rating) = row_result?;

            noodles_map.entry(id).or_insert_with(|| StorableNoodle {
                id,
                name,
                description,
                img,
                current_rating: None,
                ratings: Vec::new(),
            });

            if let Some(r) = rating {
                noodles_map.get_mut(&id).unwrap().ratings.push(r);
            }
        }

        Ok(noodles_map.into_values().collect())
    }
}

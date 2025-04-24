use std::fs::File;
use std::io::Write;
use std::process::Command;

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableRating {
    pub noodle_id: usize,
    pub rating: usize,
    pub review: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableNoodle {
    pub id: usize,
    pub name: String,
    pub description: Option<String>,
    pub img: Vec<u8>,
    pub current_rating: Option<usize>,
    pub ratings: Vec<StorableRating>,
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
        db.migrate()?; // Add migration call after initialization
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
                review TEXT,
                FOREIGN KEY (noodle_id) REFERENCES noodle_images (noodle_id)
            )",
            [],
        )?;
        Ok(())
    }

    // Add migration function to add missing columns
    fn migrate(&self) -> Result<()> {
        // Check if review column exists in noodle_ratings table
        let mut stmt = self.connection.prepare(
            "SELECT COUNT(*) FROM pragma_table_info('noodle_ratings') WHERE name = 'review'",
        )?;
        let has_review_column: i64 = stmt.query_row([], |row| row.get(0))?;

        // Add the column if it doesn't exist
        if has_review_column == 0 {
            println!("Adding 'review' column to noodle_ratings table...");
            self.connection
                .execute("ALTER TABLE noodle_ratings ADD COLUMN review TEXT", [])?;
            println!("Migration complete!");
        }

        Ok(())
    }

    pub fn store_noodle(&self, noodle: &StorableNoodle) -> Result<()> {
        // Convert image to WebP
        let webp_data = match convert_to_webp(&noodle.img) {
            Ok(data) => {
                println!(
                    "Successfully converted image to WebP, size: {} bytes",
                    data.len()
                );
                data
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to convert image to WebP: {}. Using original image.",
                    e
                );
                println!("Original image size: {} bytes", noodle.img.len());
                noodle.img.clone()
            }
        };

        // Store the WebP image directly without compression
        self.connection.execute(
            "INSERT INTO noodle_images (name, description, img) VALUES (?1, ?2, ?3)",
            params![noodle.name, noodle.description, webp_data],
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

    pub fn rate_noodle(
        &self,
        noodle_id: usize,
        rating: usize,
        review: Option<String>,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO noodle_ratings (noodle_id, rating, review) VALUES (?1, ?2, ?3)",
            params![noodle_id, rating, review],
        )?;
        Ok(())
    }

    pub fn fetch_noodles(&self) -> Result<Vec<StorableNoodle>> {
        let mut noodles_map = std::collections::HashMap::new();

        let mut stmt = self.connection.prepare(
            "SELECT n.noodle_id, n.name, n.description, n.img, r.rating, r.review
             FROM noodle_images n 
             LEFT JOIN noodle_ratings r ON n.noodle_id = r.noodle_id 
             ORDER BY n.noodle_id",
        )?;

        let rows = stmt.query_map([], |row| {
            let id: usize = row.get(0)?;
            let name: String = row.get(1)?;
            let description: Option<String> = row.get(2)?;
            let img: Vec<u8> = row.get(3)?;
            let rating: Option<usize> = row.get(4).ok();
            let review: Option<String> = row.get(5).ok();

            println!(
                "Retrieved image for noodle {}, size: {} bytes",
                id,
                img.len()
            );
            Ok((id, name, description, img, rating, review))
        })?;

        for row_result in rows {
            let (id, name, description, img, rating, review) = row_result?;

            noodles_map.entry(id).or_insert_with(|| StorableNoodle {
                id,
                name,
                description,
                img,
                current_rating: None,
                ratings: Vec::new(),
            });

            if let Some(r) = rating {
                let ratings = &mut noodles_map.get_mut(&id).unwrap().ratings;
                ratings.push(StorableRating {
                    noodle_id: id,
                    rating: r,
                    review,
                });
            }
        }

        Ok(noodles_map.into_values().collect())
    }
}

// Convert image to WebP using ffmpeg
fn convert_to_webp(img_data: &[u8]) -> Result<Vec<u8>, String> {
    // Create temporary directories
    let temp_dir = std::env::temp_dir().join("noodle_images");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;

    // Generate unique filenames for input and output with proper extensions
    let uuid = Uuid::new_v4();
    let input_path = temp_dir.join(format!("{}_input.jpg", uuid)); // Add .jpg extension
    let output_path = temp_dir.join(format!("{}_output.webp", uuid));

    println!(
        "Saving input image ({} bytes) to {}",
        img_data.len(),
        input_path.display()
    );

    // Write input image to temporary file
    let mut file =
        File::create(&input_path).map_err(|e| format!("Failed to create input file: {}", e))?;
    file.write_all(img_data)
        .map_err(|e| format!("Failed to write image data: {}", e))?;

    // Make sure file is written to disk
    file.flush()
        .map_err(|e| format!("Failed to flush file: {}", e))?;
    drop(file);

    println!("Running ffmpeg command");

    // Run ffmpeg to convert to WebP with verbose output
    let output = Command::new("ffmpeg")
        .arg("-v")
        .arg("verbose") // Verbose output
        .arg("-i")
        .arg(&input_path)
        .arg("-vf")
        .arg("scale=800:-1")
        .arg("-preset")
        .arg("photo")
        .arg("-pix_fmt")
        .arg("yuva420p")
        .arg("-vcodec")
        .arg("libwebp")
        .arg("-q:v")
        .arg("90")
        .arg(&output_path)
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    // Check if ffmpeg was successful
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("ffmpeg stdout: {}", stdout);
        return Err(format!("ffmpeg failed: {}", stderr));
    }

    println!("ffmpeg successful, reading output file");

    // Read the output WebP file
    let webp_data =
        std::fs::read(&output_path).map_err(|e| format!("Failed to read WebP file: {}", e))?;

    println!(
        "WebP conversion complete, output size: {} bytes",
        webp_data.len()
    );

    // Clean up temporary files
    std::fs::remove_file(&input_path).ok();
    std::fs::remove_file(&output_path).ok();

    Ok(webp_data)
}

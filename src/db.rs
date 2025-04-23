use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorableNoodle {
    pub id: usize,
    pub img: String,
    pub current_rating: Option<usize>, // Changed from single rating to Option
    pub ratings: Vec<usize>,           // Changed from single rating to Vec of ratings
}

impl StorableNoodle {
    pub fn new(img: String, rating: usize) -> Self {
        StorableNoodle {
            id: 0,
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
                img TEXT NOT NULL
            )",
            [],
        )?;
        // Modified schema - noodle_id is not primary key anymore, add rating_id
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
        self.connection.execute(
            "INSERT INTO noodle_images (img) VALUES (?1)",
            params![noodle.img],
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
            "SELECT n.noodle_id, n.img, r.rating 
             FROM noodle_images n 
             LEFT JOIN noodle_ratings r ON n.noodle_id = r.noodle_id 
             ORDER BY n.noodle_id",
        )?;

        let rows = stmt.query_map([], |row| {
            let id: usize = row.get(0)?;
            let img: String = row.get(1)?;
            let rating: Option<usize> = row.get(2).ok();

            Ok((id, img, rating))
        })?;

        for row_result in rows {
            let (id, img, rating) = row_result?;

            noodles_map.entry(id).or_insert_with(|| StorableNoodle {
                id,
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

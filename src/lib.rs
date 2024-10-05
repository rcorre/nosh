pub mod data;
pub mod food;
pub mod journal;
pub mod nutrients;
pub mod serving;

pub use data::*;
pub use food::*;
pub use journal::*;
pub use nutrients::*;
pub use serving::*;

use anyhow::{anyhow, bail, Result};
use chrono::Datelike;
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

pub const APP_NAME: &'static str = env!("CARGO_PKG_NAME");

// Database provides access to the nosh "database".
// Nosh stores all of it's data as text files using a particular directory structure:
// - $root/ (typically XDG_DATA_HOME)
//
//   - food/
//     - apple.txt
//     - banana.txt
//
//   - recipe/
//     - cake.txt
//     - pie.txt
//
//   - journal/
//     - 2024/
//       - 01/
//         - 01.txt
//         - 02.txt
//     - 2023/
//       - 12/
//         - 30.txt
//         - 31.txt
#[derive(Debug)]
pub struct Database {
    food_dir: PathBuf,
    recipe_dir: PathBuf,
    journal_dir: PathBuf,
}

impl Database {
    // Create a new database at the given root directory.
    pub fn new(root_dir: &Path) -> Result<Database> {
        let food_dir = root_dir.join("food");
        let recipe_dir = root_dir.join("recipe");
        let journal_dir = root_dir.join("journal");
        fs::create_dir_all(&food_dir)?;
        fs::create_dir_all(&recipe_dir)?;
        fs::create_dir_all(&journal_dir)?;
        Ok(Database {
            food_dir,
            recipe_dir,
            journal_dir,
        })
    }

    fn list<'a, T: Data>(
        &self,
        dir: &Path,
        term: &'a Option<&str>,
    ) -> Result<impl Iterator<Item = Result<T>> + 'a> {
        log::trace!("Listing {dir:?}");
        let term = term.as_ref().unwrap_or(&"");
        Ok(fs::read_dir(&dir)?
            .filter(move |e| match e {
                Ok(e) => e.path().as_path().to_string_lossy().contains(term),
                Err(_) => false, // propagate errors through
            })
            .map(|e| -> Result<T> {
                let f = fs::File::open(e?.path())?;
                let r = std::io::BufReader::new(f);
                let t = T::load(r)?;
                Ok(t)
            }))
    }

    fn write<T: serde::Serialize + std::fmt::Debug>(&self, path: &Path, obj: &T) -> Result<()> {
        log::trace!("Writing to {path:?}: {obj:?}");
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| anyhow!("Missing parent dir: {path:?}"))?,
        )?;
        Ok(fs::write(&path, toml::to_string_pretty(obj)?)?)
    }

    pub fn load_food(&self, key: &str) -> Result<Option<Food>> {
        let path = &self.food_dir.join(key).with_extension("txt");
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                bail!("Failed to open '{path:?}': {e}")
            }
        };
        let r = std::io::BufReader::new(file);
        Ok(Some(Food::load(r)?))
    }

    pub fn save_food(&self, key: &str, food: &Food) -> Result<()> {
        let path = &self.food_dir.join(key).with_extension("txt");
        let file = fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        food.save(&mut writer)
    }

    // TODO: fix listing
    pub fn list_food<'a>(
        &self,
        term: &'a Option<&str>,
    ) -> Result<impl Iterator<Item = Result<Food>> + 'a> {
        self.list(&self.food_dir, term)
    }

    pub fn remove_food<'a>(&self, key: &str) -> Result<()> {
        Ok(std::fs::remove_file(
            &self.food_dir.join(key).with_extension("toml"),
        )?)
    }

    fn journal_path(&self, date: &impl Datelike) -> PathBuf {
        self.journal_dir
            .join(format!("{:04}", date.year()))
            .join(format!("{:02}", date.month()))
            .join(format!("{:02}", date.day()))
            .with_extension("txt")
    }

    // Fetch the journal for the given date.
    // Returns None if there is no journal for that date.
    pub fn load_journal(&self, key: &impl Datelike) -> Result<Option<Journal>> {
        let path = &self.journal_path(key);
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                bail!("Failed to open '{path:?}': {e}")
            }
        };
        let r = std::io::BufReader::new(file);
        Ok(Some(Journal::load(r)?))
    }

    pub fn save_journal(&self, key: &impl Datelike, journal: &Journal) -> Result<()> {
        let path = &self.journal_path(key);
        let file = fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        journal.save(&mut writer)
    }
}

#[cfg(test)]
mod tests {
    use crate::nutrients::Nutrients;

    use super::*;

    //https://stackoverflow.com/a/65192210/1435461
    fn cp(src: impl AsRef<Path>, dst: impl AsRef<Path>) {
        fs::create_dir_all(&dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let ty = entry.file_type().unwrap();
            if ty.is_dir() {
                cp(entry.path(), dst.as_ref().join(entry.file_name()));
            } else {
                fs::copy(entry.path(), dst.as_ref().join(entry.file_name())).unwrap();
            }
        }
    }

    fn setup() -> (Database, tempfile::TempDir) {
        let _ = env_logger::try_init();
        let tmp = tempfile::tempdir().unwrap();
        let data = Database::new(tmp.path()).unwrap();
        cp("tests/testdata", &tmp);
        (data, tmp)
    }

    #[test]
    fn test_load_food() {
        let (data, _tmp) = setup();
        let oats = data.load_food("oats").unwrap().unwrap();
        assert_eq!(oats.name, "Oats");
        assert_eq!(oats.nutrients.carb, 68.7);
        assert_eq!(oats.nutrients.fat, 5.89);
        assert_eq!(oats.nutrients.protein, 13.5);
        assert_eq!(oats.nutrients.kcal, 382.0);
        assert_eq!(oats.servings, [("cups".into(), 0.5), ("g".into(), 100.0)]);
    }

    #[test]
    fn test_save_food() {
        let (data, tmp) = setup();
        let food = Food {
            name: "Cereal".into(),
            nutrients: Nutrients {
                carb: 22.0,
                fat: 0.5,
                protein: 1.2,
                kcal: 120.0,
            },
            servings: vec![("g".into(), 50.0), ("cups".into(), 2.5)],
        };
        data.save_food("banana", &food).unwrap();
        let res = fs::read_to_string(tmp.path().join("food/banana.txt")).unwrap();
        assert_eq!(
            res,
            [
                "name = Cereal",
                "carb = 22",
                "fat = 0.5",
                "protein = 1.2",
                "kcal = 120",
                "serving = 50 g",
                "serving = 2.5 cups",
                "",
            ]
            .join("\n")
        );
    }

    #[test]
    fn test_load_journal_not_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let data = Database::new(tmp.path()).unwrap();
        let date = &chrono::NaiveDate::from_ymd_opt(2024, 07, 01).unwrap();
        let actual = data.load_journal(&date.clone()).unwrap();
        assert!(actual.is_none());
    }

    #[test]
    fn test_load_journal() {
        let (data, _tmp) = setup();

        let serv = |food: &str, size, unit| (food.into(), Serving { size, unit });
        let expected = Journal(vec![
            serv("banana", 1.0, None),
            serv("oats", 0.5, Some("c".into())),
            serv("oats", 1.0, None),
            serv("banana", 50.0, Some("g".into())),
        ]);

        let date = &chrono::NaiveDate::from_ymd_opt(2024, 07, 01).unwrap();
        let actual = data.load_journal(&date.clone()).unwrap().unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_save_journal() {
        let (data, tmp) = setup();

        let serv = |food: &str, size, unit| (food.into(), Serving { size, unit });
        let expected = Journal(vec![
            serv("cookies", 1.0, None),
            serv("crackers", 0.5, Some("cups".into())),
            serv("cereal", 50.0, Some("g".into())),
        ]);

        let date = &chrono::NaiveDate::from_ymd_opt(2024, 07, 08).unwrap();
        data.save_journal(&date.clone(), &expected).unwrap();

        let actual = fs::read_to_string(
            tmp.path()
                .join("journal")
                .join(format!("{:04}", date.year()))
                .join(format!("{:02}", date.month()))
                .join(format!("{:02}", date.day()))
                .with_extension("txt"),
        )
        .unwrap();
        assert_eq!(
            actual,
            ["cookies = 1", "crackers = 0.5 cups", "cereal = 50 g", ""].join("\n")
        );
    }
}

pub mod data;
pub mod food;
pub mod journal;
pub mod nutrients;
pub mod search;
pub mod serving;

pub use data::*;
pub use food::*;
pub use journal::*;
pub use nutrients::*;
pub use search::*;
pub use serving::*;

use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

pub const APP_NAME: &'static str = env!("CARGO_PKG_NAME");

// Database provides access to the nosh "database".
// Nosh stores all of it's data as text files using a particular directory structure:
// - $root/ (typically XDG_DATA_HOME)
//
//   - food/
//     - apple.txt
//     - banana.txt
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
    dir: PathBuf,
}

impl Database {
    // Create a new database at the given root directory.
    pub fn new(dir: impl Into<PathBuf>) -> Result<Database> {
        Ok(Database { dir: dir.into() })
    }

    // Return a list of (key, item) pairs.
    // If `term` is Some, only return items containing `term`.
    pub fn list<'a, T: Data>(
        &self,
        term: &'a Option<&str>,
    ) -> Result<impl Iterator<Item = Result<(String, T)>> + 'a> {
        let term = term.as_ref().unwrap_or(&"");
        let dir = self.dir.join(T::DIR);
        log::trace!("Listing {dir:?}");
        Ok(fs::read_dir(&dir)?
            .filter(move |e| match e {
                Ok(e) => e.path().as_path().to_string_lossy().contains(term),
                Err(_) => false, // propagate errors through
            })
            .map(|e| -> Result<(String, T)> {
                let path = e?.path();
                let f = fs::File::open(&path)?;
                let r = std::io::BufReader::new(f);
                let path = path.with_extension("");
                let key = path
                    .file_name()
                    .with_context(|| format!("Invalid path: {path:?}"))?
                    .to_str()
                    .with_context(|| format!("Non UTF-8 path: {path:?}"))?;
                let obj = T::load(r)?;
                Ok((key.to_string(), obj))
            }))
    }

    pub fn save<T: Data + std::fmt::Debug>(&self, key: &T::Key, data: &T) -> Result<()> {
        let path = self.dir.join(T::path(key));
        log::debug!("Saving {data:?} to {path:?}");
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| anyhow!("No parent path: {path:?}"))?,
        )?;
        let file = std::fs::File::create(&path).with_context(|| format!("Open {path:?}"))?;
        let mut writer = BufWriter::new(&file);
        data.save(&mut writer)?;
        Ok(())
    }

    pub fn load<T: Data>(&self, key: &T::Key) -> Result<Option<T>> {
        let path = self.dir.join(T::path(key));
        log::debug!("Loading {path:?}");
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                bail!("Failed to open '{path:?}': {e}")
            }
        };
        let reader = BufReader::new(file);
        Ok(Some(T::load(reader)?))
    }

    pub fn remove<T: Data>(&self, key: &T::Key) -> Result<()> {
        Ok(std::fs::remove_file(&self.dir.join(T::path(key)))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nutrients::Nutrients;
    use chrono::Datelike as _;
    use std::path::Path;

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
        cp("tests/testdata/good", &tmp);
        (data, tmp)
    }

    #[test]
    fn test_load_food() {
        let (data, _tmp) = setup();
        let oats: Food = data.load("oats").unwrap().unwrap();
        assert_eq!(
            oats,
            Food {
                name: "Oats".into(),
                nutrients: Nutrients {
                    carb: 68.7,
                    fat: 5.89,
                    protein: 13.5,
                    kcal: 382.0,
                },
                servings: vec![("cups".into(), 0.5), ("g".into(), 100.0)],
            }
        );
    }

    #[test]
    fn test_load_food_not_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let data = Database::new(tmp.path()).unwrap();
        let actual = data.load::<Food>("nope").unwrap();
        assert!(actual.is_none());
    }

    #[test]
    fn test_list_food() {
        let (data, _tmp) = setup();
        let actual = data
            .list::<Food>(&Some("oats"))
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            actual,
            vec![(
                "oats".to_string(),
                Food {
                    name: "Oats".into(),
                    nutrients: Nutrients {
                        carb: 68.7,
                        fat: 5.89,
                        protein: 13.5,
                        kcal: 382.0,
                    },
                    servings: vec![("cups".into(), 0.5), ("g".into(), 100.0)],
                }
            )]
        );
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
        data.save("cereal", &food).unwrap();
        let res = fs::read_to_string(tmp.path().join("food/cereal.txt")).unwrap();
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
        let actual = data.load::<Journal>(&date.clone()).unwrap();
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
        let actual: Journal = data.load(&date.clone()).unwrap().unwrap();
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
        data.save(&date.clone(), &expected).unwrap();

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

    // #[test]
    // fn test_save_recipe() {
    //     let (data, tmp) = setup();
    //     let recipe = Recipe {
    //         name: "Granola".into(),
    //         ingredients: vec![
    //             (
    //                 "oats".into(),
    //                 Serving {
    //                     size: 6.0,
    //                     unit: Some("cups".into()),
    //                 },
    //             ),
    //             (
    //                 "oil".into(),
    //                 Serving {
    //                     size: 0.5,
    //                     unit: Some("cups".into()),
    //                 },
    //             ),
    //             (
    //                 "maple_syrup".into(),
    //                 Serving {
    //                     size: 0.5,
    //                     unit: Some("cups".into()),
    //                 },
    //             ),
    //         ],
    //     };
    //     data.save("granola", &recipe).unwrap();

    //     let actual = fs::read_to_string(
    //         tmp.path()
    //             .join("recipe")
    //             .join("granola")
    //             .with_extension("txt"),
    //     )
    //     .unwrap();
    //     assert_eq!(
    //         actual,
    //         [
    //             "oats = 6 cups",
    //             "oil = 0.5 cups",
    //             "maple_syrup = 0.5 cups",
    //             ""
    //         ]
    //         .join("\n")
    //     );
    // }

    // #[test]
    // fn test_load_recipe() {
    //     let (data, _tmp) = setup();
    //     let actual = data.load::<Recipe>("banana_oatmeal").unwrap().unwrap();
    //     let expected = Recipe {
    //         name: "Banana Oatmeal".into(),
    //         ingredients: vec![
    //             (
    //                 "oats".into(),
    //                 Serving {
    //                     size: 0.5,
    //                     unit: Some("c".into()),
    //                 },
    //             ),
    //             (
    //                 "banana".into(),
    //                 Serving {
    //                     size: 150.0,
    //                     unit: Some("g".into()),
    //                 },
    //             ),
    //         ],
    //     };
    //     assert_eq!(actual, expected,);
    // }
}

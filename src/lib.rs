use anyhow::{anyhow, bail, Context, Result};
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const APP_NAME: &'static str = env!("CARGO_PKG_NAME");

fn float0(f: &f32) -> String {
    format!("{:.0}", f)
}

fn float1(f: &f32) -> String {
    format!("{:.1}", f)
}

// The macronutrients of a food.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, tabled::Tabled)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Nutrients {
    #[tabled(display_with = "float1")]
    pub carb: f32,
    #[tabled(display_with = "float1")]
    pub fat: f32,
    #[tabled(display_with = "float1")]
    pub protein: f32,
    #[tabled(display_with = "float0")]
    pub kcal: f32,
}

impl Nutrients {
    // If kcal is 0, compute it using the Atwater General Calculation:
    // 4*carb + 4*protein + 9*fat.
    // Note that there is a newer system that uses food-specific multipliers:
    // See https://en.wikipedia.org/wiki/Atwater_system#Modified_system.
    pub fn maybe_compute_kcal(self) -> Nutrients {
        Nutrients {
            kcal: if self.kcal > 0.0 {
                self.kcal
            } else {
                self.carb * 4.0 + self.fat * 9.0 + self.protein * 4.0
            },
            ..self
        }
    }
}
impl std::ops::Add<Nutrients> for Nutrients {
    type Output = Nutrients;

    fn add(self, rhs: Nutrients) -> Self::Output {
        Nutrients {
            carb: self.carb + rhs.carb,
            fat: self.fat + rhs.fat,
            protein: self.protein + rhs.protein,
            kcal: self.kcal + rhs.kcal,
        }
    }
}

impl std::ops::Mul<f32> for Nutrients {
    type Output = Nutrients;

    fn mul(self, rhs: f32) -> Self::Output {
        Nutrients {
            carb: self.carb * rhs,
            fat: self.fat * rhs,
            protein: self.protein * rhs,
            kcal: self.kcal * rhs,
        }
    }
}

#[test]
fn test_nutrient_mult() {
    let nut = Nutrients {
        carb: 1.2,
        fat: 2.3,
        protein: 3.1,
        kcal: 124.5,
    } * 2.0;

    assert_eq!(nut.carb, 2.4);
    assert_eq!(nut.fat, 4.6);
    assert_eq!(nut.protein, 6.2);
    assert_eq!(nut.kcal, 249.0);
}

#[test]
fn test_nutrient_kcal_computation() {
    let nut = Nutrients {
        carb: 1.2,
        fat: 2.3,
        protein: 3.1,
        kcal: 0.0,
    }
    .maybe_compute_kcal();

    assert_eq!(nut.carb, 1.2);
    assert_eq!(nut.fat, 2.3);
    assert_eq!(nut.protein, 3.1);
    assert_eq!(nut.kcal, 37.9);
}

// Food describes a single food item.
#[derive(Serialize, Deserialize, Debug, Default, tabled::Tabled)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Food {
    // The display name of the food. This is shown in the UI.
    // Data files will reference the food by it's filename, not display name.
    pub name: String,

    // The macronutrients of this food item.
    #[tabled(inline)]
    pub nutrients: Nutrients,

    // Ways of describing a single serving of this food.
    // For example, [("g", 100.0), ("cups", 0.5)] means that
    // either 100g or 0.5cups equates to one serving.
    #[tabled(skip)]
    pub servings: Vec<(String, f32)>,
}

// Serving is a portion of food, optionally paired with a unit.
// Without a unit, it represents a portion of a serving, e.g. 1.5 servings.
// Otherwise, it represents a quantity such as "150 grams".
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Serving {
    pub size: f32,
    pub unit: Option<String>,
}

impl Default for Serving {
    fn default() -> Self {
        Self {
            size: 1.0,
            unit: None,
        }
    }
}

impl std::fmt::Display for Serving {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.unit {
            Some(unit) => write!(f, "{} {}", self.size, unit),
            None => write!(f, "{}", self.size),
        }
    }
}

// A serving can be parsed from a strings of form:
// "1.5", "1.5cups", "1.5 cups"
impl FromStr for Serving {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        let (size, unit) = match s.find(|c: char| c != '.' && !c.is_digit(10)) {
            Some(idx) => {
                let (size, unit) = s.split_at(idx);
                (size.trim(), Some(unit.trim()))
            }
            None => (s, None),
        };
        let size = size.parse().with_context(|| format!("Parsing '{size}'"))?;
        Ok(Self {
            size,
            unit: unit.map(str::to_string),
        })
    }
}

#[test]
fn test_parse_serving() {
    let parse = |s: &str| s.parse::<Serving>();
    let serv = |size, unit: Option<&str>| Serving {
        size,
        unit: unit.map(str::to_string),
    };
    assert_eq!(parse("1.5").unwrap(), serv(1.5, None));
    assert_eq!(parse("1.5c").unwrap(), serv(1.5, Some("c")));
    assert_eq!(parse("1.5 cup").unwrap(), serv(1.5, Some("cup")));
    assert_eq!(parse("25 g dry").unwrap(), serv(25.0, Some("g gry")));
    assert_eq!(parse("25g dry").unwrap(), serv(25.0, Some("g gry")));
    assert_eq!(parse(" 1.5  cup ").unwrap(), serv(1.5, Some("cup")));
    assert!(parse("cup 1.5").is_err());
}

// Journal is a record of food consumed during a day.
// It is a list of "food: serving" lines.
// The serving is optional and defaults to 1.
// For example:
// ```
// oats: 0.5 cup
// banana: 1
// berries
// ```
#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Journal(pub Vec<(String, Serving)>);

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

    fn list<'a, T: serde::de::DeserializeOwned>(
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
                let s = fs::read_to_string(e?.path())?;
                let t: T = toml::from_str(&s)?;
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
        let content = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                bail!("Failed to open '{path:?}': {e}")
            }
        };

        let mut food = Food::default();
        for line in content.lines() {
            log::trace!("Parsing food {key} line: {line}");
            let Some((k, v)) = line.rsplit_once(":") else {
                bail!("Invalid food line, expected ':': {line}");
            };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "name" => food.name = v.into(),
                "kcal" => food.nutrients.kcal = v.parse()?,
                "carb" => food.nutrients.carb = v.parse()?,
                "fat" => food.nutrients.fat = v.parse()?,
                "protein" => food.nutrients.protein = v.parse()?,
                "serving" => {
                    let idx = v
                        .find(|c: char| c != '.' && !c.is_digit(10))
                        .ok_or_else(|| anyhow!("Invalid serving: {v}"))?;
                    let (size, unit) = v.split_at(idx);
                    let size = size.trim();
                    let unit = unit.trim();
                    let size = size.parse().with_context(|| format!("Parsing '{size}'"))?;
                    food.servings.push((unit.into(), size));
                }
                _ => bail!("Unexpected food key: {k}"),
            }
        }
        Ok(Some(food))
    }

    pub fn save_food(&self, key: &str, food: &Food) -> Result<()> {
        let path = &self.food_dir.join(key).with_extension("txt");
        let file = fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        let n = &food.nutrients;
        writeln!(writer, "name: {}", food.name)?;
        writeln!(writer, "carb: {}", n.carb)?;
        writeln!(writer, "fat: {}", n.fat)?;
        writeln!(writer, "protein: {}", n.protein)?;
        writeln!(writer, "kcal: {}", n.kcal)?;
        for (unit, size) in &food.servings {
            writeln!(writer, "serving: {size} {unit}")?;
        }
        Ok(())
    }

    // TODO: fix listing
    pub fn list_food<'a>(
        &self,
        term: &'a Option<&str>,
    ) -> Result<impl Iterator<Item = Result<Food>> + 'a> {
        self.list::<Food>(&self.food_dir, &term)
    }

    pub fn remove_food<'a>(&self, key: &str) -> Result<()> {
        Ok(std::fs::remove_file(
            &self.food_dir.join(key).with_extension("toml"),
        )?)
    }

    pub fn write_food(&self, key: &str, food: &Food) -> Result<()> {
        self.write(&self.food_dir.join(key).with_extension("toml"), food)
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
        let content = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                bail!("Failed to open '{path:?}': {e}")
            }
        };

        let mut rows = vec![];
        for line in content.lines() {
            match line.split_once(":") {
                Some((food, serving)) => rows.push((food.trim().into(), serving.parse()?)),
                None => rows.push((line.trim().into(), Serving::default())),
            }
        }
        Ok(Some(Journal(rows)))
    }

    pub fn save_journal(&self, key: &impl Datelike, journal: &Journal) -> Result<()> {
        let path = &self.journal_path(key);
        let file = fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        for (food, serving) in &journal.0 {
            writeln!(writer, "{food}: {serving}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
                "name: Cereal",
                "carb: 22",
                "fat: 0.5",
                "protein: 1.2",
                "kcal: 120",
                "serving: 50 g",
                "serving: 2.5 cups",
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
            ["cookies = 1.0", "crackers = 0.5 cups", "cereal = 50.0 g",].join("\n")
        );
    }
}

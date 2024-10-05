use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

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
    // For example, the following says that 1 serving is 100g or "14 chips":
    // ```
    // [portions]
    // g = 100.0
    // chips = 14
    // ```
    #[tabled(skip)]
    pub servings: HashMap<String, f32>,
}

// Journal is a record of food consumed during a day.
#[derive(Serialize, Deserialize, Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Journal(pub HashMap<String, f32>);

// Data provides access to the nosh "database".
// Nosh stores all of it's data as TOML files using a particular directory structure:
// - $root/ (typically XDG_DATA_HOME)
//
//   - food/
//     - apple.toml
//     - banana.toml
//
//   - recipe/
//     - cake.toml
//     - pie.toml
//
//   - journal/
//     - 2024/
//       - 01/
//         - 01.toml
//         - 02.toml
//     - 2023/
//       - 12/
//         - 30.toml
//         - 31.toml
#[derive(Debug)]
pub struct Data {
    food_dir: PathBuf,
    recipe_dir: PathBuf,
    journal_dir: PathBuf,
}

impl Data {
    // Create a new database at the given root directory.
    pub fn new(root_dir: &Path) -> Result<Data> {
        let food_dir = root_dir.join("food");
        let recipe_dir = root_dir.join("recipe");
        let journal_dir = root_dir.join("journal");
        fs::create_dir_all(&food_dir)?;
        fs::create_dir_all(&recipe_dir)?;
        fs::create_dir_all(&journal_dir)?;
        Ok(Data {
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

    fn read<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<Option<T>> {
        log::trace!("Reading {path:?}");
        match fs::read_to_string(&path) {
            Ok(s) => Ok(Some(toml::from_str(&s)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| "Opening {path:?}"),
        }
    }

    fn write<T: serde::Serialize + std::fmt::Debug>(&self, path: &Path, obj: &T) -> Result<()> {
        log::trace!("Writing to {path:?}: {obj:?}");
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| anyhow!("Missing parent dir: {path:?}"))?,
        )?;
        Ok(fs::write(&path, toml::to_string_pretty(obj)?)?)
    }

    pub fn read_food(&self, key: &str) -> Result<Option<Food>> {
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
                    food.servings.insert(unit.into(), size);
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
        for (size, unit) in &food.servings {
            writeln!(writer, "serving: {size} {unit}")?;
        }
        Ok(())
    }

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

    fn journal_path(&self, date: &impl chrono::Datelike) -> PathBuf {
        self.journal_dir
            .join(format!("{:04}", date.year()))
            .join(format!("{:02}", date.month()))
            .join(format!("{:02}", date.day()))
            .with_extension("toml")
    }

    // Fetch the journal for the given date.
    // Returns None if there is no journal for that date.
    pub fn journal(&self, date: &impl chrono::Datelike) -> Result<Option<Journal>> {
        self.read(&self.journal_path(date))
    }

    pub fn write_journal(&self, date: &impl chrono::Datelike, journal: &Journal) -> Result<()> {
        self.write(&self.journal_path(date), journal)
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

    fn setup() -> (Data, tempfile::TempDir) {
        let _ = env_logger::try_init();
        let tmp = tempfile::tempdir().unwrap();
        let data = Data::new(tmp.path()).unwrap();
        cp("tests/testdata", &tmp);
        (data, tmp)
    }

    #[test]
    fn test_read_food() {
        let (data, _tmp) = setup();
        let oats = data.read_food("oats").unwrap().unwrap();
        assert_eq!(oats.name, "Oats");
        assert_eq!(oats.nutrients.carb, 30.0);
        assert_eq!(oats.nutrients.fat, 2.5);
        assert_eq!(oats.nutrients.protein, 5.0);
        assert_eq!(oats.nutrients.kcal, 162.0);
        assert_eq!(oats.servings["cups"], 0.5);
        assert_eq!(oats.servings["g"], 50.0);
    }

    #[test]
    fn test_save_food() {
        let (data, tmp) = setup();
        let banana = Food {
            name: "Banana".into(),
            nutrients: Nutrients {
                carb: 22.0,
                fat: 0.5,
                protein: 1.2,
                kcal: 120.0,
            },
            servings: [("g".into(), 50.0), ("cups".into(), 2.5)].into(),
        };
        data.save_food("banana", &banana).unwrap();
        let res = fs::read_to_string(tmp.path().join("food/banana.txt")).unwrap();
        assert_eq!(
            res,
            [
                "name: Banana",
                "carb: 22",
                "fat: 0.5",
                "protein: 1.2",
                "kcal: 162",
                "serving: 50.0 g",
                "serving: 2.5 cups",
            ]
            .join("\n")
        );
    }

    #[test]
    fn test_food_data() {
        let tmp = tempfile::tempdir().unwrap();
        let data = Data::new(tmp.path()).unwrap();

        let expected = Food {
            name: "Oats".into(),
            nutrients: Nutrients {
                carb: 68.7,
                fat: 5.89,
                protein: 13.5,
                kcal: 382.0,
            },
            servings: HashMap::from([("g".into(), 100.0)]),
        };

        data.write_food("oats", &expected).unwrap();
        let actual = data.read_food("oats").unwrap().unwrap();
        assert_eq!(expected, actual);

        assert_eq!(
            expected,
            data.list_food(&None).unwrap().next().unwrap().unwrap()
        );
        assert_eq!(
            expected,
            data.list_food(&Some("oat"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
        );
        assert!(data.list_food(&Some("nope")).unwrap().next().is_none());

        data.remove_food("oats").unwrap();
        assert!(data.list_food(&Some("oats")).unwrap().next().is_none());
    }

    #[test]
    fn test_journal_data() {
        let tmp = tempfile::tempdir().unwrap();
        let data = Data::new(tmp.path()).unwrap();

        let expected = Journal(HashMap::from([
            ("banana".to_string(), 1.0),
            ("oats".to_string(), 2.0),
            ("peanut_butter".to_string(), 1.5),
        ]));

        let date = &chrono::NaiveDate::from_ymd_opt(2024, 04, 02).unwrap();
        data.write_journal(&date.clone(), &expected).unwrap();
        let actual = data.journal(date).unwrap().unwrap();
        assert_eq!(expected, actual);
    }
}

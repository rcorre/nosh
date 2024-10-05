use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// The macronutrients of a food.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, tabled::Tabled)]
pub struct Nutrients {
    pub carb: f32,
    pub fat: f32,
    pub protein: f32,
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
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Food {
    // The display name of the food. This is shown in the UI.
    // Data files will reference the food by it's filename, not display name.
    pub name: String,

    // The macronutrients of this food item.
    pub nutrients: Nutrients,

    // Ways of describing a single serving of this food.
    // For example, the following says that 1 serving is 100g or "14 chips":
    // ```
    // [portions]
    // g = 100.0
    // chips = 14
    // ```
    pub servings: HashMap<String, f32>,
}

// Data provides access to the nom "database".
// Nom stores all of it's data as TOML files using a particular directory structure:
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
    // Create a new database from the given root directory.
    pub fn new(root_dir: &Path) -> Data {
        Data {
            food_dir: root_dir.join("food"),
            recipe_dir: root_dir.join("recipe"),
            journal_dir: root_dir.join("journal"),
        }
    }

    // Ensure all necessary subdirectories exist.
    pub fn create_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.food_dir)?;
        fs::create_dir_all(&self.recipe_dir)?;
        fs::create_dir_all(&self.journal_dir)?;
        Ok(())
    }

    fn read<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<Option<T>> {
        log::trace!("Reading {path:?}");
        match fs::read_to_string(&path) {
            Ok(s) => Ok(Some(toml::from_str(&s)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| "Opening {path:?}"),
        }
    }

    pub fn food(&self, key: &str) -> Result<Option<Food>> {
        self.read(&self.food_dir.join(key).with_extension("toml"))
    }

    pub fn write_food(&self, key: &str, food: &Food) -> Result<()> {
        Ok(fs::write(
            self.food_dir.join(key).with_extension("toml"),
            toml::to_string_pretty(food)?,
        )?)
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
        Ok(fs::write(
            self.journal_path(date),
            toml::to_string_pretty(journal)?,
        )?)
    }
}

// Journal is a record of food consumed during a day.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Journal(pub HashMap<String, f32>);

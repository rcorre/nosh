use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use toml::value::Date;

const APP_NAME: &'static str = "nom";

// Macro describes the macronutrients of a food.
#[derive(Deserialize, Debug)]
struct Nutrients {
    carb: Option<f32>,
    fat: Option<f32>,
    protein: Option<f32>,
    kcal: Option<f32>,
}

impl Nutrients {
    // Grams of carbohydrates per serving.
    // Defaults to 0.
    pub fn carb(&self) -> f32 {
        self.carb.unwrap_or_default()
    }

    // Grams of fat per serving.
    // Defaults to 0.
    pub fn fat(&self) -> f32 {
        self.fat.unwrap_or_default()
    }

    // Grams of protein per serving.
    // Defaults to 0.
    pub fn protein(&self) -> f32 {
        self.protein.unwrap_or_default()
    }

    // KiloCalories (or "big C Calories") per serving.
    // If omitted, defaults to the Atwater General Calculation:
    // 4*carb + 4*protein + 9*fat.
    // Note that there is a newer system that uses food-specific multipliers:
    // See https://en.wikipedia.org/wiki/Atwater_system#Modified_system.
    pub fn kcal(&self) -> f32 {
        self.kcal
            .unwrap_or_else(|| self.carb() * 4.0 + self.protein() * 4.0 + self.fat() * 9.0)
    }
}

#[test]
fn test_nutrient_defaults() {
    let nut = Nutrients {
        carb: None,
        fat: None,
        protein: None,
        kcal: None,
    };

    assert_eq!(nut.carb(), 0.0);
    assert_eq!(nut.fat(), 0.0);
    assert_eq!(nut.protein(), 0.0);
    assert_eq!(nut.kcal(), 0.0);
}

#[test]
fn test_nutrient_values() {
    let nut = Nutrients {
        carb: Some(1.2),
        fat: Some(2.3),
        protein: Some(3.1),
        kcal: Some(124.5),
    };

    assert_eq!(nut.carb(), 1.2);
    assert_eq!(nut.fat(), 2.3);
    assert_eq!(nut.protein(), 3.1);
    assert_eq!(nut.kcal(), 124.5);
}

#[test]
fn test_nutrient_kcal_computation() {
    let nut = Nutrients {
        carb: Some(1.2),
        fat: Some(2.3),
        protein: Some(3.1),
        kcal: None,
    };

    assert_eq!(nut.kcal(), 37.9);
}

// Food describes a single food item.
#[derive(Deserialize, Debug)]
pub struct Food {
    // The display name of the food. This is shown in the UI.
    // Data files will reference the food by it's filename, not display name.
    name: String,

    // The macronutrients of this food item.
    nutrients: Nutrients,

    // Ways of describing a single serving of this food.
    // For example, the following says that 1 serving is 100g or "14 chips":
    // ```
    // [portions]
    // g = 100.0
    // chips = 14
    // ```
    servings: HashMap<String, f32>,
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

    pub fn food(&self, key: &str) -> Result<Food> {
        let path = self.food_dir.join(key).with_extension("toml");
        let raw = fs::read_to_string(&path).with_context(|| format!("Reading {path:?}"))?;
        Ok(toml::from_str(&raw)?)
    }

    pub fn journal(&self, date: &toml::value::Date) -> Result<Journal> {
        let path = self
            .journal_dir
            .join(format!("{:04}", date.year))
            .join(format!("{:02}", date.month))
            .join(format!("{:02}", date.day))
            .with_extension("toml");
        let raw = fs::read_to_string(&path).with_context(|| format!("Reading {path:?}"))?;
        Ok(toml::from_str(&raw)?)
    }
}

// Journal is a record of food consumed during a day.
#[derive(Deserialize, Debug)]
pub struct Journal(HashMap<String, f32>);

fn main() -> Result<()> {
    env_logger::init();

    let dirs = xdg::BaseDirectories::new()?;
    let data = Data::new(&dirs.create_data_directory(APP_NAME)?);
    data.create_dirs()?;
    log::debug!("Created directories: {data:?}");

    let food = data.food("banana");
    log::info!("Food: {food:?}");

    let journal = data.journal(&Date {
        year: 2024,
        month: 01,
        day: 01,
    })?;
    log::info!("Journal: {journal:?}");

    Ok(())
}

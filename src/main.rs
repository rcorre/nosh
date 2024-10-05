use anyhow::Result;
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

const APP_NAME: &'static str = "nom";
const FOOD_DIR: &'static str = "nom/food";
const RECIPE_DIR: &'static str = "nom/recipe";
const JOURNAL_DIR: &'static str = "nom/journal";

// Macro describes the macronutrients of a food.
#[derive(Deserialize, Debug)]
struct Nutrients {
    // Grams of carbohydrates per serving.
    // Defaults to 0.
    carb: Option<f32>,

    // Grams of fat per serving.
    // Defaults to 0.
    fat: Option<f32>,

    // Grams of protein per serving.
    // Defaults to 0.
    protein: Option<f32>,

    // KiloCalories (or "big C Calories") per serving.
    // If omitted, defaults to the Atwater General Calculation:
    // 4*carb + 4*protein + 9*fat.
    // Note that there is a newer system that uses food-specific multipliers:
    // See https://en.wikipedia.org/wiki/Atwater_system#Modified_system.
    kcal: Option<f32>,
}

impl Nutrients {
    pub fn carb(&self) -> f32 {
        self.carb.unwrap_or_default()
    }
    pub fn fat(&self) -> f32 {
        self.fat.unwrap_or_default()
    }
    pub fn protein(&self) -> f32 {
        self.protein.unwrap_or_default()
    }
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
struct Food {
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

fn main() -> Result<()> {
    env_logger::init();

    let dirs = xdg::BaseDirectories::new()?;
    let food_dir = dirs.create_data_directory(FOOD_DIR)?;
    let food = fs::read_to_string(food_dir.join("banana.toml"))?;
    let food: Food = toml::from_str(&food)?;

    log::info!("Food: {food:?}");

    Ok(())
}

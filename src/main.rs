use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tabled::{
    grid::colors::Colors,
    settings::{
        object::Rows,
        panel::Header,
        style::{HorizontalLine, LineText, VerticalLine},
        themes::{Colorization, ColumnNames},
        Border, Color, Concat, Panel, Style,
    },
    Table,
};

const APP_NAME: &'static str = "nom";

#[derive(clap::Args)]
struct ShowArgs {}

#[derive(clap::Args)]
struct NomArgs {
    food: String,
    quantity: Option<f32>,
    unit: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    Nom(NomArgs),
    Show(ShowArgs),
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

// The macronutrients of a food.
#[derive(Clone, Copy, Debug, Default, Deserialize, tabled::Tabled)]
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
    assert_eq!(nut.kcal, 149.0);
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

    // Fetch the journal for the given date.
    // Returns None if there is no journal for that date.
    pub fn journal(&self, date: impl chrono::Datelike) -> Result<Option<Journal>> {
        let path = self
            .journal_dir
            .join(format!("{:04}", date.year()))
            .join(format!("{:02}", date.month()))
            .join(format!("{:02}", date.day()))
            .with_extension("toml");
        let raw = match fs::read_to_string(&path) {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::debug!("Not found: {path:?}");
                return Ok(None);
            }
            Err(e) => Err(e).with_context(|| "Opening {path:?}"),
        }?;
        //.with_context(|| format!("Reading {path:?}"))?;
        Ok(toml::from_str(&raw)?)
    }
}

// Journal is a record of food consumed during a day.
#[derive(Deserialize, Debug, Default)]
pub struct Journal(HashMap<String, f32>);

#[derive(tabled::Tabled, Default)]
struct JournalRow {
    name: String,
    serving: f32,
    #[tabled(inline)]
    nutrients: Nutrients,
}

fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let dirs = xdg::BaseDirectories::new()?;
    let data = Data::new(&dirs.create_data_directory(APP_NAME)?);

    data.create_dirs()?;
    log::debug!("Created directories: {data:?}");

    match args.command {
        Command::Nom(args) => nom(&data, args),
        Command::Show(args) => show(&data, args),
    }?;

    Ok(())
}

fn show(data: &Data, args: ShowArgs) -> Result<()> {
    let now = chrono::Local::now();
    let journal = data.journal(now)?.unwrap_or_default();
    let rows: Result<Vec<_>> = journal
        .0
        .iter()
        .map(|(key, &serving)| {
            data.food(key).map(|food| JournalRow {
                name: food.name,
                serving,
                nutrients: food.nutrients * serving,
            })
        })
        .collect();
    let rows = rows?;
    let total: Nutrients = rows
        .iter()
        .fold(Nutrients::default(), |a, b| a + b.nutrients);
    let mut total = Table::new([[
        "Total".to_string(),
        "".to_string(),
        total.carb.to_string(),
        total.fat.to_string(),
        total.protein.to_string(),
        total.kcal.to_string(),
    ]]);
    total.with(ColumnNames::default());

    let line = HorizontalLine::inherit(Style::modern());

    let table = Table::new(rows)
        .with(
            Style::modern()
                .remove_horizontals()
                .horizontals([(1, line)]),
        )
        .with(Concat::vertical(total))
        .with(Colorization::exact([Color::BOLD], Rows::last()))
        .to_string();

    println!("{table}");
    Ok(())
}

fn nom(data: &Data, args: NomArgs) -> Result<()> {
    Ok(())
}

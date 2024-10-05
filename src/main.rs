use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tabled::{
    settings::{
        object::Rows,
        style::HorizontalLine,
        themes::{Colorization, ColumnNames},
        Color, Concat, Style,
    },
    Table,
};

const APP_NAME: &'static str = "nom";

#[derive(Subcommand)]
enum FoodCommand {
    Edit { key: String },
    Show { key: String },
}

#[derive(Subcommand)]
enum JournalCommand {
    Edit { key: String },
    Show { key: String },
}

#[derive(Subcommand)]
enum RecipeCommand {
    Edit { key: String },
    Show { key: String },
}

#[derive(Subcommand)]
enum Command {
    Nom {
        food: String,
        serving: Option<String>,
    },
    Food {
        #[command(subcommand)]
        command: FoodCommand,
    },
    Recipe {
        #[command(subcommand)]
        command: RecipeCommand,
    },
    Journal {
        #[command(subcommand)]
        command: JournalCommand,
    },
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

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
#[derive(Serialize, Deserialize, Debug, Default)]
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

    // Fetch the journal for the given date.
    // Returns None if there is no journal for that date.
    pub fn journal(&self, date: impl chrono::Datelike) -> Result<Option<Journal>> {
        self.read(
            &self
                .journal_dir
                .join(format!("{:04}", date.year()))
                .join(format!("{:02}", date.month()))
                .join(format!("{:02}", date.day()))
                .with_extension("toml"),
        )
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
        Command::Nom { food, serving } => nom(&data, &food, serving),
        Command::Food { command } => match command {
            FoodCommand::Edit { key } => edit_food(&data, &key),
            FoodCommand::Show { key } => show_food(&data, &key),
        },
        Command::Recipe { command } => match command {
            RecipeCommand::Edit { key } => todo!(),
            RecipeCommand::Show { key } => todo!(),
        },
        Command::Journal { command } => match command {
            JournalCommand::Edit { key } => todo!(),
            JournalCommand::Show { key } => show_journal(&data),
        },
    }?;

    Ok(())
}

fn show_journal(data: &Data) -> Result<()> {
    let now = chrono::Local::now();
    let journal = data.journal(now)?.unwrap_or_default();
    let rows: Result<Vec<_>> = journal
        .0
        .iter()
        .map(|(key, serving)| (data.food(key), serving))
        .map(|(food, &serving)| match food {
            Ok(Some(food)) => Ok(JournalRow {
                name: food.name,
                serving,
                nutrients: food.nutrients * serving,
            }),
            Ok(None) => Err(anyhow::format_err!("Food not found")),
            Err(err) => Err(err),
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

fn nom(data: &Data, food: &str, serving: Option<String>) -> Result<()> {
    Ok(())
}

fn edit_food(data: &Data, key: &str) -> Result<()> {
    let food = data.food(key)?.unwrap_or_default();
    let mut tmp = tempfile::Builder::new().suffix(".toml").tempfile()?;
    tmp.write_all(toml::to_string_pretty(&food)?.as_bytes())?;
    tmp.flush()?;
    log::debug!("Wrote {food:?} to {tmp:?}");

    let editor = std::env::var("EDITOR").context("EDITOR not set")?;
    let editor = which::which(editor)?;
    let mut cmd = std::process::Command::new(editor);
    cmd.arg(tmp.path())
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    log::debug!("Running {cmd:?}");

    let status = cmd.spawn()?.wait()?;
    anyhow::ensure!(status.success(), "Editor exited with code: {status:?}");

    let mut input = String::new();
    let mut file = fs::File::open(tmp.path())?;
    file.read_to_string(&mut input)?;
    log::debug!("Read edited food: {input}");

    let food: Food = toml::from_str(&input)?;
    log::debug!("Parsed edited food: {food:?}");

    data.write_food(key, &food)
}

fn show_food(data: &Data, key: &str) -> Result<()> {
    let Some(food) = data.food(key)? else {
        println!("No food with key {key:?}");
        return Ok(());
    };

    println!("{food:#?}");
    Ok(())
}

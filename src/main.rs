use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use nosh::{Data, Nutrients, APP_NAME};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
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

#[derive(Subcommand)]
enum FoodCommand {
    Edit { key: String },
    Show { key: String },
    Ls { term: Option<String> },
    Rm { key: String },
    Search { key: String, term: Option<String> },
}

#[derive(Subcommand)]
enum JournalCommand {
    Edit { key: Option<String> },
    Show { key: Option<String> },
}

#[derive(Subcommand)]
enum RecipeCommand {
    Edit { key: String },
    Show { key: String },
}

#[derive(Subcommand)]
enum Command {
    Eat {
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

    match args.command {
        Command::Eat { food, serving } => nosh(&data, food, serving),
        Command::Food { command } => match command {
            FoodCommand::Edit { key } => edit_food(&data, &key),
            FoodCommand::Show { key } => show_food(&data, &key),
            FoodCommand::Search { key, term } => search_food(&data, key, term),
            FoodCommand::Ls { term } => list_food(&data, term),
            FoodCommand::Rm { key } => rm_food(&data, key),
        },
        Command::Recipe { command } => match command {
            RecipeCommand::Edit { key } => todo!(),
            RecipeCommand::Show { key } => todo!(),
        },
        Command::Journal { command } => match command {
            JournalCommand::Edit { key } => edit_journal(&data, key),
            JournalCommand::Show { key } => show_journal(&data, key),
        },
    }?;

    Ok(())
}

fn edit_journal(data: &Data, key: Option<String>) -> Result<()> {
    let date = match key {
        Some(key) => chrono::NaiveDate::parse_from_str(&key, "%Y-%m-%d")?,
        None => chrono::Local::now().date_naive(),
    };
    let journal = data.journal(&date)?.unwrap_or_default();
    let journal = edit(&journal)?;
    data.write_journal(&date, &journal)
}

fn show_journal(data: &Data, key: Option<String>) -> Result<()> {
    let date = match key {
        Some(key) => chrono::NaiveDate::parse_from_str(&key, "%Y-%m-%d")?,
        None => chrono::Local::now().date_naive(),
    };
    let journal = data.journal(&date)?.unwrap_or_default();
    let rows: Result<Vec<_>> = journal
        .0
        .iter()
        .map(|(key, serving)| (data.get_food(key), serving))
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
        format!("{:.1}", total.carb),
        format!("{:.1}", total.fat),
        format!("{:.1}", total.protein),
        format!("{:.0}", total.kcal),
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

fn nosh(data: &Data, key: String, serving: Option<String>) -> Result<()> {
    let Some(_food) = data.get_food(&key)? else {
        bail!("No food with key {key:?}");
    };
    let serving = if let Some(serving) = serving {
        if let Some((amount, _unit)) = serving.split_once(|c: char| !c.is_digit(10) && c != '.') {
            amount.parse()?
        } else {
            serving.parse()?
        }
    } else {
        1.0
    };

    let date = chrono::Local::now();
    log::debug!("Adding food={key} serving={serving} to {date:?}");

    let mut journal = data.journal(&date)?.unwrap_or_default();
    journal
        .0
        .entry(key)
        .and_modify(|x| *x += serving)
        .or_insert(serving);

    data.write_journal(&date, &journal)
}

fn edit<T: Serialize + DeserializeOwned + std::fmt::Debug>(orig: &T) -> Result<T> {
    let mut tmp = tempfile::Builder::new().suffix(".toml").tempfile()?;
    tmp.write_all(toml::to_string_pretty(&orig)?.as_bytes())?;
    tmp.flush()?;
    log::debug!("Wrote {orig:?} to {tmp:?}");

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
    log::debug!("Read: {input}");

    let new: T = toml::from_str(&input)?;
    log::debug!("Parsed: {new:?}");
    Ok(new)
}

fn edit_food(data: &Data, key: &str) -> Result<()> {
    let food = data.get_food(key)?.unwrap_or_default();
    let food = edit(&food)?;
    data.write_food(key, &food)
}

fn show_food(data: &Data, key: &str) -> Result<()> {
    match data.get_food(key)? {
        Some(food) => {
            println!("{food:#?}");
            Ok(())
        }
        None => Err(anyhow!("No food with key {key:?}")),
    }
}

// TODO: include keys in data. Probably want to make key a non-serialized struct field.
fn list_food(data: &Data, term: Option<String>) -> Result<()> {
    let term = term.as_ref().map(|s| s.as_str());
    let items = data.list_food(&term)?;
    let mut table = Table::new(items.filter_map(|x| match x {
        Ok(food) => Some(food),
        Err(err) => {
            log::error!("Failed to list food: {err:?}");
            None
        }
    }));
    println!("{}", table.with(Style::sharp()));
    Ok(())
}

fn rm_food(data: &Data, key: String) -> Result<()> {
    data.remove_food(&key)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchNutrient {
    nutrient_id: u32,
    value: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchFood {
    description: Option<String>,
    serving_size: Option<f32>,
    serving_size_unit: Option<String>,
    food_nutrients: Option<Vec<SearchNutrient>>,
    // household_serving_full_text: String,
}

impl SearchFood {
    const NUTRIENT_ID_PROTEIN: u32 = 1003; //Protein
    const NUTRIENT_ID_FAT: u32 = 1004; //Total lipid (fat)
    const NUTRIENT_ID_CARB_DIFFERENCE: u32 = 1005; //Carbohydrate, by difference
    const NUTRIENT_ID_ENERGY_KCAL: u32 = 1008; // Energy
    const NUTRIENT_ID_CARB_SUMMATION: u32 = 1050; //Carbohydrate, by summation

    fn nutrient(&self, id: u32) -> Option<f32> {
        match &self.food_nutrients {
            Some(n) => n.iter().find(|x| x.nutrient_id == id).map(|x| x.value),
            None => None,
        }
    }

    fn nutrients(&self) -> Nutrients {
        Nutrients {
            carb: self
                .nutrient(Self::NUTRIENT_ID_CARB_DIFFERENCE)
                .or(self.nutrient(Self::NUTRIENT_ID_CARB_SUMMATION))
                .unwrap_or_default(),
            fat: self.nutrient(Self::NUTRIENT_ID_FAT).unwrap_or_default(),
            protein: self.nutrient(Self::NUTRIENT_ID_PROTEIN).unwrap_or_default(),
            kcal: self
                .nutrient(Self::NUTRIENT_ID_ENERGY_KCAL)
                .unwrap_or_default(),
        }
    }

    fn servings(&self) -> HashMap<String, f32> {
        // TODO: Parse household serving
        match (&self.serving_size_unit, self.serving_size) {
            (Some(unit), Some(size)) => [(unit.clone(), size)].into(),
            _ => [].into(),
        }
    }
}

impl From<&SearchFood> for nosh::Food {
    fn from(value: &SearchFood) -> Self {
        nosh::Food {
            nutrients: value.nutrients(),
            servings: value.servings(),
            name: value.description.clone().unwrap_or_default(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    foods: Vec<SearchFood>,
}

fn search_food(data: &Data, key: String, term: Option<String>) -> Result<()> {
    if data.get_food(&key)?.is_some() {
        bail!("Food with key {key} already exists");
    }

    let term = term.unwrap_or(key.clone());

    let client = reqwest::blocking::Client::new();
    let req = client
        .get("https://api.nal.usda.gov/fdc/v1/foods/search")
        .header("X-Api-Key", "DEMO_KEY")
        .query(&[("query", term)])
        .build()?;

    log::debug!("Sending request: {req:?}");

    #[derive(tabled::Tabled)]
    struct Index(#[tabled(rename = "index")] usize);
    let res: SearchResponse = client.execute(req)?.error_for_status()?.json()?;
    let foods: Vec<_> = res
        .foods
        .iter()
        .enumerate()
        .map(|(i, x)| (Index(i), nosh::Food::from(x)))
        .collect();

    let table = Table::new(&foods).with(Style::sharp()).to_string();
    println!("{table}");

    print!("\n[0-{}]? ", foods.len());
    std::io::stdout().flush()?;

    let mut res = String::new();
    std::io::stdin().read_line(&mut res)?;
    let res = res.trim();

    if res.is_empty() {
        log::debug!("Empty response, not adding any food");
        return Ok(());
    }

    let idx: usize = res.parse()?;
    let (_, food) = foods.get(idx).ok_or(anyhow!("Index out of range"))?;
    data.write_food(&key, &food)?;
    println!("Added '{}' as {key}", food.name);

    Ok(())
}

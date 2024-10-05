use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use nom::{Data, Nutrients};
use serde::{de::DeserializeOwned, Serialize};
use std::{
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

const APP_NAME: &'static str = "nom";

#[derive(Subcommand)]
enum FoodCommand {
    Edit { key: String },
    Show { key: String },
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
        Command::Nom { food, serving } => nom(&data, food, serving),
        Command::Food { command } => match command {
            FoodCommand::Edit { key } => edit_food(&data, &key),
            FoodCommand::Show { key } => show_food(&data, &key),
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

fn nom(data: &Data, key: String, serving: Option<String>) -> Result<()> {
    let Some(_food) = data.food(&key)? else {
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
    let food = data.food(key)?.unwrap_or_default();
    let food = edit(&food)?;
    data.write_food(key, &food)
}

fn show_food(data: &Data, key: &str) -> Result<()> {
    match data.food(key)? {
        Some(food) => {
            println!("{food:#?}");
            Ok(())
        }
        None => Err(anyhow!("No food with key {key:?}")),
    }
}

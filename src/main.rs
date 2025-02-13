use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use nosh::{Database, Food, JournalEntry, Nutrients, Serving, APP_NAME};
use std::{fs, io::Write};
use tabled::{
    settings::{
        object::Rows,
        style::HorizontalLine,
        themes::{Colorization, ColumnNames},
        Color, Concat, Style,
    },
    Table,
};
use terminal_size::{terminal_size, Height};

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
enum Command {
    Eat {
        food: String,
        serving: Option<String>,
    },
    Food {
        #[command(subcommand)]
        command: FoodCommand,
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

fn float0(f: &f32) -> String {
    format!("{:.0}", f)
}

fn float1(f: &f32) -> String {
    format!("{:.1}", f)
}

// The macronutrients of a food, adapted for display purposes.
#[derive(tabled::Tabled)]
#[cfg_attr(test, derive(PartialEq))]
pub struct NutrientsRow {
    #[tabled(display_with = "float1")]
    pub carb: f32,
    #[tabled(display_with = "float1")]
    pub fat: f32,
    #[tabled(display_with = "float1")]
    pub protein: f32,
    #[tabled(display_with = "float0")]
    pub kcal: f32,
}

impl From<Nutrients> for NutrientsRow {
    fn from(value: Nutrients) -> Self {
        Self {
            carb: value.carb,
            fat: value.fat,
            protein: value.protein,
            kcal: value.kcal,
        }
    }
}

#[derive(tabled::Tabled)]
struct FoodRow {
    key: String,
    name: String,
    #[tabled(inline)]
    nutrients: NutrientsRow,
    servings: String,
}

impl FoodRow {
    fn new(key: &str, food: &Food) -> Self {
        Self {
            key: key.into(),
            nutrients: food.nutrients().into(),
            name: food.name.clone(),
            servings: food
                .servings
                .iter()
                .map(|(unit, amount)| format!("{amount}{unit}"))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

#[derive(tabled::Tabled)]
struct JournalRow {
    name: String,
    serving: Serving,
    #[tabled(inline)]
    nutrients: NutrientsRow,
}

fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let dirs = xdg::BaseDirectories::new()?;
    let data = Database::new(&dirs.create_data_directory(APP_NAME)?)?;

    match args.command {
        Command::Eat { food, serving } => eat(&data, food, serving),
        Command::Food { command } => match command {
            FoodCommand::Edit { key } => edit_food(&data, &key),
            FoodCommand::Show { key } => show_food(&data, &key),
            FoodCommand::Search { key, term } => search_food(&data, key, term),
            FoodCommand::Ls { term } => list_food(&data, term),
            FoodCommand::Rm { key } => rm_food(&data, key),
        },
        Command::Journal { command } => match command {
            JournalCommand::Edit { key } => edit_journal(&data, key),
            JournalCommand::Show { key } => show_journal(&data, key),
        },
    }?;

    Ok(())
}

fn edit_journal(data: &Database, key: Option<String>) -> Result<()> {
    let date = match key {
        Some(key) => chrono::NaiveDate::parse_from_str(&key, "%Y-%m-%d")?,
        None => chrono::Local::now().date_naive(),
    };
    let journal = data.load_journal(&date)?.unwrap_or_default();
    let journal = edit(&journal, &data)?;
    data.save_journal(&date, &journal)
}

fn show_journal(data: &Database, key: Option<String>) -> Result<()> {
    let date = match key {
        Some(key) => chrono::NaiveDate::parse_from_str(&key, "%Y-%m-%d")?,
        None => chrono::Local::now().date_naive(),
    };
    let journal = data.load_journal(&date)?.unwrap_or_default();
    let rows: Result<Vec<_>> = journal
        .0
        .iter()
        .map(|entry| {
            Ok(JournalRow {
                serving: entry.serving.clone(),
                nutrients: entry.food.serve(&entry.serving)?.into(),
                name: entry.food.name.clone(),
            })
        })
        .collect();
    let rows = rows?;
    let total: NutrientsRow = journal.nutrients()?.into();
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

fn eat(data: &Database, key: String, serving: Option<String>) -> Result<()> {
    let Some(food) = data.load_food(&key)? else {
        bail!("No food with key {key:?}");
    };
    let serving = match serving {
        Some(s) => s.parse()?,
        None => Serving::default(),
    };
    if let Err(err) = food.serve(&serving) {
        bail!("Invalid serving: {err:?}");
    };

    let date = chrono::Local::now().date_naive();
    log::debug!("Adding food={key} serving={serving} to {date:?}");

    let mut journal = data.load_journal(&date)?.unwrap_or_default();
    journal.0.push(JournalEntry { key, serving, food });
    data.save_journal(&date, &journal)
}

fn edit<T: nosh::Data + std::fmt::Debug>(orig: &T, data: &Database) -> Result<T> {
    let mut tmp = tempfile::Builder::new().suffix(".txt").tempfile()?;
    orig.save(&mut std::io::BufWriter::new(&tmp))?;
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

    let file = fs::File::open(tmp.path())?;
    let reader = std::io::BufReader::new(file);
    let new = T::load(reader, |key| data.load_food(key))?;
    log::debug!("Parsed: {new:?}");
    Ok(new)
}

fn edit_food(data: &Database, key: &str) -> Result<()> {
    let food = data.load_food(key)?.unwrap_or_default();
    let food = edit(&food, &data)?;
    data.save_food(key, &food)
}

fn show_food(data: &Database, key: &str) -> Result<()> {
    let Some(food) = data.load_food(key)? else {
        bail!("No food with key {key:?}");
    };
    let food = FoodRow::new(key, &food);
    let mut table = Table::new(std::iter::once(food));
    println!("{}", table.with(Style::sharp()));
    Ok(())
}

fn list_food(data: &Database, pattern: Option<String>) -> Result<()> {
    let pattern = pattern.unwrap_or("".to_string());
    log::debug!("Listing food matching '{pattern}'");
    let items = data.list_food()?;
    let mut items: Vec<_> = items
        .filter_map(|x| match x {
            Ok(key) if key.contains(&pattern) => match data.load_food(&key) {
                Ok(Some(food)) => Some((key, food)),
                Ok(None) => {
                    // Should be there, as we just listed it.
                    // Maybe something messed with the DB out of sync.
                    log::error!("Food '{key}' not found");
                    None
                }
                Err(err) => {
                    log::error!("Failed to load food '{key}': {err:?}");
                    None
                }
            },
            Ok(key) => {
                log::trace!("Food '{key}' does not match '{pattern}'");
                None
            }
            Err(err) => {
                log::error!("Failed to list food: {err:?}");
                None
            }
        })
        .map(|(key, food)| FoodRow::new(&key, &food))
        .collect();
    items.sort_by(|a, b| a.key.cmp(&b.key));
    if !items.is_empty() {
        let mut table = Table::new(items);
        println!("{}", table.with(Style::sharp()));
    }
    Ok(())
}

fn rm_food(data: &Database, key: String) -> Result<()> {
    data.remove::<Food>(&key)
}

fn search_food(data: &Database, key: String, term: Option<String>) -> Result<()> {
    if data.load_food(&key)?.is_some() {
        bail!("Food with key {key} already exists");
    }

    let term = term.unwrap_or(key.clone());

    let mut search = nosh::Search {
        term: &term,
        ..Default::default()
    };

    // Show only as many results as will fit on screen.
    if let Some((_, Height(h))) = terminal_size() {
        log::debug!("Terminal height is {h}");
        // Subtract 6 to leave room for headers/footers.
        search.page_size = h.saturating_sub(6) as usize;
    } else {
        log::warn!("Unable to get terminal size");
    }

    // This is mostly here to allow injecting a url for testing.
    let url = std::env::var("NOSH_SEARCH_URL").ok();
    if let Some(url) = url.as_ref() {
        search.url = &url;
    };

    loop {
        let foods = search.next_page()?;
        let foods: Vec<_> = foods.iter().collect();

        if foods.is_empty() {
            bail!("Found no foods matching '{term}'");
        }

        let table: Vec<_> = foods
            .iter()
            .enumerate()
            .map(|(i, food)| FoodRow::new(&i.to_string(), food))
            .collect();

        let table = Table::new(&table).with(Style::sharp()).to_string();
        println!("{table}");

        print!("\n[0-{}],(n)ext,(q)uit? ", foods.len().saturating_sub(1));
        std::io::stdout().flush()?;

        let mut res = String::new();
        std::io::stdin().read_line(&mut res)?;
        let res = res.trim();

        if res.is_empty() {
            log::debug!("Empty response, not adding any food");
            return Ok(());
        }

        if res.starts_with("q") {
            log::debug!("Quit requested, not adding any food");
            return Ok(());
        } else if res.starts_with("n") {
            log::debug!("Getting next page of results");
            continue;
        }

        let idx: usize = res.parse()?;
        let food = foods.get(idx).ok_or(anyhow!("Index out of range"))?;
        data.save_food(key.as_str(), food)?;
        println!("Added '{}' as {key}", food.name);
        return Ok(());
    }
}

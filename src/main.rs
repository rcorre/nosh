use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use nosh::{Database, Food, Journal, Nutrients, Recipe, Serving, APP_NAME};
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
    serving: Serving,
    #[tabled(inline)]
    nutrients: Nutrients,
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
        Command::Recipe { command } => match command {
            RecipeCommand::Edit { key } => edit_recipe(&data, &key),
            RecipeCommand::Show { key } => show_recipe(&data, &key),
        },
        Command::Journal { command } => match command {
            JournalCommand::Edit { key } => edit_journal(&data, key),
            JournalCommand::Show { key } => show_journal(&data, key),
        },
    }?;

    Ok(())
}

fn edit_recipe(data: &Database, key: &str) -> Result<()> {
    let recipe = data.load::<Recipe>(&key)?.unwrap_or_default();
    let recipe = edit(&recipe)?;
    data.save(key, &recipe)
}

fn show_recipe(data: &Database, key: &str) -> Result<()> {
    let journal = data.load::<Recipe>(key)?.unwrap_or_default();
    let rows: Result<Vec<_>> = journal
        .0
        .iter()
        .map(|(key, serving)| (key, data.load::<Food>(key), serving))
        .map(|(key, food, serving)| match food {
            Ok(Some(food)) => Ok(JournalRow {
                serving: serving.clone(),
                nutrients: food.serve(serving)?,
                name: food.name,
            }),
            Ok(None) => Err(anyhow::format_err!("Food not found: {key}")),
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

fn edit_journal(data: &Database, key: Option<String>) -> Result<()> {
    let date = match key {
        Some(key) => chrono::NaiveDate::parse_from_str(&key, "%Y-%m-%d")?,
        None => chrono::Local::now().date_naive(),
    };
    let journal = data.load::<Journal>(&date)?.unwrap_or_default();
    let journal = edit(&journal)?;
    data.save(&date, &journal)
}

fn show_journal(data: &Database, key: Option<String>) -> Result<()> {
    let date = match key {
        Some(key) => chrono::NaiveDate::parse_from_str(&key, "%Y-%m-%d")?,
        None => chrono::Local::now().date_naive(),
    };
    let journal = data.load::<Journal>(&date)?.unwrap_or_default();
    let rows: Result<Vec<_>> = journal
        .0
        .iter()
        .map(|(key, serving)| (key, data.load::<Food>(key), serving))
        .map(|(key, food, serving)| match food {
            Ok(Some(food)) => Ok(JournalRow {
                serving: serving.clone(),
                nutrients: food.serve(serving)?,
                name: food.name,
            }),
            Ok(None) => Err(anyhow::format_err!("Food not found: {key}")),
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

fn eat(data: &Database, key: String, serving: Option<String>) -> Result<()> {
    let Some(food) = data.load::<Food>(&key)? else {
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

    let mut journal = data.load::<Journal>(&date)?.unwrap_or_default();
    journal.0.push((key, serving));
    data.save(&date, &journal)
}

fn edit<T: nosh::Data + std::fmt::Debug>(orig: &T) -> Result<T> {
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
    let new = T::load(reader)?;
    log::debug!("Parsed: {new:?}");
    Ok(new)
}

fn edit_food(data: &Database, key: &str) -> Result<()> {
    let food = data.load::<Food>(key)?.unwrap_or_default();
    let food = edit(&food)?;
    data.save(key, &food)
}

fn show_food(data: &Database, key: &str) -> Result<()> {
    let Some(food) = data.load::<Food>(key)? else {
        bail!("No food with key {key:?}");
    };
    let mut table = Table::new(std::iter::once(food));
    println!("{}", table.with(Style::sharp()));
    Ok(())
}

// TODO: include keys in data. Probably want to make key a non-serialized struct field.
fn list_food(data: &Database, term: Option<String>) -> Result<()> {
    let term = term.as_ref().map(|s| s.as_str());
    let items = data.list::<Food>(&term)?;
    #[derive(tabled::Tabled)]
    struct Key(#[tabled(rename = "key")] String);
    let mut items: Vec<_> = items
        .filter_map(|x| match x {
            Ok((key, food)) => Some((Key(key), food)),
            Err(err) => {
                log::error!("Failed to list food: {err:?}");
                None
            }
        })
        .collect();
    items.sort_by(|a, b| a.0 .0.cmp(&b.0 .0));
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
    if data.load::<Food>(&key)?.is_some() {
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
        let mut foods = foods.iter().peekable();

        if foods.peek().is_none() {
            bail!("Found no foods matching '{term}'");
        }

        #[derive(tabled::Tabled)]
        struct Index(#[tabled(rename = "index")] usize);
        let foods: Vec<_> = foods
            .enumerate()
            .map(|(i, food)| (Index(i), food))
            .collect();

        let table = Table::new(&foods).with(Style::sharp()).to_string();
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
        let (_, food) = foods.get(idx).ok_or(anyhow!("Index out of range"))?;
        data.save(key.as_str(), food)?;
        println!("Added '{}' as {key}", food.name);
        return Ok(());
    }
}

use crate::{Data, Food, Nutrients, Serving};
use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};

#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct JournalEntry {
    pub key: String,
    pub serving: Serving,
    pub food: Food,
}

// Journal is a record of food consumed during a day.
// It is a list of "food = serving" lines.
// The serving is optional and defaults to 1.
// For example:
// ```
// oats = 0.5 cup
// banana = 1
// berries
// ```
#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Journal(pub Vec<JournalEntry>);

impl Data for Journal {
    type Key = NaiveDate;
    const DIR: &str = "journal";

    fn path(key: &NaiveDate) -> std::path::PathBuf {
        format!(
            "journal/{:04}/{:02}/{:02}.txt",
            key.year(),
            key.month(),
            key.day()
        )
        .into()
    }

    fn load(
        r: impl std::io::BufRead,
        mut load_food: impl FnMut(&str) -> Result<Option<Food>>,
    ) -> Result<Self> {
        let mut rows = vec![];
        for line in r.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (key, serving) = match line.split_once("=") {
                Some((food, serving)) => (food.trim(), serving.parse()?),
                None => (line.trim(), Serving::default()),
            };
            let food = load_food(key)?;
            let food = food.with_context(|| format!("Food not found: {key}"))?;
            // Check that the serving is actually valid for this food.
            food.serve(&serving)?;
            rows.push(JournalEntry {
                key: key.into(),
                serving,
                food,
            });
        }
        Ok(Self(rows))
    }

    fn save(&self, w: &mut impl std::io::Write) -> Result<()> {
        for JournalEntry { key, serving, .. } in &self.0 {
            writeln!(w, "{key} = {serving}")?;
        }
        Ok(())
    }
}

impl Journal {
    // Compute the total nutrients of this journal.
    pub fn nutrients(&self) -> Result<Nutrients> {
        let mut res = Nutrients::default();
        for entry in &self.0 {
            res += entry.food.serve(&entry.serving)?;
        }
        Ok(res)
    }
}

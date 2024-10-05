use crate::{Data, Food, Nutrients, Serving};
use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use ini::{Ini, WriteOption};

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
        mut r: impl std::io::BufRead,
        mut load_food: impl FnMut(&str) -> Result<Option<Food>>,
    ) -> Result<Self> {
        let mut rows = vec![];
        let ini = Ini::read_from(&mut r)?;
        log::trace!("Parsing: {ini:?}");
        for (k, v) in ini.general_section() {
            rows.push(JournalEntry {
                key: k.into(),
                serving: v.parse()?,
                food: load_food(k)?.with_context(|| format!("Food not found: {k}"))?,
            })
        }
        Ok(Self(rows))
    }

    fn save(&self, w: &mut impl std::io::Write) -> Result<()> {
        let mut ini = Ini::new();
        let mut sec = ini.with_general_section();
        for JournalEntry { key, serving, .. } in &self.0 {
            sec.add(key, serving.to_string());
        }
        log::trace!("Writing: {ini:?}");
        ini.write_to_opt(
            w,
            WriteOption {
                line_separator: ini::LineSeparator::CR,
                kv_separator: " = ",
                ..Default::default()
            },
        )?;
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

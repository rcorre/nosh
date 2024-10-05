use crate::{Data, Serving};
use anyhow::Result;
use chrono::{Datelike, NaiveDate};

// Journal is a record of food consumed during a day.
// It is a list of "food: serving" lines.
// The serving is optional and defaults to 1.
// For example:
// ```
// oats: 0.5 cup
// banana: 1
// berries
// ```
#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Journal(pub Vec<(String, Serving)>);

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

    fn load(r: impl std::io::BufRead) -> Result<Self> {
        let mut rows = vec![];
        for line in r.lines() {
            let line = line?;
            match line.split_once("=") {
                Some((food, serving)) => rows.push((food.trim().into(), serving.parse()?)),
                None => rows.push((line.trim().into(), Serving::default())),
            }
        }
        Ok(Self(rows))
    }

    fn save(&self, w: &mut impl std::io::Write) -> Result<()> {
        for (food, serving) in &self.0 {
            writeln!(w, "{food} = {serving}")?;
        }
        Ok(())
    }
}

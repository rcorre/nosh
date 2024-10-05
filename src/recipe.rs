use crate::{Data, Serving};
use anyhow::Result;

// Recipe is a collection of foods in various quantities.
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
pub struct Recipe {
    pub name: String,
    pub ingredients: Vec<(String, Serving)>,
}

impl Data for Recipe {
    type Key = str;
    const DIR: &str = "recipe";

    fn path(key: &str) -> std::path::PathBuf {
        [Self::DIR, key]
            .iter()
            .collect::<std::path::PathBuf>()
            .with_extension("txt")
    }

    fn load(r: impl std::io::BufRead) -> Result<Self> {
        let mut res = Recipe::default();
        for line in r.lines() {
            let line = line?;
            log::trace!("Parsing recipe line: {line}");
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match line.split_once("=").map(|(a, b)| (a.trim(), b.trim())) {
                Some(("name", name)) => res.name = name.into(),
                Some((food, serving)) => {
                    res.ingredients.push((food.trim().into(), serving.parse()?))
                }
                None => res
                    .ingredients
                    .push((line.trim().into(), Serving::default())),
            }
        }
        Ok(res)
    }

    fn save(&self, w: &mut impl std::io::Write) -> Result<()> {
        for (food, serving) in &self.ingredients {
            writeln!(w, "{food} = {serving}")?;
        }
        Ok(())
    }
}

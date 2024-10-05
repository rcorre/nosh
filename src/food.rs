use crate::serving::Serving;
use crate::{nutrients::Nutrients, Data};

use anyhow::{anyhow, bail, Context, Result};

fn format_servings(servings: &Vec<(String, f32)>) -> String {
    servings
        .iter()
        .map(|(unit, size)| format!("{size}{unit}"))
        .collect::<Vec<_>>()
        .join(",")
}

// Food describes a single food item.
#[derive(Debug, Default, tabled::Tabled)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Food {
    // The display name of the food. This is shown in the UI.
    // Data files will reference the food by it's filename, not display name.
    pub name: String,

    // The macronutrients of this food item.
    #[tabled(inline)]
    pub nutrients: Nutrients,

    // Ways of describing a single serving of this food.
    // For example, [("g", 100.0), ("cups", 0.5)] means that
    // either 100g or 0.5cups equates to one serving.
    #[tabled(display_with = "format_servings")]
    pub servings: Vec<(String, f32)>,
}

impl Food {
    // Compute the nutrients in a serving of this food.
    // Returns an error if the serving unit is not defined for this food.
    pub fn serve(&self, s: &Serving) -> Result<Nutrients> {
        let Some(unit) = &s.unit else {
            return Ok(self.nutrients * s.size);
        };

        let mut matched = self.servings.iter().filter(|(u, _)| u.starts_with(unit));
        let Some(first) = matched.next() else {
            let units = self
                .servings
                .iter()
                .cloned()
                .map(|(unit, _)| unit)
                .collect::<Vec<_>>();
            bail!(
                "Unknown serving unit {unit}, expected one of: {}",
                units.join(", ")
            );
        };

        if let Some(next) = matched.next() {
            bail!(
                "Serving unit '{unit}' ambiguous between '{}' and '{}'",
                first.0,
                next.0
            );
        }

        let (_, size) = first;
        Ok(self.nutrients * (s.size / *size))
    }
}

#[test]
fn test_food_serve() {
    let food = Food {
        name: "".into(),
        nutrients: Nutrients {
            carb: 12.0,
            fat: 3.0,
            protein: 8.0,
            kcal: 120.0,
        },
        servings: vec![("g".into(), 100.0), ("cups".into(), 0.5)],
    };
    let serve = |size, unit: Option<&str>| {
        food.serve(&Serving {
            size,
            unit: unit.map(str::to_string),
        })
    };
    assert_eq!(
        serve(2.0, None).unwrap(),
        Nutrients {
            carb: 24.0,
            fat: 6.0,
            protein: 16.0,
            kcal: 240.0,
        }
    );
    assert_eq!(
        serve(2.0, Some("cups")).unwrap(),
        Nutrients {
            carb: 48.0,
            fat: 12.0,
            protein: 32.0,
            kcal: 480.0,
        }
    );
    assert_eq!(
        serve(2.0, Some("c")).unwrap(),
        Nutrients {
            carb: 48.0,
            fat: 12.0,
            protein: 32.0,
            kcal: 480.0,
        }
    );
    assert_eq!(
        serve(10.0, Some("g")).unwrap(),
        Nutrients {
            carb: 1.2,
            fat: 0.3,
            protein: 0.8,
            kcal: 12.0,
        }
    );
}

impl Data for Food {
    type Key = str;
    const DIR: &str = "food";

    fn path(key: &str) -> std::path::PathBuf {
        [Self::DIR, key]
            .iter()
            .collect::<std::path::PathBuf>()
            .with_extension("txt")
    }

    fn load(r: impl std::io::BufRead) -> Result<Self> {
        let mut food = Food::default();
        for line in r.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            log::trace!("Parsing food line: {line}");
            let Some((k, v)) = line.rsplit_once("=") else {
                bail!("Invalid food line, expected '=': {line}");
            };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "name" => food.name = v.into(),
                "kcal" => food.nutrients.kcal = v.parse()?,
                "carb" => food.nutrients.carb = v.parse()?,
                "fat" => food.nutrients.fat = v.parse()?,
                "protein" => food.nutrients.protein = v.parse()?,
                "serving" => {
                    let idx = v
                        .find(|c: char| c != '.' && !c.is_digit(10))
                        .ok_or_else(|| anyhow!("Invalid serving: {v}"))?;
                    let (size, unit) = v.split_at(idx);
                    let size = size.trim();
                    let unit = unit.trim();
                    let size = size.parse().with_context(|| format!("Parsing '{size}'"))?;
                    food.servings.push((unit.into(), size));
                }
                _ => bail!("Unexpected food key: {k}"),
            }
        }
        Ok(food)
    }

    fn save(&self, w: &mut impl std::io::Write) -> Result<()> {
        log::debug!("Saving {self:?}");
        let n = &self.nutrients;
        writeln!(w, "name = {}", self.name)?;
        writeln!(w, "carb = {}", n.carb)?;
        writeln!(w, "fat = {}", n.fat)?;
        writeln!(w, "protein = {}", n.protein)?;
        writeln!(w, "kcal = {}", n.kcal)?;
        for (unit, size) in &self.servings {
            writeln!(w, "serving = {size} {unit}")?;
        }
        Ok(())
    }
}

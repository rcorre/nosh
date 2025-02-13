use crate::serving::Serving;
use crate::{nutrients::Nutrients, Data};

use anyhow::{bail, Context, Result};
use ini::{Ini, WriteOption};

#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Ingredient {
    pub key: String,
    pub serving: Serving,
    pub food: Food,
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
// FoodSpec defines a food either in terms of nutrients or ingredients.
pub enum FoodSpec {
    Nutrients(Nutrients),
    Ingredients(Vec<Ingredient>),
}

impl Default for FoodSpec {
    fn default() -> Self {
        Self::Nutrients(Nutrients::default())
    }
}

// Food describes a single food item.
#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Food {
    // The display name of the food. This is shown in the UI.
    // Data files will reference the food by it's filename, not display name.
    pub name: String,

    // The macronutrients of this food item.
    pub spec: FoodSpec,

    // Ways of describing a single serving of this food.
    // For example, [("g", 100.0), ("cups", 0.5)] means that
    // either 100g or 0.5cups equates to one serving.
    pub servings: Vec<(String, f32)>,
}

impl Food {
    // Return the nutrients in one serving.
    pub fn nutrients(&self) -> Nutrients {
        match &self.spec {
            FoodSpec::Nutrients(n) => *n,
            FoodSpec::Ingredients(i) => i.iter().map(|x| x.food.nutrients()).sum(),
        }
    }

    // Compute the nutrients in a serving of this food.
    // Returns an error if the serving unit is not defined for this food.
    pub fn serve(&self, s: &Serving) -> Result<Nutrients> {
        let portion = if let Some(unit) = &s.unit {
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
            s.size / *size
        } else {
            s.size
        };

        match &self.spec {
            FoodSpec::Nutrients(n) => Ok(*n * portion),
            FoodSpec::Ingredients(ingredients) => {
                let mut res = Nutrients::default();
                for i in ingredients {
                    res = res + i.food.serve(&(i.serving.clone() * portion))?;
                }
                Ok(res)
            }
        }
    }
}

#[test]
fn test_food_serve() {
    let food = Food {
        name: "".into(),
        spec: FoodSpec::Nutrients(Nutrients {
            carb: 12.0,
            fat: 3.0,
            protein: 8.0,
            kcal: 120.0,
        }),
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

    fn load(
        mut r: impl std::io::BufRead,
        mut load_food: impl FnMut(&str) -> Result<Option<Food>>,
    ) -> Result<Self> {
        let mut food = Food::default();
        let ini = Ini::read_from(&mut r)?;
        log::trace!("Parsing: {ini:?}");

        food.name = if let Some(name) = ini.general_section().get("name") {
            name.into()
        } else {
            bail!("Missing name");
        };

        if let Some(servings) = ini.section(Some("servings")) {
            for (k, v) in servings.iter() {
                log::trace!("Parsing serving: {k} = {v}");
                food.servings.push((k.into(), v.parse()?));
            }
        }

        match (
            ini.section(Some("nutrients")),
            ini.section(Some("ingredients")),
        ) {
            (None, None) => bail!("Must specify one of [nutrients] or [ingredients]"),
            (Some(n), None) => {
                log::trace!("Parsing nutrients");
                let mut nutrients = Nutrients::default();
                nutrients.kcal = n.get("kcal").unwrap_or("0").parse()?;
                nutrients.carb = n.get("carb").unwrap_or("0").parse()?;
                nutrients.fat = n.get("fat").unwrap_or("0").parse()?;
                nutrients.protein = n.get("protein").unwrap_or("0").parse()?;
                food.spec = FoodSpec::Nutrients(nutrients);
            }
            (None, Some(i)) => {
                log::trace!("Parsing ingredients");
                let mut ingredients = vec![];
                for (k, v) in i {
                    ingredients.push(Ingredient {
                        key: k.into(),
                        serving: v.parse()?,
                        food: load_food(k)?.with_context(|| format!("Food not found: {k}"))?,
                    });
                }
                food.spec = FoodSpec::Ingredients(ingredients);
            }
            (Some(_), Some(_)) => bail!("Cannot have both [nutrients] and [ingredients]"),
        }

        Ok(food)
    }

    fn save(&self, w: &mut impl std::io::Write) -> Result<()> {
        log::debug!("Saving {self:?}");
        let mut ini = Ini::new();
        ini.general_section_mut().insert("name", &self.name);
        match &self.spec {
            FoodSpec::Nutrients(n) => {
                let mut sec = ini.with_section(Some("nutrients"));
                sec.add("carb", n.carb.to_string());
                sec.add("fat", n.fat.to_string());
                sec.add("protein", n.protein.to_string());
                sec.add("kcal", n.kcal.to_string());
            }
            FoodSpec::Ingredients(i) => {
                let mut sec = ini.with_section(Some("ingredients"));
                for ingredient in i {
                    sec.add(&ingredient.key, ingredient.serving.to_string());
                }
            }
        }

        let mut servings = ini.with_section(Some("servings"));
        for (unit, size) in &self.servings {
            servings.add(unit, size.to_string());
        }
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

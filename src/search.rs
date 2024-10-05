use anyhow::Result;
use serde::Deserialize;

use crate::Nutrients;

const FDC_URL: &str = "https://api.nal.usda.gov/fdc/v1/foods/search";

// Search for a food on Food Data Central
// https://fdc.nal.usda.gov/api-guide.html
pub struct Search<'a> {
    pub term: &'a str,
    pub page_size: usize,
    pub url: &'a str,
    pub page: usize, // starts at 1
}

impl<'a> Default for Search<'a> {
    fn default() -> Self {
        Self {
            term: "",
            url: FDC_URL,
            page_size: 50,
            page: 1,
        }
    }
}

impl<'a> Search<'a> {
    // Return the next page of results.
    pub fn next_page(&mut self) -> Result<Page> {
        let client = reqwest::blocking::Client::new();

        let req = client
            .get(self.url)
            .header("X-Api-Key", "DEMO_KEY")
            .query(&[("query", self.term)])
            .query(&[("pageNumber", self.page)])
            .query(&[("pageSize", self.page_size)])
            .build()?;

        log::debug!("Sending request: {req:?}");

        let res: Page = client.execute(req)?.error_for_status()?.json()?;
        self.page += 1;
        Ok(res)
    }
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
    serving_size: Option<f32>,                   // 144.0
    serving_size_unit: Option<String>,           // "g"
    household_serving_full_text: Option<String>, // "1 cup"
    food_nutrients: Option<Vec<SearchNutrient>>,
}

impl SearchFood {
    const NUTRIENT_ID_PROTEIN: u32 = 1003; // Protein
    const NUTRIENT_ID_FAT: u32 = 1004; // Total lipid (fat)
    const NUTRIENT_ID_CARB_DIFFERENCE: u32 = 1005; // Carbohydrate, by difference
    const NUTRIENT_ID_ENERGY: u32 = 1008; // Energy
    const NUTRIENT_ID_ENERGY_ATWATER_GENERAL: u32 = 2047; //Energy (Atwater General Factors)
    const NUTRIENT_ID_ENERGY_ATWATER_SPECIFIC: u32 = 2048; //Energy (Atwater Specific Factors)
    const NUTRIENT_ID_CARB_SUMMATION: u32 = 1050; // Carbohydrate, by summation

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
                .or_else(|| self.nutrient(Self::NUTRIENT_ID_CARB_SUMMATION))
                .unwrap_or_default(),
            fat: self.nutrient(Self::NUTRIENT_ID_FAT).unwrap_or_default(),
            protein: self.nutrient(Self::NUTRIENT_ID_PROTEIN).unwrap_or_default(),
            kcal: self
                .nutrient(Self::NUTRIENT_ID_ENERGY_ATWATER_SPECIFIC)
                .or_else(|| self.nutrient(Self::NUTRIENT_ID_ENERGY_ATWATER_GENERAL))
                .or_else(|| self.nutrient(Self::NUTRIENT_ID_ENERGY))
                .unwrap_or_default(),
        }
    }

    // BUG: Nutritional values appear to be always based on 100g.
    // Look for a serving unit of "g" or "GRM", then adjust the nutrients accordingly.
    fn servings(&self) -> Vec<(String, f32)> {
        let mut res = Vec::new();
        if let (Some(unit), Some(size)) = (&self.serving_size_unit, self.serving_size) {
            res.push((unit.clone(), size));
        }
        if let Some(serving) = self.household_serving_full_text.as_ref() {
            let Some((amount, unit)) = serving.split_once(char::is_whitespace) else {
                log::warn!("Failed to parse household serving: {serving}");
                return res;
            };
            let Ok(amount) = amount.parse::<f32>() else {
                log::warn!("Failed to parse household serving amount: {serving}");
                return res;
            };
            res.push((unit.into(), amount));
        }
        // Foundation foods don't seem to have serving portions, but
        // https://fdc.nal.usda.gov/Foundation_Foods_Documentation.html says:
        // All reported values are based on a 100-gram or percent basis of the edible portion
        if res.is_empty() {
            res.push(("g".into(), 100.0));
        }
        res
    }
}

impl From<&SearchFood> for crate::Food {
    fn from(value: &SearchFood) -> Self {
        crate::Food {
            nutrients: value.nutrients(),
            servings: value.servings(),
            name: value.description.clone().unwrap_or_default(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    foods: Vec<SearchFood>,
}

impl Page {
    pub fn iter(&self) -> impl Iterator<Item = crate::Food> + '_ {
        self.foods.iter().map(|x| crate::Food::from(x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Food;
    use httptest::{matchers::*, responders::*, Expectation, Server};
    use pretty_assertions::assert_eq;
    use std::fs;

    fn expect_page(kind: &str, page: usize) -> Expectation {
        Expectation::matching(all_of![
            request::method_path("GET", "/test"),
            request::query(url_decoded(contains(("query", "potato")))),
            request::query(url_decoded(contains(("pageSize", "2")))),
            request::query(url_decoded(contains(("pageNumber", page.to_string()))))
        ])
        .respond_with(status_code(200).body(
            fs::read_to_string(format!("tests/testdata/search/{kind}/page{page}.json")).unwrap(),
        ))
    }

    #[test]
    fn test_search_foundation() {
        let _ = env_logger::try_init();
        let server = Server::run();
        server.expect(expect_page("foundation", 1));
        let url = server.url("/test");

        let mut search = Search {
            term: "potato",
            page_size: 2,
            url: &url.to_string(),
            page: 1,
        };
        let actual = search.next_page().unwrap();
        assert_eq!(
            actual.iter().collect::<Vec<_>>(),
            vec![
                Food {
                    name: "Flour, potato".into(),
                    nutrients: Nutrients {
                        carb: 79.9,
                        fat: 0.951,
                        protein: 8.11,
                        kcal: 353.0
                    },
                    servings: vec![("g".into(), 100.0)],
                },
                Food {
                    name: "Potatoes, gold, without skin, raw".into(),
                    nutrients: Nutrients {
                        carb: 16.0,
                        fat: 0.264,
                        protein: 1.81,
                        kcal: 71.6,
                    },
                    servings: vec![("g".into(), 100.0)],
                },
            ]
        );
    }

    #[test]
    fn test_search_fndds() {
        let _ = env_logger::try_init();
        let server = Server::run();
        server.expect(expect_page("fndds", 1));
        let url = server.url("/test");

        let mut search = Search {
            term: "potato",
            page_size: 2,
            url: &url.to_string(),
            page: 1,
        };
        let actual = search.next_page().unwrap();
        assert_eq!(
            actual.iter().collect::<Vec<_>>(),
            vec![
                Food {
                    name: "Potato patty".into(),
                    nutrients: Nutrients {
                        carb: 13.5,
                        fat: 11.3,
                        protein: 3.89,
                        kcal: 171.0,
                    },
                    servings: vec![("g".into(), 100.0)],
                },
                Food {
                    name: "Potato pancake".into(),
                    nutrients: Nutrients {
                        carb: 20.6,
                        fat: 10.8,
                        protein: 4.47,
                        kcal: 196.0,
                    },
                    servings: vec![("g".into(), 100.0)],
                },
            ]
        );
    }

    #[test]
    fn test_search_sr_legacy() {
        let _ = env_logger::try_init();
        let server = Server::run();
        server.expect(expect_page("sr_legacy", 1));
        let url = server.url("/test");

        let mut search = Search {
            term: "potato",
            page_size: 2,
            url: &url.to_string(),
            page: 1,
        };
        let actual = search.next_page().unwrap();
        assert_eq!(
            actual.iter().collect::<Vec<_>>(),
            vec![
                Food {
                    name: "Bread, potato".into(),
                    nutrients: Nutrients {
                        carb: 47.1,
                        fat: 3.13,
                        protein: 12.5,
                        kcal: 266.0,
                    },
                    servings: vec![("g".into(), 100.0)],
                },
                Food {
                    name: "Potato flour".into(),
                    nutrients: Nutrients {
                        carb: 83.1,
                        fat: 0.34,
                        protein: 6.9,
                        kcal: 357.0,
                    },
                    servings: vec![("g".into(), 100.0)],
                },
            ]
        );
    }

    #[test]
    fn test_search_branded() {
        let _ = env_logger::try_init();
        let server = Server::run();
        server.expect(expect_page("branded", 1));
        let url = server.url("/test");

        let mut search = Search {
            term: "potato",
            page_size: 2,
            url: &url.to_string(),
            page: 1,
        };
        let actual = search.next_page().unwrap();
        assert_eq!(
            actual.iter().collect::<Vec<_>>(),
            vec![
                Food {
                    name: "KASIA'S, POTATO PANCAKES, POTATO, POTATO".into(),
                    nutrients: Nutrients {
                        carb: 26.3,
                        fat: 7.02,
                        protein: 3.51,
                        kcal: 158.0,
                    },
                    servings: vec![("GRM".into(), 57.0), ("PANCAKE".into(), 1.0)],
                },
                Food {
                    name: "GNOCCHI WITH POTATO, POTATO".into(),
                    nutrients: Nutrients {
                        carb: 29.3,
                        fat: 0.36,
                        protein: 3.57,
                        kcal: 136.0,
                    },
                    servings: vec![("g".into(), 140.0), ("cup".into(), 1.0)],
                },
            ]
        );
    }

    #[test]
    fn test_search_paged() {
        let _ = env_logger::try_init();
        let server = Server::run();
        server.expect(expect_page("foundation", 1));
        server.expect(expect_page("foundation", 2));
        server.expect(expect_page("foundation", 3));
        let url = server.url("/test");

        let mut search = Search {
            term: "potato",
            page_size: 2,
            url: &url.to_string(),
            page: 1,
        };

        let actual = search.next_page().unwrap();
        assert_eq!(
            actual.iter().collect::<Vec<_>>(),
            vec![
                Food {
                    name: "Flour, potato".into(),
                    nutrients: Nutrients {
                        carb: 79.9,
                        fat: 0.951,
                        protein: 8.11,
                        kcal: 353.0
                    },
                    servings: vec![("g".into(), 100.0)],
                },
                Food {
                    name: "Potatoes, gold, without skin, raw".into(),
                    nutrients: Nutrients {
                        carb: 16.0,
                        fat: 0.264,
                        protein: 1.81,
                        kcal: 71.6,
                    },
                    servings: vec![("g".into(), 100.0)],
                },
            ]
        );

        let actual = search.next_page().unwrap();
        assert_eq!(
            actual.iter().collect::<Vec<_>>(),
            vec![
                Food {
                    name: "Potatoes, red, without skin, raw".into(),
                    nutrients: Nutrients {
                        carb: 16.3,
                        fat: 0.248,
                        protein: 2.06,
                        kcal: 73.4,
                    },
                    servings: vec![("g".into(), 100.0)],
                },
                Food {
                    name: "Potatoes, russet, without skin, raw".into(),
                    nutrients: Nutrients {
                        carb: 17.8,
                        fat: 0.36,
                        protein: 2.27,
                        kcal: 81.0,
                    },
                    servings: vec![("g".into(), 100.0)],
                },
            ]
        );

        let actual = search.next_page().unwrap();
        assert_eq!(
            actual.iter().collect::<Vec<_>>(),
            vec![Food {
                name: "Sweet potatoes, orange flesh, without skin, raw".into(),
                nutrients: Nutrients {
                    carb: 17.3,
                    fat: 0.375,
                    protein: 1.58,
                    kcal: 77.4,
                },
                servings: vec![("g".into(), 100.0)],
            },]
        );
    }
}

use anyhow::Result;
use serde::Deserialize;

use crate::Nutrients;

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
    const NUTRIENT_ID_ENERGY_KCAL: u32 = 1008; // Energy
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
                .or(self.nutrient(Self::NUTRIENT_ID_CARB_SUMMATION))
                .unwrap_or_default(),
            fat: self.nutrient(Self::NUTRIENT_ID_FAT).unwrap_or_default(),
            protein: self.nutrient(Self::NUTRIENT_ID_PROTEIN).unwrap_or_default(),
            kcal: self
                .nutrient(Self::NUTRIENT_ID_ENERGY_KCAL)
                .unwrap_or_default(),
        }
    }

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
struct SearchResponse {
    foods: Vec<SearchFood>,
}

// Search for a food on Food Data Central
// https://fdc.nal.usda.gov/api-guide.html
// Leave URL as None to use the default search API.
pub fn search_food(term: &str, url: Option<&str>) -> Result<Vec<crate::Food>> {
    let client = reqwest::blocking::Client::new();
    let url = url.unwrap_or("https://api.nal.usda.gov/fdc/v1/foods/search");

    let req = client
        .get(url)
        .header("X-Api-Key", "DEMO_KEY")
        .query(&[("query", &term)])
        .build()?;

    log::debug!("Sending request: {req:?}");

    let res: SearchResponse = client.execute(req)?.error_for_status()?.json()?;
    Ok(res.foods.iter().map(|x| crate::Food::from(x)).collect())
}

// mod tests {
//     use super::*;
//     use httptest::{matchers::*, responders::*, Expectation, Server};

//     #[test]
//     fn test_search() {
//         let server = Server::run();
//         server.expect(
//             Expectation::matching(request::method_path("GET", "/test")).respond_with(
//                 status_code(200).body(
//                     r#"{"foods":[{
//         "description": "Potato, NFS",
//         "servingSizeUnit": "g",
//         "servingSize": 144.0,
//         "householdServingFullText": "1 cup",
//         "foodNutrients": [
//             {
//               "nutrientId": 1003,
//               "nutrientName": "Protein",
//               "unitName": "G",
//               "value": 1.87
//             },
//             {
//               "nutrientId": 1004,
//               "nutrientName": "Total lipid (fat)",
//               "unitName": "G",
//               "value": 4.25
//             },
//             {
//               "nutrientId": 1005,
//               "nutrientName": "Carbohydrate, by difference",
//               "unitName": "G",
//               "value": 20.4
//             },
//             {
//               "nutrientId": 1008,
//               "nutrientName": "Energy",
//               "unitName": "KCAL",
//               "value": 126
//             }
//         ]
//     }]}"#,
//                 ),
//             ),
//         );
//         let url = server.url("/test");
//         let food = &Food {
//             name: "Potato, NFS".into(),
//             nutrients: Nutrients {
//                 carb: 20.4,
//                 fat: 4.25,
//                 protein: 1.87,
//                 kcal: 126.0,
//             },
//             servings: [("g".to_string(), 144.0), ("cup".to_string(), 1.0)].into(),
//         };

//         cli.search(&url.to_string())
//             .args(["food", "search", "potato"])
//             .write_stdin("0") // select result 0
//             .assert()
//             .success()
//             .stdout(matches_food(&food));

//         // The food should have been added.
//         cli.cmd()
//             .args(["food", "show", "potato"])
//             .assert()
//             .success()
//             .stdout(matches_food(&food));
//     }
// }

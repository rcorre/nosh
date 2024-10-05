use std::{io::Write, ops::Deref, os::unix::fs::OpenOptionsExt};

use assert_cmd::Command;
use httptest::http::request;
use nosh::{Food, Nutrients, APP_NAME};
use predicates::{prelude::*, BoxPredicate};

struct CLI {
    data_dir: tempfile::TempDir,
}

fn oats() -> Food {
    Food {
        name: "Oats".into(),
        nutrients: Nutrients {
            carb: 68.7,
            fat: 5.89,
            protein: 13.5,
            kcal: 382.0,
        },
        servings: [("g".into(), 100.0), ("cups".into(), 0.5)].into(),
    }
}

fn banana() -> Food {
    Food {
        name: "Banana".into(),
        nutrients: Nutrients {
            carb: 23.0,
            fat: 0.20,
            protein: 0.74,
            kcal: 98.0,
        },
        servings: [("g".into(), 100.0)].into(),
    }
}

impl CLI {
    fn new() -> Self {
        let cli = Self {
            data_dir: tempfile::tempdir().unwrap(),
        };

        // pre-load some data
        cli.add_food("oats", &oats());
        cli.add_food("banana", &banana());

        cli
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
        cmd.env("XDG_DATA_HOME", self.data_dir.path());
        cmd
    }

    fn editor(&self, content: &str) -> Command {
        let editor = format!("#!/bin/sh\necho -e {content:?} > $1");
        let path = self.data_dir.path().join("editor");
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o770)
            .open(&path)
            .unwrap()
            .write_all(editor.as_bytes())
            .unwrap();
        log::debug!("Test wrote fake editor to {path:?}:\n{editor:?}");
        let mut cmd = self.cmd();
        cmd.env("EDITOR", path);
        cmd
    }

    fn search(&self, url: &str) -> Command {
        let mut cmd = self.cmd();
        cmd.env("NOSH_SEARCH_URL", url);
        cmd
    }

    fn add_food(&self, key: &str, food: &Food) {
        let path = self.data_dir.path().join(APP_NAME);
        log::info!("Test staging food to {path:?}: {food:?}");
        nosh::Data::new(&path)
            .unwrap()
            .write_food(key, food)
            .unwrap()
    }
}

fn matches(pattern: &str) -> predicates::str::RegexPredicate {
    predicates::str::is_match(pattern).unwrap()
}

fn matches_food(food: &Food) -> predicates::str::RegexPredicate {
    let n = &food.nutrients;
    matches(&format!(
        "{}.*{:.1}.*{:.1}.*{:.1}.*{:.0}",
        food.name, n.carb, n.fat, n.protein, n.kcal
    ))
}

fn matches_food_details(food: &Food) -> BoxPredicate<str> {
    let n = &food.nutrients;
    let mut pred = BoxPredicate::<str>::new(
        matches(&format!("fat.*{}", n.fat))
            .and(matches(&format!("protein.*{}", n.protein)))
            .and(matches(&format!("kcal.*{}", n.kcal))),
    );
    for (unit, amount) in &food.servings {
        pred = BoxPredicate::<str>::new(pred.and(matches(&format!("{unit}.*{amount}"))));
    }
    pred
}

fn matches_serving(serving: f32, food: &Food) -> predicates::str::RegexPredicate {
    let n = food.nutrients * serving;
    matches(&format!(
        "{}.*{:.1}.*{:.1}.*{:.1}.*{:.1}.*{:.0}",
        food.name, serving, n.carb, n.fat, n.protein, n.kcal
    ))
}

fn matches_total(servings: &[(f32, &Food)]) -> predicates::str::RegexPredicate {
    let n = servings
        .iter()
        .map(|(serving, food)| food.nutrients * *serving)
        .reduce(|acc, e| acc + e)
        .unwrap();
    matches(&format!(
        "Total.*{:.1}.*{:.1}.*{:.1}.*{:.0}",
        n.carb, n.fat, n.protein, n.kcal
    ))
}

#[test]
fn test_show_food_missing() {
    let cli = CLI::new();

    cli.cmd()
        .args(["food", "show", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error: No food with key \"nope\""));
}

#[test]
fn test_food_show() {
    let cli = CLI::new();

    cli.cmd()
        .args(["food", "show", "oats"])
        .assert()
        .success()
        .stdout(matches_food_details(&oats()));
}

#[test]
fn test_food_ls() {
    let cli = CLI::new();

    cli.cmd()
        .args(["food", "ls"])
        .assert()
        .success()
        .stdout(matches_food(&oats()))
        .stdout(matches_food(&banana()));
}

#[test]
fn test_food_ls_pattern() {
    let cli = CLI::new();

    cli.cmd()
        .args(["food", "ls", "oat"])
        .assert()
        .success()
        .stdout(matches_food(&oats()));
}

#[test]
fn test_food_ls_pattern_nomatch() {
    let cli = CLI::new();

    cli.cmd().args(["food", "ls", "nope"]).assert().success();
}

#[test]
fn test_food_rm() {
    let cli = CLI::new();

    cli.cmd().args(["food", "rm", "oats"]).assert().success();
    cli.cmd()
        .args(["food", "ls"])
        .assert()
        .success()
        .stdout(matches_food(&banana()));
}

#[test]
fn test_food_rm_not_exist() {
    let cli = CLI::new();
    cli.cmd().args(["food", "rm", "nope"]).assert().failure();
}

#[test]
fn test_food_edit_new() {
    let cli = CLI::new();

    cli.editor(
        r#"
name = "Lemon"
[nutrients]
carb = 4.0
fat = 0
protein = 0
kcal = 16
[servings]
g = 100.0
        "#,
    )
    .args(["food", "edit", "lemon"])
    .assert()
    .success();

    cli.cmd()
        .args(["food", "show", "lemon"])
        .assert()
        .success()
        .stdout(matches("carb.*4.0"))
        .stdout(matches("fat.*0.0"))
        .stdout(matches("protein.*0.0"))
        .stdout(matches("kcal.*16"))
        .stdout(matches("g.*100"));
}

#[test]
fn test_food_edit_existing() {
    let cli = CLI::new();

    cli.editor(
        r#"
name = "Oats"
[nutrients]
carb = 40.0
fat = 7.10
protein = 14.0
kcal = 240
[servings]
g = 100.0
cups = 0.5
        "#,
    )
    .args(["food", "edit", "oats"])
    .assert()
    .success();

    cli.cmd()
        .args(["food", "show", "oats"])
        .assert()
        .success()
        .stdout(matches("carb.*40.0"))
        .stdout(matches("fat.*7.1"))
        .stdout(matches("protein.*14.0"))
        .stdout(matches("kcal.*240"))
        .stdout(matches("g.*100"))
        .stdout(matches("cups.*0.5"));
}

#[test]
fn test_eat_missing() {
    let cli = CLI::new();

    cli.cmd()
        .args(["eat", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error: No food with key \"nope\""));
}

#[test]
fn test_eat() {
    let cli = CLI::new();

    // Add one serving
    cli.cmd().args(["eat", "oats"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches_serving(1.0, &oats()))
        .stdout(matches_total(&[(1.0, &oats())]));

    // Add 2.5 servings
    cli.cmd().args(["eat", "oats", "2.5"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches_serving(3.5, &oats()))
        .stdout(matches_total(&[(3.5, &oats())]));

    // Add one serving of banana
    cli.cmd().args(["eat", "banana"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches_serving(3.5, &oats()))
        .stdout(matches_serving(1.0, &banana()))
        .stdout(matches_total(&[(3.5, &oats()), (1.0, &banana())]));

    // Add one cup (two servings) of oats
    cli.cmd().args(["eat", "oats", "1cups"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches_serving(5.5, &oats()))
        .stdout(matches_serving(1.0, &banana()))
        .stdout(matches_total(&[(5.5, &oats()), (1.0, &banana())]));

    // Add 0.25 cup (half serving) of oats
    cli.cmd().args(["eat", "oats", "0.25c"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches_serving(6.0, &oats()))
        .stdout(matches_serving(1.0, &banana()))
        .stdout(matches_total(&[(6.0, &oats()), (1.0, &banana())]));
}

#[test]
fn test_food_search() {
    use httptest::{matchers::*, responders::*, Expectation, Server};

    let cli = CLI::new();
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/test")).respond_with(
            status_code(200).body(
                r#"{"foods":[{
        "description": "Potato, NFS",
        "servingSizeUnit": "g",
        "servingSize": 144.0,
        "householdServingFullText": "1 cup",
        "foodNutrients": [
            {
              "nutrientId": 1003,
              "nutrientName": "Protein",
              "unitName": "G",
              "value": 1.87
            },
            {
              "nutrientId": 1004,
              "nutrientName": "Total lipid (fat)",
              "unitName": "G",
              "value": 4.25
            },
            {
              "nutrientId": 1005,
              "nutrientName": "Carbohydrate, by difference",
              "unitName": "G",
              "value": 20.4
            },
            {
              "nutrientId": 1008,
              "nutrientName": "Energy",
              "unitName": "KCAL",
              "value": 126
            }
        ]
    }]}"#,
            ),
        ),
    );
    let url = server.url("/test");
    let food = &Food {
        name: "Potato, NFS".into(),
        nutrients: Nutrients {
            carb: 20.4,
            fat: 4.25,
            protein: 1.87,
            kcal: 126.0,
        },
        servings: [("g".to_string(), 144.0), ("cup".to_string(), 1.0)].into(),
    };

    cli.search(&url.to_string())
        .args(["food", "search", "potato"])
        .write_stdin("0") // select result 0
        .assert()
        .success()
        .stdout(matches_food(&food));

    // The food should have been added.
    cli.cmd()
        .args(["food", "show", "potato"])
        .assert()
        .success()
        .stdout(matches_food_details(&food));
}

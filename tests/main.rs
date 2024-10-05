use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

use assert_cmd::Command;
use nosh::{Food, Nutrients};
use predicates::prelude::*;

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

fn cp(src: impl AsRef<Path>, dst: impl AsRef<Path>) {
    fs::create_dir_all(&dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        if ty.is_dir() {
            cp(entry.path(), dst.as_ref().join(entry.file_name()));
        } else {
            let dst = dst.as_ref().join(entry.file_name());
            fs::copy(entry.path(), &dst).unwrap();
            log::trace!("Copied {entry:?} to {dst:?}");
        }
    }
}

impl CLI {
    fn new() -> Self {
        let _ = env_logger::try_init();
        let cli = Self {
            data_dir: tempfile::tempdir().unwrap(),
        };
        let dir = cli.data_dir.path().join("nosh");
        fs::create_dir_all(&dir).unwrap();
        cp("tests/testdata/good", &dir);
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
}

fn matches(pattern: &str) -> predicates::str::RegexPredicate {
    predicates::str::is_match(pattern).unwrap()
}

fn matches_food(food: &Food) -> predicates::str::RegexPredicate {
    let n = &food.nutrients;
    matches(&format!(
        "{}.*{}.*{:.1}.*{:.1}.*{:.0}",
        food.name, n.carb, n.fat, n.protein, n.kcal
    ))
}

fn matches_serving(serving: f32, food: &Food) -> predicates::str::RegexPredicate {
    let n = food.nutrients * serving;
    matches(&format!(
        "{}.*{}.*{:.1}.*{:.1}.*{:.1}.*{:.0}",
        food.name, serving, n.carb, n.fat, n.protein, n.kcal
    ))
}

fn matches_serving_str(serving: &str, food: &Food) -> predicates::str::RegexPredicate {
    let n = food.serve(&serving.parse().unwrap()).unwrap();
    matches(&format!(
        "{}.*{}.*{:.1}.*{:.1}.*{:.1}.*{:.0}",
        food.name, serving, n.carb, n.fat, n.protein, n.kcal
    ))
}

fn matches_total(n: Nutrients) -> predicates::str::RegexPredicate {
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
        .stdout(matches_food(&oats()));
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
name = Lemon
carb = 4.0
kcal = 16
"#,
    )
    .args(["food", "edit", "lemon"])
    .assert()
    .success();

    cli.cmd()
        .args(["food", "show", "lemon"])
        .assert()
        .success()
        .stdout(matches_food(&Food {
            name: "Lemon".into(),
            nutrients: Nutrients {
                carb: 4.0,
                fat: 0.0,
                protein: 0.0,
                kcal: 16.0,
            },
            servings: vec![],
        }));
}

#[test]
fn test_food_edit_existing() {
    let cli = CLI::new();

    cli.editor(
        r#"
name = Oats2
carb = 30.0
fat = 8.10
protein = 24.0
kcal = 480
serving = 200.0 g
serving = 2.5cups"#,
    )
    .args(["food", "edit", "oats"])
    .assert()
    .success();

    cli.cmd()
        .args(["food", "show", "oats"])
        .assert()
        .success()
        .stdout(matches_food(&Food {
            name: "Oats2".into(),
            nutrients: Nutrients {
                carb: 30.0,
                fat: 8.1,
                protein: 24.0,
                kcal: 480.0,
            },
            servings: vec![("g".into(), 200.0), ("cups".into(), 2.5)],
        }));
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
        .stdout(matches_total(oats().nutrients));

    // Add 2.5 servings
    cli.cmd().args(["eat", "oats", "2.5"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches_serving(1.0, &oats()))
        .stdout(matches_serving(2.5, &oats()))
        .stdout(matches_total(oats().nutrients * 3.5));

    // Add one serving of banana
    cli.cmd().args(["eat", "banana"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches_serving(1.0, &oats()))
        .stdout(matches_serving(2.5, &oats()))
        .stdout(matches_serving(1.0, &banana()))
        .stdout(matches_total(Nutrients {
            carb: 263.5,
            fat: 20.8,
            protein: 48.0,
            kcal: 1435.0,
        }));

    // Add one cup (two servings) of oats
    cli.cmd().args(["eat", "oats", "1cups"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches_serving(1.0, &oats()))
        .stdout(matches_serving(2.5, &oats()))
        .stdout(matches_serving(1.0, &banana()))
        .stdout(matches_serving_str("1 cups", &oats()))
        .stdout(matches_total(
            oats().nutrients + oats().nutrients * 2.5 + banana().nutrients + oats().nutrients * 2.0,
        ));

    // Add 0.25 cup (half serving) of oats
    cli.cmd().args(["eat", "oats", "0.25c"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches_serving(1.0, &oats()))
        .stdout(matches_serving(2.5, &oats()))
        .stdout(matches_serving(1.0, &banana()))
        .stdout(matches_serving_str("1 cups", &oats()))
        .stdout(matches_serving_str("0.25 c", &oats()))
        .stdout(matches_total(
            oats().nutrients
                + oats().nutrients * 2.5
                + banana().nutrients
                + oats().nutrients * 2.0
                + oats().nutrients * 0.5,
        ));
}

#[test]
fn test_food_search() {
    use httptest::{matchers::*, responders::*, Expectation, Server};

    let cli = CLI::new();
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/test")).respond_with(
            status_code(200).body(fs::read_to_string("tests/testdata/search/page0.json").unwrap()),
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
        .stdout(matches_food(&food));
}

#[test]
fn test_journal_show() {
    let cli = CLI::new();

    cli.cmd()
        .args(["recipe", "show", "banana_oatmeal"])
        .assert()
        .success()
        .stdout(matches_serving_str("0.5 c", &oats()))
        .stdout(matches_serving_str("150 g", &banana()));
}

#[test]
fn test_journal_edit() {
    let cli = CLI::new();

    cli.editor(
        r#"
oats = 1.5c
banana
"#,
    )
    .args(["recipe", "edit", "banana_oatmeal"])
    .assert()
    .success();

    cli.cmd()
        .args(["recipe", "show", "banana_oatmeal"])
        .assert()
        .success()
        .stdout(matches_serving_str("1.5 c", &oats()))
        .stdout(matches_serving(1.0, &banana()));
}

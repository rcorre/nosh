use std::{io::Write, os::unix::fs::OpenOptionsExt};

use assert_cmd::Command;
use nosh::{Food, Nutrients, APP_NAME};
use predicates::prelude::*;

struct CLI {
    data_dir: tempfile::TempDir,
}

impl CLI {
    fn new() -> Self {
        Self {
            data_dir: tempfile::tempdir().unwrap(),
        }
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

    fn add_food(&self, key: &str, food: &Food) {
        let path = self.data_dir.path().join(APP_NAME);
        log::info!("Test staging food to {path:?}: {food:?}");
        nosh::Data::new(&path).write_food(key, food).unwrap()
    }
}

fn matches(pattern: &str) -> predicates::str::RegexPredicate {
    predicates::str::is_match(pattern).unwrap()
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
fn test_edit_food() {
    let cli = CLI::new();

    // Edit a new food
    cli.editor(
        r#"
name = "Oats"
[nutrients]
carb = 68.7
fat = 5.89
protein = 13.5
kcal = 382
[servings]
g = 100.0
        "#,
    )
    .args(["food", "edit", "oats"])
    .assert()
    .success();

    cli.cmd()
        .args(["food", "show", "oats"])
        .assert()
        .success()
        .stdout(matches("carb.*68.7"))
        .stdout(matches("fat.*5.89"))
        .stdout(matches("protein.*13.5"))
        .stdout(matches("kcal.*382"))
        .stdout(matches("g.*100"));

    // Edit the existing food
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
fn test_eat_food_missing() {
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

    cli.add_food(
        "oats",
        &Food {
            name: "Oats".into(),
            nutrients: Nutrients {
                carb: 68.7,
                fat: 5.89,
                protein: 13.5,
                kcal: 382.0,
            },
            servings: [("g".into(), 100.0)].into(),
        },
    );

    cli.add_food(
        "banana",
        &Food {
            name: "Banana".into(),
            nutrients: Nutrients {
                carb: 23.0,
                fat: 0.20,
                protein: 0.74,
                kcal: 98.0,
            },
            servings: [("g".into(), 100.0)].into(),
        },
    );

    // Add one serving
    cli.cmd().args(["eat", "oats"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches("Oats.*1.*68.7.*5.9.*13.5.*382"))
        .stdout(matches("Total.*1.*68.7.*5.9.*13.5.*382"));

    // Add 2.5 servings
    cli.cmd().args(["eat", "oats", "2.5"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches("Oats.*3.5.*240.4.*20.6.*47.2.*1337"))
        .stdout(matches("Total.*240.4.*20.6.*47.2.*1337"));

    // Add one serving of banana
    cli.cmd().args(["eat", "banana"]).assert().success();
    cli.cmd()
        .args(["journal", "show"])
        .assert()
        .success()
        .stdout(matches("Oats.*3.5.*240.4.*20.6.*47.2.*1337"))
        .stdout(matches("Banana.*1.*23.0.*0.2.*0.7.*98"))
        .stdout(matches("Total.*263.4.*20.8.*48.0.*1435"));
}

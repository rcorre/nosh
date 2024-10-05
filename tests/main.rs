use std::{io::Write, os::unix::fs::OpenOptionsExt};

use assert_cmd::Command;
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
        let mut cmd = Command::cargo_bin("nom").unwrap();
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
        .stderr(predicate::str::starts_with(
            "Error: No food with key \"nope\"",
        ));
}

#[test]
fn test_edit_food() {
    let cli = CLI::new();

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

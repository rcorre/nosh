use std::process::Command;
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

use insta::internals::SettingsBindDropGuard;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

struct CLI {
    data_dir: tempfile::TempDir,
    _settings: SettingsBindDropGuard,
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

        let mut settings = insta::Settings::clone_current();

        // Remove ansi color codes
        settings.add_filter("\x1b\\[\\d+m", "");

        let cli = Self {
            data_dir: tempfile::tempdir().unwrap(),
            _settings: settings.bind_to_scope(),
        };
        let dir = cli.data_dir.path().join("nosh");
        fs::create_dir_all(&dir).unwrap();
        cp("tests/testdata/good", &dir);
        cli
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(get_cargo_bin(env!("CARGO_PKG_NAME")));
        cmd.env("XDG_DATA_HOME", self.data_dir.path());
        cmd
    }

    fn run(&self, args: &[&str]) {
        assert!(self
            .cmd()
            .args(args)
            .spawn()
            .unwrap()
            .wait()
            .unwrap()
            .success())
    }

    fn edit(&self, kind: &str, key: &str, content: &str) {
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
        cmd.args([kind, "edit", key]);
        assert!(cmd.spawn().unwrap().wait().unwrap().success());
    }

    fn search(&self, url: &str) -> Command {
        let mut cmd = self.cmd();
        cmd.env("NOSH_SEARCH_URL", url);
        cmd
    }
}

#[test]
fn test_food_show_missing() {
    let cli = CLI::new();
    assert_cmd_snapshot!(cli.cmd().args(["food", "show", "nope"]));
}

#[test]
fn test_food_show() {
    let cli = CLI::new();
    assert_cmd_snapshot!(cli.cmd().args(["food", "show", "oats"]));
}

#[test]
fn test_food_ls() {
    let cli = CLI::new();
    assert_cmd_snapshot!(cli.cmd().args(["food", "ls"]));
}

#[test]
fn test_food_ls_pattern() {
    let cli = CLI::new();
    assert_cmd_snapshot!(cli.cmd().args(["food", "ls", "oat"]));
}

#[test]
fn test_food_ls_pattern_nomatch() {
    let cli = CLI::new();
    assert_cmd_snapshot!(cli.cmd().args(["food", "ls", "nope"]));
}

#[test]
fn test_food_rm() {
    let cli = CLI::new();

    assert_cmd_snapshot!(cli.cmd().args(["food", "rm", "oats"]));
    assert_cmd_snapshot!(cli.cmd().args(["food", "ls"]));
}

#[test]
fn test_food_rm_not_exist() {
    let cli = CLI::new();
    assert_cmd_snapshot!(cli.cmd().args(["food", "rm", "nope"]));
}

#[test]
fn test_food_edit_new() {
    let cli = CLI::new();

    cli.edit(
        "food",
        "lemon",
        r#"
name = Lemon
carb = 4.0
kcal = 16
"#,
    );

    assert_cmd_snapshot!(cli.cmd().args(["food", "show", "lemon"]));
}

#[test]
fn test_food_edit_existing() {
    let cli = CLI::new();

    cli.edit(
        "food",
        "oats",
        r#"
name = Oats2
carb = 30.0
fat = 8.10
protein = 24.0
kcal = 480
serving = 200.0 g
serving = 2.5cups"#,
    );

    assert_cmd_snapshot!(cli.cmd().args(["food", "show", "oats"]));
}

#[test]
fn test_eat_missing() {
    let cli = CLI::new();
    assert_cmd_snapshot!(cli.cmd().args(["eat", "nope"]));
}

#[test]
fn test_eat() {
    let cli = CLI::new();

    cli.run(&["eat", "oats"]);
    cli.run(&["eat", "oats", "2.5"]);
    cli.run(&["eat", "banana"]);
    cli.run(&["eat", "oats", "1cups"]);
    cli.run(&["eat", "oats", "0.25c"]);

    // Add 0.25 cup (half serving) of oats
    assert_cmd_snapshot!(cli.cmd().args(["journal", "show"]));
}

#[test]

fn test_food_search() {
    use httptest::{matchers::*, responders::*, Expectation, Server};

    let cli = CLI::new();
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/test")).respond_with(
            status_code(200)
                .body(fs::read_to_string("tests/testdata/search/foundation/page1.json").unwrap()),
        ),
    );
    let url = server.url("/test");

    assert_cmd_snapshot!(
        cli.search(&url.to_string())
            .args(["food", "search", "potato"])
            .pass_stdin("1") // select result 1
    );

    // The food should have been added.
    assert_cmd_snapshot!(cli.cmd().args(["food", "show", "potato"]));
}

#[test]
fn test_journal_show() {
    let cli = CLI::new();
    assert_cmd_snapshot!(cli.cmd().args(["journal", "show", "2024-07-01"]));
}

#[test]
fn test_journal_edit() {
    let cli = CLI::new();

    cli.edit(
        "journal",
        "2024-07-01",
        r#"
oats = 1.5c
banana
"#,
    );
    assert_cmd_snapshot!(cli.cmd().args(["journal", "show", "2024-07-01"]));
}

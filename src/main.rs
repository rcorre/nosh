use anyhow::Result;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

fn parse(path: &Path) -> Result<()> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        log::trace!("Read {line:?}");
    }
    Ok(())
}

fn main() -> Result<()> {
    parse(Path::new("./food.nom"))
}

use anyhow::Result;

// A type that can be stored in the Database.
pub trait Data: Sized {
    fn load(r: impl std::io::BufRead) -> Result<Self>;
    fn save(&self, w: &mut impl std::io::Write) -> Result<()>;
}

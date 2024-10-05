use anyhow::Result;

// A type that can be stored in the Database.
pub trait Data: Sized {
    // The type used to uniquely identify items of this type.
    // Typically str, but Journals use a date.
    type Key: ?Sized;

    // The name of the root directory where data of this type is stored;
    const DIR: &str;

    // Return a relative path to the file where the given key should be stored.
    // This should include DIR as the first component, and should include an extension.
    fn path(key: &Self::Key) -> std::path::PathBuf;

    // Load an item from a reader.
    fn load(r: impl std::io::BufRead) -> Result<Self>;

    // Save an item to a reader.
    fn save(&self, w: &mut impl std::io::Write) -> Result<()>;
}

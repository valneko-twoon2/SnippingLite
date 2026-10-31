//! Saving and loading `.rec` files in a folder.
use crate::format::{decode, encode, Record};
use std::io;
use std::path::{Path, PathBuf};

pub fn save(dir: &Path, name: &str, records: &[Record]) -> io::Result<PathBuf> {
    if name.is_empty() || name.len() > 64 || name.contains('/') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad name"));
    }
    let path = dir.join(format!("{name}.rec"));
    std::fs::write(&path, encode(records))?;
    Ok(path)
}

pub fn load(dir: &Path, name: &str) -> Result<Vec<Record>, String> {
    let bytes = std::fs::read(dir.join(format!("{name}.rec"))).map_err(|e| e.to_string())?;
    decode(&bytes).map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_file_loads_back() {
        let dir = std::env::temp_dir().join(format!("rs-records-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let records = vec![Record::Text("note".into())];
        save(&dir, "first", &records).unwrap();
        assert_eq!(load(&dir, "first").unwrap(), records);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

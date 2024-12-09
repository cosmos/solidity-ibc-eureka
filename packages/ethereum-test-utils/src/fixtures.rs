use std::path::PathBuf;

pub fn load<T>(name: &str) -> T
where
    T: serde::de::DeserializeOwned,
{
    // Construct the path relative to the Cargo manifest directory
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/fixtures");
    path.push(format!("{}.json", name));

    // Open the file and deserialize its contents
    let file = std::fs::File::open(path).unwrap();
    serde_json::from_reader(file).unwrap()
}

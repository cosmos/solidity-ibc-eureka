use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct StepFixture {
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Step {
    pub name: String,
    pub data: Value,
}

impl StepFixture {
    pub fn get_data_at_step<T>(&self, step: usize) -> T
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_value(self.steps[step].data.clone()).unwrap()
    }
}

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

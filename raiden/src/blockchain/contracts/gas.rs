use std::collections::HashMap;

use super::consts::GAS;

pub struct GasMetadata {
	data: HashMap<String, u64>,
}

impl GasMetadata {
	pub fn new() -> Self {
		let gas_data: serde_json::Value = serde_json::from_str(GAS).unwrap();

		let mut data = HashMap::new();
		for (name, value) in gas_data.as_object().unwrap() {
			data.insert(name.clone(), value.as_u64().unwrap());
		}

		Self { data }
	}

	pub fn get(&self, name: &'static str) -> u64 {
		*self.data.get(name).unwrap()
	}
}

use std::collections::HashMap;

use super::consts::GAS;

/// Provides gas metadata about Raiden contract calls.
pub struct GasMetadata {
	data: HashMap<String, u64>,
}

impl GasMetadata {
	/// Create and initializes new instance of `GasMetadata`.
	pub fn new() -> Self {
		let gas_data: serde_json::Value = serde_json::from_str(GAS).unwrap();

		let mut data = HashMap::new();
		for (name, value) in gas_data.as_object().unwrap() {
			data.insert(name.clone(), value.as_u64().unwrap());
		}

		Self { data }
	}

	/// Retrives the gas information for a specific contract call.
	pub fn get(&self, name: &'static str) -> u64 {
		*self.data.get(name).unwrap()
	}
}

use serde::{
	Deserialize,
	Serialize,
};
use serde_json::json;

use crate::{
	deserializers::{
		h256_from_str,
		signature_from_str,
		u256_from_str,
		u256_from_u64,
		u64_from_str,
	},
	types::{
		BlockNumber,
		Bytes,
		ChainID,
		Signature,
		H256,
		U256,
	},
};

#[test]
fn test_deserialize_u256_from_u64() {
	#[derive(Serialize, Deserialize)]
	struct Test {
		#[serde(deserialize_with = "u256_from_u64")]
		value: U256,
	}

	let test = json!({
		"value": 123u64,
	});

	let result: Test = serde_json::from_value(test).expect("Should deserialize");
	assert_eq!(result.value, U256::from(123));
}

#[test]
fn test_deserialize_u256_from_str() {
	#[derive(Serialize, Deserialize)]
	struct Test {
		#[serde(deserialize_with = "u256_from_str")]
		value: U256,
	}

	let test = json!({
		"value": "123",
	});

	let result: Test = serde_json::from_value(test).expect("Should deserialize");
	assert_eq!(result.value, U256::from(123));
}

#[test]
fn test_deserialize_u64_from_str() {
	#[derive(Serialize, Deserialize)]
	struct Test {
		#[serde(deserialize_with = "u64_from_str")]
		value: u64,
	}

	let test = json!({
		"value": "123",
	});

	let result: Test = serde_json::from_value(test).expect("Should deserialize");
	assert_eq!(result.value, 123u64);
}

#[test]
fn test_deserialize_h256_from_str() {
	#[derive(Serialize, Deserialize)]
	struct Test {
		#[serde(deserialize_with = "h256_from_str")]
		value: H256,
	}
	let random_hash = H256::random();
	let random_hash_str = hex::encode(random_hash.0);
	let test = json!({
		"value": random_hash_str,
	});

	let result: Test = serde_json::from_value(test).expect("Should deserialize");
	assert_eq!(result.value, random_hash);
}

#[test]
fn test_deserialize_signature_from_str() {
	#[derive(Serialize, Deserialize)]
	struct Test {
		#[serde(deserialize_with = "signature_from_str")]
		value: Signature,
	}
	let random_hash = H256::random().0;
	let random_hash_str = hex::encode(random_hash);
	let test = json!({
		"value": random_hash_str,
	});

	let result: Test = serde_json::from_value(test).expect("Should deserialize");
	assert_eq!(result.value, Bytes(random_hash.to_vec()));
}

#[test]
fn test_deserialize_chain_id() {
	#[derive(Debug, Serialize, Deserialize)]
	struct Test {
		value: ChainID,
	}
	let test = json!({
		"value": "1",
	});

	let result: Test = serde_json::from_value(test).expect("Should deserialize");
	assert_eq!(result.value, ChainID::Mainnet);

	let test = json!({
		"value": 1,
	});

	let result: Test = serde_json::from_value(test).expect("Should deserialize");
	assert_eq!(result.value, ChainID::Mainnet);

	let test = json!({
		"value": "123",
	});

	let result: Test = serde_json::from_value(test).expect("Should deserialize");
	assert_eq!(result.value, ChainID::Private(U256::from(123)));
}

#[test]
fn test_deserialize_block_number_from_str() {
	#[derive(Serialize, Deserialize)]
	struct Test {
		value: BlockNumber,
	}

	let test = json!({
		"value": "123",
	});

	let result: Test = serde_json::from_value(test).expect("Should deserialize");
	assert_eq!(result.value, BlockNumber::from(123));
}

//

use raiden_network_messages::messages::LockedTransfer;
use serde::Deserialize;
use serde_json::json;

#[test]
fn test_deserialize_locked_transfer() {
	let locked_transfer_content = json!({
		"chain_id": "4321",
		"message_identifier": "6964785332600670672",
		"recipient": "0x89acfcf95fdbda8f91ee68ff3856b761051bba00",
		"transferred_amount": "0",
		"channel_identifier": "1",
		"locked_amount": "10000000000000000000",
		"locksroot": "0x03c414dde5c05395d484e3382e0abdfa8c1a93539837216fcb00d8afda1d5a95",
		"token": "0xd807ccc6be4c906c08adcd2f82634c030ceb326d",
		"payment_identifier": "1677193922037",
		"token_network_address": "0xb7ee9024a9449bbdb3f1907ab0cb99d1966c0549",
		"target": "0x89acfcf95fdbda8f91ee68ff3856b761051bba00",
		"initiator": "0x1b74935e78f33695962c9ac278127335a4089882",
		"signature": "0x17908385c8aa4e70b33de9637c18fc6d044d60decfa2cfaaa345787658c0ca1376dd5aa5afb69884eba19c3701c0bacbc4301e2b7d2f3573dd157cfc7d0201721c",
		"metadata": {
			"routes": [{
				"route": [
					"0x1b74935e78f33695962c9ac278127335a4089882",
					"0x89acfcf95fdbda8f91ee68ff3856b761051bba00"
				],
				"address_metadata": {
					"0x1b74935e78f33695962c9ac278127335a4089882": {
						"user_id": "@0x1b74935e78f33695962c9ac278127335a4089882:192.168.100.49:8008",
						"capabilities": "mxc://raiden.network/cap?Receive=1&Mediate=1&Delivery=1&webRTC=1&toDevice=1&immutableMetadata=1",
						"displayname": "0x839c6ce5cf234f04c8fecdaeb77d183f3c654a26ec21e537670b48f585ea5ca46574cfd7ccc8295f7a9e4a92b1b566ef33a86a74452552f3cb050eeeadd195ec1c"
					},
					"0x89acfcf95fdbda8f91ee68ff3856b761051bba00": {
						"user_id": "@0x89acfcf95fdbda8f91ee68ff3856b761051bba00:192.168.100.49:8008",
						"capabilities": "mxc://",
						"displayname": "0xc692551a3f1f24972113e0efa489f1c5729ef88541254b37529e776872a055af64b8f101fd9af9149f7898aab7f81b3aebb116353af18f943594eb01694d33b61c"
					}
				}
			}],
			"secret": "0x04e7c84aab1cc3880f4aa18be7bcaf87f412912bf58d5b541eea8267bb11230aa22a5d888c7edff12b50d5b22b5afbeaec5beb5d28bc01d3c573523782927600ddbaaddaf57a9c222300b42c672e6e10a1002c6359eb0f5a60991bd2d7684bf79647df9e81c2a0f7df0d08a768326e9974d30b61dc750389e88b512dfd776086554ab6d1a8209256659c7ec0d7b4f7253d16d94ab1781b962233cb5ab4602d6c88efaa3bedc41e54ca1f20d2eb26360f76bf06ca890998f629f57c2b8cf0d8224ffdd9df381e5c16295def352c99fb1468deef119e860d5868419142134bc0f57f2a4843f821446d1e35f2d9fd262cadf3e875027373c627d244"
		},
		"lock": {
			"expiration": "1501",
			"amount": "10000000000000000000"
		},
		"nonce": "1",
		"type": "LockedTransfer",
	});

	let locked_transfer = LockedTransfer::deserialize(locked_transfer_content).unwrap();
}

mod consts;
mod gas;
mod manager;
mod types;

pub use gas::*;
pub use manager::*;
pub use types::*;

// #[derive(Clone)]
// pub struct Contract {
//     pub name: &'static str,
//     pub address: Address,
//     pub deploy_block_number: U64,
//     inner: ethabi::Contract,
// }

// impl Contract {
//     fn new(name: &'static str, address: Address, deploy_block_number: U64, abi: String) -> Result<Self, ethabi::Error> {
//         Ok(Self {
//             name,
//             address,
//             deploy_block_number,
//             inner: ethabi::Contract::load(abi.as_bytes())?,
//         })
//     }

//     pub fn events(&self) -> Events {
//         self.inner.events()
//     }

//     pub fn topics(&self) -> Vec<H256> {
//         let events = self.inner.events();
//         events.map(|e| e.signature()).collect()
//     }

//     pub fn filters(&self, from_block: U64, to_block: U64) -> Filter {
//         FilterBuilder::default()
//             .address(vec![self.address])
//             .topics(Some(self.topics), None, None, None)
//             .from_block(from_block.into())
//             .to_block(to_block.into())
//             .build()
//     }

// 	pub fn inner(&self) -> ethabi::Contract {
// 		self.inner
// 	}
// }

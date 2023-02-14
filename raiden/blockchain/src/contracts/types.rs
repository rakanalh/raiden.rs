#[derive(Clone, Eq, PartialEq, Hash)]
pub enum ContractIdentifier {
	TokenNetworkRegistry,
	TokenNetwork,
	SecretRegistry,
	MonitoringService,
	ServiceRegistry,
	UserDeposit,
	OneToN,
	Deposit,
	CustomToken,
	CustomTokenNoDecimals,
	HumanStandardToken,
}

impl ToString for ContractIdentifier {
	fn to_string(&self) -> String {
		match self {
			Self::TokenNetworkRegistry => "TokenNetworkRegistry".to_owned(),
			Self::TokenNetwork => "TokenNetwork".to_owned(),
			Self::SecretRegistry => "SecretRegistry".to_owned(),
			Self::MonitoringService => "MonitoringService".to_owned(),
			Self::ServiceRegistry => "ServiceRegistry".to_owned(),
			Self::UserDeposit => "UserDeposit".to_owned(),
			Self::OneToN => "OneToN".to_owned(),
			Self::Deposit => "Deposit".to_owned(),
			Self::CustomToken => "CustomToken".to_owned(),
			Self::CustomTokenNoDecimals => "CustomTokenNoDecimals".to_owned(),
			Self::HumanStandardToken => "HumanStandardToken".to_owned(),
		}
	}
}

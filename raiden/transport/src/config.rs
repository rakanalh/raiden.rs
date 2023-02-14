#[derive(Clone)]
pub struct TransportConfig {
	pub retry_timeout: u8,
	pub retry_timeout_max: u8,
	pub retry_count: u32,
	pub matrix: MatrixTransportConfig,
}

#[derive(Clone)]
pub struct MatrixTransportConfig {
	pub homeserver_url: String,
}

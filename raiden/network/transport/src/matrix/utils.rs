use std::collections::HashMap;

use reqwest;

use super::constants::{
	MATRIX_DEFAULT_DEVELOPMENT_SERVERS_LIST_URL,
	MATRIX_DEFAULT_PRODUCTION_SERVERS_LIST_URL,
};
use crate::types::EnvironmentType;

pub async fn get_default_matrix_servers(
	environment_type: EnvironmentType,
) -> reqwest::Result<Vec<String>> {
	let url = match environment_type {
		EnvironmentType::Production => MATRIX_DEFAULT_PRODUCTION_SERVERS_LIST_URL,
		EnvironmentType::Development => MATRIX_DEFAULT_DEVELOPMENT_SERVERS_LIST_URL,
	};
	let resp = reqwest::get(url).await?.json::<HashMap<String, Vec<String>>>().await?;

	let servers = resp.get("active_servers").cloned().unwrap_or(vec![]);
	Ok(servers
		.iter()
		.map(|s| if s.starts_with("http") { return s.clone() } else { format!("https://{}", s) })
		.collect())
}

pub fn select_best_server(servers: Vec<String>) -> String {
	servers.first().unwrap_or(&"".to_owned()).clone()
}

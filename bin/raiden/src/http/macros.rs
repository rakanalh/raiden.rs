#[macro_export]
macro_rules! json_response {
	($data:expr, $status_code:expr) => {
		match serde_json::to_string(&$data) {
			Ok(json) => Ok(Response::builder()
				.header(header::CONTENT_TYPE, "application/json")
				.status($status_code)
				.body(Body::from(json))
				.unwrap()),
			Err(e) => {
				let mut error_data = HashMap::new();
				error_data.insert("error", format!("{}", e));
				let error_json = serde_json::to_string(&error_data).unwrap();
				Ok(Response::builder()
					.header(header::CONTENT_TYPE, "application/json")
					.status(StatusCode::INTERNAL_SERVER_ERROR)
					.body(Body::from(error_json))
					.unwrap())
			},
		}
	};
	(status: $status:tt, body: $body:expr) => {
		hyper::Response::new()
			.with_header(hyper::header::ContentType::json())
			.with_status(hyper::StatusCode::$status)
			.with_body($body)
	};
}

#[macro_export]
macro_rules! unwrap_result_or_error {
	($data:expr, $error_code:expr) => {
		match $data {
			Ok(obj) => obj,
			Err(e) => {
				let mut error_data = HashMap::new();
				error_data.insert("error", format!("{}", e));
				let error_json = serde_json::to_string(&error_data).unwrap();
				return Ok(Response::builder()
					.header(header::CONTENT_TYPE, "application/json")
					.status($error_code)
					.body(Body::from(error_json))
					.unwrap())
			},
		}
	};
}

#[macro_export]
macro_rules! unwrap_option_or_error {
	($data:expr, $error_code:expr) => {
		match $data {
			Some(obj) => obj,
			None => {
				let mut error_data = HashMap::new();
				error_data.insert("error", format!("Not found"));
				let error_json = serde_json::to_string(&error_data).unwrap();
				return Ok(Response::builder()
					.header(header::CONTENT_TYPE, "application/json")
					.status($error_code)
					.body(Body::from(error_json))
					.unwrap())
			},
		}
	};
}

use url::{ParseError, Url};

pub trait ToHTTPEndpoint {
    fn to_http(&self) -> Result<String, ParseError>;
}

pub trait ToSocketEndpoint {
    fn to_socket(&self) -> Result<String, ParseError>;
}

impl ToHTTPEndpoint for str {
    fn to_http(&self) -> Result<String, ParseError> {
        let mut parsed = Url::parse(self)?;
        match parsed.scheme() {
            "http" | "https" => Ok(self.to_string().clone()),
            _ => {
                let _ = parsed.set_scheme("https");
                Ok(parsed.as_str().to_string())
            }
        }
    }
}

impl ToSocketEndpoint for str {
    fn to_socket(&self) -> Result<String, ParseError> {
        let mut parsed = Url::parse(self)?;
        match parsed.scheme() {
            "wss" => Ok(self.to_string().clone()),
            _ => {
                let _ = parsed.set_scheme("wss");
                Ok(parsed.as_str().to_string())
            }
        }
    }
}

pub(crate) enum Error {
    Http(hyper::Error),
    Serialization(serde_json::Error),
}

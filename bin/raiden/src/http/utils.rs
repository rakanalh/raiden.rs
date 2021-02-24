use std::sync::Arc;

use hyper::{
    Body,
    Request,
};
use parking_lot::RwLock;
use raiden::state_manager::StateManager;
use routerify::ext::RequestExt;

pub(crate) fn state_manager(req: Request<Body>) -> Arc<RwLock<StateManager>> {
    req.data::<Arc<RwLock<StateManager>>>().unwrap().clone()
}

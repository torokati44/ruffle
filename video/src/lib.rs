#![deny(clippy::unwrap_used)]

use slotmap::new_key_type;

pub mod backend;
pub mod error;
pub mod frame;
pub mod null;
pub mod queue;

new_key_type! {
    pub struct VideoStreamHandle;
}

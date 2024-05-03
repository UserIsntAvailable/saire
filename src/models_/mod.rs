pub mod canvas;
pub mod document;
pub mod layer;
pub mod thumbnail;

pub mod prelude {
    pub use super::{canvas::*, document::*, layer::*, thumbnail::*};
}

// TODO(Unavailable): serde feature.

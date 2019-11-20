use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    points: Vec<(f32, f32)>,
}

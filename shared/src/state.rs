use serde_derive::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    points: Vec<(f32, f32)>
}

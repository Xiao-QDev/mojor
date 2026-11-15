// Camera 照搬 Nukkit BlockCamera
use crate::block::Block;

pub struct Camera;
pub const INSTANCE: Camera = Camera;

impl Block for Camera {
    fn id(&self) -> u8 { 237 }
    fn name(&self) -> &'static str { "Camera" }
}

// SignPost 照搬 Nukkit BlockSignPost
use crate::block::Block;

pub struct SignPost;
pub const INSTANCE: SignPost = SignPost;

impl Block for SignPost {
    fn id(&self) -> u8 { 63 }
    fn name(&self) -> &'static str { "SignPost" }
}

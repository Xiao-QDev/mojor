// ElectricalAppliance 照搬 Nukkit BlockElectricalAppliance
use crate::block::Block;

pub struct ElectricalAppliance;
pub const INSTANCE: ElectricalAppliance = ElectricalAppliance;

impl Block for ElectricalAppliance {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "ElectricalAppliance" }
}

//! The rollback planner's volume lookup, answered by the app's volume registry.

use std::sync::Arc;

use crate::file_system::VolumeManager;
use crate::file_system::volume::Volume;
use cmdr_operation_log::rollback::VolumeLookup;

impl VolumeLookup for VolumeManager {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        VolumeManager::get(self, volume_id)
    }
}

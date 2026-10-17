//! The rollback planner's volume lookup over a `VolumeManager`, for the backend
//! suites' rollback cells that build a private registry. The write engine answers
//! the same question for production through `operations_host::volumes`.

use std::sync::Arc;

use crate::file_system::VolumeManager;
use crate::file_system::volume::Volume;
use cmdr_operation_log::rollback::VolumeLookup;

impl VolumeLookup for VolumeManager {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        VolumeManager::get(self, volume_id)
    }
}

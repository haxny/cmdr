//! What a sync session needs from the app, as one seam: volumes, the panes'
//! selections, the write engine with its sink, and the five events. The app's
//! answer is `AppSyncHost`; tests bring their own.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_listing::ListingId;
use cmdr_operations::{OperationEventSink, Operations};
use cmdr_sync::RootRef;
use tauri::AppHandle;
use tauri_specta::Event;

use super::engine::SinkFactory;
use super::wire::{
    SelectionRef, SyncCompareFinished, SyncCompareProgress, SyncRowsChanged, SyncRunFinished, SyncRunProgress,
};
use crate::operations_host::TauriEventSink;

/// The volume a custom versions folder on this Mac is written through.
pub(crate) const LOCAL_DISK_VOLUME_ID: &str = cmdr_fs::volume::DEFAULT_VOLUME_ID;

/// One event a session sends to the dialog.
#[derive(Debug, Clone)]
pub(crate) enum SyncEvent {
    CompareProgress(SyncCompareProgress),
    CompareFinished(SyncCompareFinished),
    RowsChanged(SyncRowsChanged),
    RunProgress(SyncRunProgress),
    RunFinished(SyncRunFinished),
}

/// The app, as a session sees it.
pub(crate) trait SyncHost: Send + Sync {
    /// The connected volume with this id.
    fn volume(&self, volume_id: &str) -> Option<Arc<dyn Volume>>;
    /// The names on the selected rows of a pane that shows `root`; `None` when
    /// the listing is gone, shows another folder, or moved on.
    fn selection_names(&self, root: &RootRef, selection: &SelectionRef, include_hidden: bool) -> Option<Vec<String>>;
    /// The write engine, once installed.
    fn operations(&self) -> Option<&'static Operations>;
    /// The app's sink per engine operation.
    fn sinks(&self) -> SinkFactory;
    /// Sends one event to the dialog.
    fn emit(&self, event: SyncEvent);
}

/// The app's answers: the volume registry, the listing engine, the installed
/// write engine, and Tauri events.
pub(crate) struct AppSyncHost {
    app: AppHandle,
}

impl AppSyncHost {
    pub(crate) fn new(app: AppHandle) -> Arc<Self> {
        Arc::new(Self { app })
    }
}

impl SyncHost for AppSyncHost {
    fn volume(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        crate::file_system::volume::manager::get_volume_manager().get(volume_id)
    }

    fn selection_names(&self, root: &RootRef, selection: &SelectionRef, include_hidden: bool) -> Option<Vec<String>> {
        let listings = crate::listing_host::listings();
        let id = ListingId::new(selection.listing_id.clone());
        let (volume_id, path) = listings.location(&id)?;
        if volume_id != root.volume_id || path != Path::new(&root.path) {
            return None;
        }
        listings
            .selection_names(
                &id,
                include_hidden,
                Some(&selection.indices),
                selection.expected_sequence,
            )
            .ok()
    }

    fn operations(&self) -> Option<&'static Operations> {
        crate::operations_host::operations().ok()
    }

    fn sinks(&self) -> SinkFactory {
        let app = self.app.clone();
        Arc::new(move || Arc::new(TauriEventSink::new(app.clone())) as Arc<dyn OperationEventSink>)
    }

    fn emit(&self, event: SyncEvent) {
        // A window that closed meanwhile has nobody to tell; nothing to do.
        let _ = match event {
            SyncEvent::CompareProgress(event) => event.emit(&self.app),
            SyncEvent::CompareFinished(event) => event.emit(&self.app),
            SyncEvent::RowsChanged(event) => event.emit(&self.app),
            SyncEvent::RunProgress(event) => event.emit(&self.app),
            SyncEvent::RunFinished(event) => event.emit(&self.app),
        };
    }
}

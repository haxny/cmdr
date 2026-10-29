//! The transfer configs' `destinationName` on the wire: one optional leaf,
//! absent or `null` meaning "each source keeps its own name". The frontend and
//! `bindings.ts` read exactly this shape.

use crate::types::{TransferLeaves, VolumeCopyConfig, WriteOperationConfig};
use serde_json::{Value, json};

fn write_config_round_trip(input: Value) -> Value {
    let config: WriteOperationConfig = serde_json::from_value(input).expect("a write config");
    serde_json::to_value(&config).expect("serializes")
}

#[test]
fn a_write_config_carries_its_leaf_as_destination_name() {
    let named = write_config_round_trip(json!({ "destinationName": "backup.txt" }));
    assert_eq!(named["destinationName"], json!("backup.txt"));

    for unnamed in [json!({}), json!({ "destinationName": null })] {
        let wire = write_config_round_trip(unnamed);
        assert_eq!(wire.get("destinationName"), Some(&Value::Null), "got {wire}");
    }
}

#[test]
fn a_volume_copy_config_reads_destination_name_as_its_leaf() {
    let base = json!({ "progressIntervalMs": 200, "conflictResolution": "stop", "maxConflictsToShow": 100 });
    let mut named = base.clone();
    named["destinationName"] = json!("backup.txt");
    let config: VolumeCopyConfig = serde_json::from_value(named).expect("a volume config");
    assert_eq!(config.leaves, TransferLeaves::single("backup.txt"));

    let config: VolumeCopyConfig = serde_json::from_value(base).expect("a volume config");
    assert_eq!(config.leaves, TransferLeaves::default());
}

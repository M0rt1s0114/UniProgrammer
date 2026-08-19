// DLL backend build of the CH34X sidecar. The actual implementation lives in
// `uni_ch34x_sidecar.rs`; this thin wrapper exists so the same source can be
// compiled twice with different Cargo features.
include!("uni_ch34x_sidecar.rs");

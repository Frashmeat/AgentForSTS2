//! Game-neutral execution contracts for Run, Artifact and external capability ports.

use ats_kernel::{PrimitiveId, SchemaRef};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PrimitiveContract {
    pub id: PrimitiveId,
    pub request_schema: SchemaRef,
    pub result_schema: SchemaRef,
}

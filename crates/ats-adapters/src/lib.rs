//! Infrastructure implementations selected by the application composition roots.

use ats_kernel::{PrimitiveId, SchemaRef};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AdapterContract {
    pub primitive_id: PrimitiveId,
    pub configuration_schema: SchemaRef,
}

//! Project and resource workspace contracts.

use ats_kernel::SchemaRef;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkspaceContract {
    pub schema: SchemaRef,
}

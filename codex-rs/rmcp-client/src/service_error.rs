//! Access structured MCP errors without dropping their original protocol data.

use anyhow::Error;
use rmcp::ErrorData;
use rmcp::service::ServiceError;

use crate::rmcp_client::ClientOperationError;

/// Returns the server's structured protocol error, including its original data.
pub fn mcp_error(error: &Error) -> Option<&ErrorData> {
    let ClientOperationError::Service(ServiceError::McpError(error)) = error.downcast_ref()? else {
        return None;
    };
    Some(error)
}

//! This module defines the error types for the extension system.
// TODO: contain source error
use crate::DispatchError;
use codec::Error as CodecError;

/// The error type for the extension system.
// Typically will be used as a UserError
#[derive(Debug)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum ExtensionError {
    /// Permission to perform the requested operation was denied.
    #[cfg_attr(feature = "std", error("Permission denied"))]
    PermissionError,

    /// Failed to allocate memory.
    #[cfg_attr(feature = "std", error("Failed to allocate memory"))]
    MemoryAllocationError,

    /// An error occurred while accessing memory.
    #[cfg_attr(feature = "std", error("Memory access error: {0}"))]
    MemoryAccessError(polkavm::MemoryAccessError),

    /// An error occurred while decoding data.
    #[cfg_attr(feature = "std", error("Decode error: {0}"))]
    DecodeError(CodecError),

    /// An error occurred while dispatching a call.
    #[cfg_attr(feature = "std", error("Dispatch error: {0:?}"))]
    DispatchError(#[cfg_attr(feature = "std", from)] DispatchError),

    /// The requested extension is not supported.
    #[cfg_attr(feature = "std", error("Unsupported extension"))]
    UnsupportedExtension,
}

impl From<polkavm::MemoryAccessError> for ExtensionError {
    fn from(e: polkavm::MemoryAccessError) -> Self {
        Self::MemoryAccessError(e)
    }
}

impl From<CodecError> for ExtensionError {
    fn from(e: CodecError) -> Self {
        Self::DecodeError(e)
    }
}

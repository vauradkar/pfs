//! A helper modeule to build sha256 strings
use std::path::Path;

use sha2::Digest;
use sha2::Sha256;
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::errors::Error;

/// Trait for constructing a `Sha256` digest context from various inputs.
///
/// Implementors provide an async method to build an initialized `Sha256`
/// context. A default helper `sha256_update` is provided which updates the
/// context with a slice of bytes.
pub trait Sha256Builder {
    /// Build and return a `Sha256` digest context for `self`.
    fn sha256_build(&self) -> impl std::future::Future<Output = Result<Sha256, Error>> + Send;

    /// Update the provided `context` with `data` bytes.
    ///
    /// This default implementation simply feeds `data` into the context and
    /// returns `Ok(())`. Implementors may override it if special handling is
    /// required.
    fn sha256_update(
        &self,
        data: &[u8],
        context: &mut Sha256,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
        async move {
            context.update(data);
            Ok(())
        }
    }
}

/// Convert a completed `Sha256` digest context into a hex-encoded string.
pub trait Sha256String {
    /// Consume the `Sha256` context and return the hex string representation
    /// of the digest (lowercase hex).
    fn sha256_string(self) -> impl std::future::Future<Output = Result<String, Error>> + Send;
}

impl Sha256String for Sha256 {
    async fn sha256_string(self) -> Result<String, Error> {
        Ok(format!("{:x}", self.finalize()))
    }
}

impl Sha256Builder for &Path {
    async fn sha256_build(&self) -> Result<Sha256, Error> {
        let mut file = fs::File::open(&self).await.map_err(|e| Error::Read {
            what: self.to_string_lossy().to_string(),
            how: e.to_string(),
        })?;
        let mut context = Sha256::new();
        let mut buffer = vec![0; 4096]; // Read in chunks

        loop {
            let bytes_read = file.read(&mut buffer).await.map_err(|e| Error::Read {
                what: self.to_string_lossy().to_string(),
                how: e.to_string(),
            })?;

            if bytes_read == 0 {
                break; // End of file
            }
            context.update(&buffer[..bytes_read]);
        }
        Ok(context)
    }
}

/// `Sha256Builder` implementation for byte slices. Builds a digest context
/// from the provided in-memory bytes.
impl Sha256Builder for &[u8] {
    async fn sha256_build(&self) -> Result<Sha256, Error> {
        let mut context = Sha256::new();
        self.sha256_update(self, &mut context).await?;
        Ok(context)
    }
}

// (impl for &[u8] moved above with documentation)

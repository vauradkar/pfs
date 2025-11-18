use std::path::Path;

use sha2::Digest;
use sha2::Sha256;
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::errors::Error;

pub(crate) trait Sha256Builder {
    async fn sha256_build(&self) -> Result<Sha256, Error>;

    async fn sha256_update(&self, data: &[u8], context: &mut Sha256) -> Result<(), Error> {
        context.update(data);
        Ok(())
    }
}

pub(crate) trait Sha256String {
    async fn sha256_string(self) -> Result<String, Error>;
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

impl Sha256Builder for &[u8] {
    async fn sha256_build(&self) -> Result<Sha256, Error> {
        let mut context = Sha256::new();
        self.sha256_update(self, &mut context).await?;
        Ok(context)
    }
}

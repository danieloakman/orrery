//! Newline-delimited JSON over a Unix stream.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use thiserror::Error;

use crate::protocol::{Request, Response};

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("server error: {0}")]
    Server(String),
    #[error("empty response")]
    Empty,
}

pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Client {
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, IpcError> {
        let stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_secs(600)))?;
        stream.set_write_timeout(Some(Duration::from_secs(60)))?;
        let writer = stream.try_clone()?;
        Ok(Self {
            reader: BufReader::new(stream),
            writer,
        })
    }

    pub fn call(&mut self, req: &Request) -> Result<Response, IpcError> {
        let mut line = serde_json::to_string(req)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes())?;
        self.writer.flush()?;

        let mut resp_line = String::new();
        let n = self.reader.read_line(&mut resp_line)?;
        if n == 0 {
            return Err(IpcError::Empty);
        }
        Ok(serde_json::from_str(resp_line.trim_end())?)
    }

    pub fn call_ok(&mut self, req: &Request) -> Result<crate::protocol::OkBody, IpcError> {
        match self.call(req)? {
            Response::Ok(body) => Ok(body),
            Response::Err { error } => Err(IpcError::Server(error)),
        }
    }
}

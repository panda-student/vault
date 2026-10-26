//! Final MCP wire boundary, including SDK-generated errors.
use serde_json::Value;
use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
};
use tokio::io::AsyncWrite;
use vault_core::api::{CoreSession, SharedGuard};
pub struct SafeWriter<W> {
    inner: W,
    guard: SharedGuard,
    core: Option<Arc<CoreSession>>,
    input: Vec<u8>,
    pending: Vec<u8>,
    offset: usize,
}
impl<W> SafeWriter<W> {
    pub fn with_core(mut self, core: Arc<CoreSession>) -> Self {
        self.core = Some(core);
        self
    }
    pub fn new(inner: W, guard: SharedGuard) -> Self {
        Self {
            inner,
            guard,
            core: None,
            input: vec![],
            pending: vec![],
            offset: 0,
        }
    }
}
impl<W: AsyncWrite + Unpin> SafeWriter<W> {
    fn drain(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.offset < self.pending.len() {
            let written =
                ready!(Pin::new(&mut self.inner).poll_write(cx, &self.pending[self.offset..]))?;
            if written == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            self.offset += written;
        }
        self.pending.clear();
        self.offset = 0;
        Poll::Ready(Ok(()))
    }
}
impl<W: AsyncWrite + Unpin> AsyncWrite for SafeWriter<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.drain(cx))?;
        if self.input.len() + bytes.len() > 4 * 1024 * 1024 {
            return Poll::Ready(Err(io::Error::other("MCP output limit exceeded")));
        }
        self.input.extend_from_slice(bytes);
        while let Some(end) = self.input.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.input.drain(..=end).collect();
            let mut value: Value = serde_json::from_slice(&line)
                .map_err(|_| io::Error::other("Invalid outgoing protocol frame"))?;
            {
                let mut guard = self
                    .guard
                    .write()
                    .map_err(|_| io::Error::other("Output guard unavailable"))?;
                if let Some(core) = &self.core {
                    core.refresh_output_guard(&mut guard)
                        .map_err(|_| io::Error::other("Output protection refresh failed"))?;
                }
                guard.scrub(&mut value);
            }
            let encoded = serde_json::to_vec(&value)
                .map_err(|_| io::Error::other("Output encoding failed"))?;
            self.pending.extend(encoded);
            self.pending.push(b'\n');
        }
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.drain(cx))?;
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.drain(cx))?;
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;
    use vault_core::api::OutputGuard;
    #[test]
    fn protocol_guard_handles_split_frames_and_sdk_error_text() {
        use tokio::io::AsyncWriteExt;
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async{
            let mut output=OutputGuard::default();output.add("synthetic-global-secret-456");
            let mut writer=SafeWriter::new(Vec::<u8>::new(),Arc::new(RwLock::new(output)));
            writer.write_all(br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"synthetic-global-"#).await.unwrap();
            writer.write_all(b"secret-456\"}}\n").await.unwrap();writer.flush().await.unwrap();
            let text=String::from_utf8(writer.inner).unwrap();assert!(!text.contains("synthetic-global-secret-456"));let value:Value=serde_json::from_str(&text).unwrap();assert!(value["error"]["message"].is_null());
        });
    }
}

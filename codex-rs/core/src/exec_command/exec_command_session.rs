use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::sync::mpsc;

use crate::exec_command::session_id::SessionId;

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct ExecCommandSession {
    pub(crate) id: SessionId,
    /// Queue for writing bytes to the process stdin (PTY master write side).
    writer_tx: mpsc::Sender<Vec<u8>>,
    /// Stream of output chunks read from the PTY. Wrapped in Mutex so callers can
    /// `await` receiving without needing `&mut self`.
    output_rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}

#[allow(dead_code)]
impl ExecCommandSession {
    pub(crate) fn new(
        id: SessionId,
        writer_tx: mpsc::Sender<Vec<u8>>,
        output_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            id,
            writer_tx,
            output_rx: Arc::new(Mutex::new(output_rx)),
        }
    }

    /// Enqueue bytes to be written to the process stdin (PTY master).
    pub(crate) async fn write_stdin(&self, bytes: impl AsRef<[u8]>) -> anyhow::Result<()> {
        self.writer_tx
            .send(bytes.as_ref().to_vec())
            .await
            .map_err(|e| anyhow::anyhow!("failed to send to writer: {e}"))
    }

    /// Receive the next chunk of output from the process. Returns `None` when the
    /// output stream is closed (process exited or reader finished).
    pub(crate) async fn recv_output_chunk(&self) -> Option<Vec<u8>> {
        self.output_rx.lock().await.recv().await
    }

    pub(crate) fn writer_sender(&self) -> mpsc::Sender<Vec<u8>> {
        self.writer_tx.clone()
    }

    pub(crate) fn output_receiver(&self) -> Arc<Mutex<mpsc::Receiver<Vec<u8>>>> {
        self.output_rx.clone()
    }
}

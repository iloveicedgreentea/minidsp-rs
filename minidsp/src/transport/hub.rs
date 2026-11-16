//! Provides a way to share a transport on a frame level, all received frames are forward to all clients.

use std::{
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::{channel::mpsc, stream::SplitSink, Sink, SinkExt, Stream, StreamExt};
use futures_util::ready;
use pin_project::pin_project;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use super::{MiniDSPError, Transport};
use crate::utils::OwnedJoinHandle;

const CAPACITY: usize = 100;

/// Clonable transport which implements frame level forwarding
#[pin_project]
pub struct Hub {
    // Shared data between clients
    inner: Arc<Mutex<Option<Inner>>>,
    handles: Arc<Mutex<Vec<OwnedJoinHandle<()>>>>,

    /// Stream where client can read frames from the device.
    #[pin]
    device_rx: BroadcastStream<Bytes>,
    /// Sink where client can write frames to the device.
    #[pin]
    device_tx: mpsc::Sender<Bytes>,
}

impl Hub {
    pub fn new(transport: Transport) -> Self {
        let (read_tx, read_rx) = broadcast::channel::<Bytes>(CAPACITY);
        let read_tx = Arc::new(read_tx);
        let (send_tx, mut send_rx) = mpsc::channel(CAPACITY);
        let (device_tx, mut device_rx) = transport.split();

        let inner = Inner::new(Arc::downgrade(&read_tx), device_tx);

        let read_handle = {
            OwnedJoinHandle::new(tokio::spawn(async move {
                while let Some(frame) = device_rx.next().await {
                    match frame {
                        Ok(frame) => {
                            let result = read_tx.send(frame);
                            if let Err(e) = result {
                                log::error!("send error {e}");
                                break;
                            }
                        }
                        Err(e) => {
                            log::error!("recv error {e}");
                            break;
                        }
                    }
                }
            }))
        };

        let send_handle = {
            let transport_sink_shared = inner.transport_sink.clone();

            OwnedJoinHandle::new(tokio::spawn({
                async move {
                    let mut transport_sink = transport_sink_shared.lock().await;
                    while let Some(frame) = send_rx.next().await {
                        let res = transport_sink.send(frame).await;
                        if let Err(e) = res {
                            log::error!("error sending to device: {e}");
                            break;
                        }
                    }
                }
            }))
        };

        let handles = Arc::new(Mutex::new(vec![read_handle, send_handle]));

        Self {
            inner: Arc::new(Mutex::new(Some(inner))),
            handles,
            device_rx: BroadcastStream::new(read_rx),
            device_tx: send_tx,
        }
    }

    pub async fn shutdown(&self) {
        let handles: Vec<_> = { self.handles.lock().unwrap().drain(..).collect() };
        for h in handles.iter() {
            h.abort();
        }
        let inner = { self.inner.lock().unwrap().take() };
        if let Some(inner) = inner {
            let mut transport_sink = inner.transport_sink.lock().await;
            let res = transport_sink.close().await;
            if let Err(e) = res {
                log::error!("error closing transport: {e}");
            }
        }
        for h in handles {
            let res = h.await;
            if let Err(e) = res {
                if !e.is_cancelled() {
                    log::error!("error joining handle: {e}");
                }
            }
        }
    }

    /// Clones the transport if it is still available, returns None if it has been closed
    pub fn try_clone(&self) -> Option<Self> {
        let inner = self.inner.lock().unwrap();
        let device_rx = BroadcastStream::new(inner.as_ref()?.device_rx.upgrade()?.subscribe());
        Some(Self {
            inner: self.inner.clone(),
            handles: self.handles.clone(),
            device_rx,
            device_tx: self.device_tx.clone(),
        })
    }
}

impl Stream for Hub {
    type Item = Result<Bytes, MiniDSPError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut device_rx: Pin<&mut BroadcastStream<_>> = self.project().device_rx;
        loop {
            let res = ready!(device_rx.as_mut().poll_next(cx));
            return Poll::Ready(match res {
                Some(Ok(obj)) => Some(Ok(obj)),
                Some(Err(e)) => {
                    log::warn!("lost messages: {e:?}");
                    continue;
                }
                None => None,
            });
        }
    }
}

impl Sink<Bytes> for Hub {
    type Error = MiniDSPError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project()
            .device_tx
            .poll_ready(cx)
            .map_err(|_| MiniDSPError::TransportClosed)
    }

    fn start_send(self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        self.project()
            .device_tx
            .start_send(item)
            .map_err(|_| MiniDSPError::TransportClosed)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project()
            .device_tx
            .poll_flush(cx)
            .map_err(|_| MiniDSPError::TransportClosed)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project()
            .device_tx
            .poll_close(cx)
            .map_err(|_| MiniDSPError::TransportClosed)
    }
}

struct Inner {
    // Only the receive task owns the sender. When the transport ends or fails,
    // dropping it closes every subscriber and lets the device manager reconnect.
    device_rx: Weak<broadcast::Sender<Bytes>>,
    transport_sink: Arc<tokio::sync::Mutex<SplitSink<Transport, Bytes>>>,
}

impl Inner {
    pub fn new(
        device_rx: Weak<broadcast::Sender<Bytes>>,
        transport_sink: SplitSink<Transport, Bytes>,
    ) -> Self {
        Self {
            device_rx,
            transport_sink: Arc::new(tokio::sync::Mutex::new(transport_sink)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::utils::Combine;

    fn test_transport() -> (mpsc::UnboundedSender<Result<Bytes, MiniDSPError>>, Hub) {
        let (input, stream) = mpsc::unbounded();
        let (sink, _output) = mpsc::unbounded::<Bytes>();
        let sink = sink.sink_map_err(|_| MiniDSPError::TransportClosed);
        (input, Hub::new(Box::pin(Combine::new(stream, sink))))
    }

    #[tokio::test]
    async fn closes_all_subscribers_when_transport_ends() {
        let (input, mut hub) = test_transport();
        let mut subscriber = hub.try_clone().unwrap();
        input
            .unbounded_send(Ok(Bytes::from_static(b"frame")))
            .unwrap();
        assert_eq!(hub.next().await.unwrap().unwrap(), &b"frame"[..]);
        assert_eq!(subscriber.next().await.unwrap().unwrap(), &b"frame"[..]);
        drop(input);
        assert!(tokio::time::timeout(Duration::from_secs(1), hub.next())
            .await
            .unwrap()
            .is_none());
        assert!(subscriber.next().await.is_none());
        assert!(hub.try_clone().is_none());
    }

    #[tokio::test]
    async fn closes_all_subscribers_when_transport_fails() {
        let (input, mut hub) = test_transport();
        let mut subscriber = hub.try_clone().unwrap();
        input
            .unbounded_send(Err(MiniDSPError::TransportClosed))
            .unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(1), hub.next())
            .await
            .unwrap()
            .is_none());
        assert!(subscriber.next().await.is_none());
        assert!(hub.try_clone().is_none());
        // Keep the input channel alive: the transport error alone must end the hub.
        drop(input);
    }
}

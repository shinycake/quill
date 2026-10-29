//! External frames on the mock engine: a sender per connected call that
//! records what a feeder pushed, for capture and driver tests.
use super::*;

/// One recorded external frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockExternalFrame {
    pub target: CaptureTarget,
    pub device: ExternalDevice,
    pub bytes: Vec<u8>,
    pub meta: ExternalFrameMeta,
}

struct MockFrameSender {
    inner: Arc<Mutex<MockInner>>,
    target: CaptureTarget,
}

impl ExternalFrameSender for MockFrameSender {
    fn send(
        &self,
        device: ExternalDevice,
        data: &[u8],
        meta: ExternalFrameMeta,
    ) -> Result<(), EngineError> {
        if !external_frame_valid(device, data.len(), meta) {
            return Err(EngineError::Engine {
                op: "qc_external_frame",
                code: -1,
            });
        }
        let mut inner = self.inner.lock_or_recover();
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.external_frames.push(MockExternalFrame {
            target: self.target,
            device,
            bytes: data.to_vec(),
            meta,
        });
        Ok(())
    }
}

impl MockEngine {
    /// Every external frame pushed so far, oldest first.
    pub fn external_frames(&self) -> Vec<MockExternalFrame> {
        self.inner.lock_or_recover().external_frames.clone()
    }

    /// A sender once the call's sources exist: a connected 1:1 call or a
    /// connected group call, as on the native engine.
    pub(super) fn mock_external_sender(
        &self,
        target: CaptureTarget,
    ) -> Option<Arc<dyn ExternalFrameSender>> {
        let inner = self.inner.lock_or_recover();
        if !inner.available {
            return None;
        }
        let ready = match target {
            CaptureTarget::Private(call_id) => {
                inner.calls.contains_key(&call_id)
                    && inner.connects.iter().any(|(id, _)| *id == call_id)
            }
            CaptureTarget::Group(group_call_id) => {
                inner
                    .group_connects
                    .iter()
                    .any(|(id, _, _)| *id == group_call_id)
                    && !inner.group_leaves.contains(&group_call_id)
            }
        };
        drop(inner);
        ready.then(|| {
            Arc::new(MockFrameSender {
                inner: self.inner.clone(),
                target,
            }) as Arc<dyn ExternalFrameSender>
        })
    }
}

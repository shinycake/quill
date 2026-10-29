use super::super::CallbackShared;
use super::*;
use crate::calls::engine::SsrcOwners;
use std::sync::atomic::AtomicUsize;

/// Encrypts by appending 0xEE, decrypts by prefixing the sender's id byte.
#[derive(Default)]
struct Marking {
    calls: AtomicUsize,
}

impl FrameCrypto for Marking {
    fn encrypt(&self, frame: &[u8], unencrypted_prefix: i32) -> Option<Vec<u8>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut out = frame.to_vec();
        out.push(0xEE);
        out.push(unencrypted_prefix as u8);
        Some(out)
    }

    fn decrypt(&self, sender_user_id: i64, frame: &[u8]) -> Option<Vec<u8>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        (sender_user_id != 0).then(|| {
            let mut out = vec![sender_user_id as u8];
            out.extend_from_slice(frame);
            out
        })
    }
}

fn conference_context() -> (Arc<GroupContext>, Arc<Marking>) {
    let crypto = Arc::new(Marking::default());
    let shared = Arc::new(CallbackShared::new());
    let context = GroupContext::new(-1, 0, false, &shared).conference(crypto.clone());
    (Arc::new(context), crypto)
}

/// What `set_result` was handed.
unsafe extern "C" fn collect(result: *mut c_void, data: *const u8, length: usize) {
    // SAFETY: the tests pass a `Vec<u8>` as `result` and `length` bytes at `data`.
    unsafe {
        let out = &mut *result.cast::<Vec<u8>>();
        *out = std::slice::from_raw_parts(data, length).to_vec();
    }
}

fn run(context: &Arc<GroupContext>, frame: &qc_group_e2e_frame) -> Vec<u8> {
    let callbacks = context.callbacks();
    let e2e = callbacks.e2e.expect("a conference call wires e2e");
    let mut out = Vec::new();
    // SAFETY: `user` is the live context and `out` outlives the call.
    unsafe {
        e2e(
            callbacks.user,
            frame,
            collect,
            std::ptr::from_mut(&mut out).cast::<c_void>(),
        )
    };
    out
}

#[test]
fn frames_go_through_the_call_s_crypto() {
    let (context, crypto) = conference_context();
    let data = [1u8, 2, 3];
    let encrypt = qc_group_e2e_frame {
        data: data.as_ptr(),
        length: data.len(),
        is_encrypt: 1,
        user_id: 0,
        unencrypted_prefix_size: 1,
    };
    assert_eq!(run(&context, &encrypt), vec![1, 2, 3, 0xEE, 1]);
    let decrypt = qc_group_e2e_frame {
        is_encrypt: 0,
        user_id: 9,
        ..encrypt
    };
    assert_eq!(run(&context, &decrypt), vec![9, 1, 2, 3]);
    // A refused frame hands nothing back: tgcalls drops it.
    let unknown = qc_group_e2e_frame {
        user_id: 0,
        ..decrypt
    };
    assert!(run(&context, &unknown).is_empty());
    assert_eq!(crypto.calls.load(Ordering::SeqCst), 3);
}

#[test]
fn only_a_conference_call_wires_e2e_and_media_channels() {
    let shared = Arc::new(CallbackShared::new());
    let plain = Arc::new(GroupContext::new(7, -70, false, &shared)).callbacks();
    assert!(plain.e2e.is_none());
    assert!(plain.request_media_channels.is_none());

    let (call, _) = conference_context();
    let call = call.callbacks();
    assert!(call.e2e.is_some());
    assert!(call.request_media_channels.is_some());

    let crypto = Arc::new(Marking::default());
    let screen = GroupContext::new(7, 0, true, &shared).conference(crypto);
    let screen = Arc::new(screen).callbacks();
    assert!(screen.e2e.is_some());
    assert!(screen.request_media_channels.is_none());
}

fn owners(owners: &[(u32, i64)], complete: bool) -> SsrcOwners {
    SsrcOwners {
        owners: owners.to_vec(),
        complete,
    }
}

#[test]
fn media_channel_requests_wait_for_their_owners() {
    let conference = Conference::new(Arc::new(Marking::default()));
    // Known at once: answered in place.
    conference.set_owners(&owners(&[(100, 7)], false));
    let answer = conference.request(1, vec![100]).expect("known ssrc");
    assert_eq!((answer[0].audio_ssrc, answer[0].user_id), (100, 7));

    // An unknown ssrc waits for the participant to show up.
    assert!(conference.request(2, vec![100, 200]).is_none());
    assert!(conference.request(3, vec![300]).is_none());
    assert_eq!(conference.pending(), vec![2, 3]);
    let ready = conference.set_owners(&owners(&[(100, 7), (200, 8)], false));
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].0, 2);
    assert_eq!(
        ready[0]
            .1
            .iter()
            .map(|c| (c.audio_ssrc, c.user_id))
            .collect::<Vec<_>>(),
        vec![(100, 7), (200, 8)]
    );
    assert_eq!(conference.pending(), vec![3]);

    // Once every participant is loaded, what's left is answered as is.
    let ready = conference.set_owners(&owners(&[(100, 7), (200, 8)], true));
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].0, 3);
    assert!(ready[0].1.is_empty());
    assert!(conference.pending().is_empty());
}

#[test]
fn a_cancelled_media_channel_request_is_dropped() {
    let conference = Conference::new(Arc::new(Marking::default()));
    assert!(conference.request(4, vec![500]).is_none());
    assert!(!conference.cancel(5));
    assert!(conference.cancel(4));
    assert!(conference.pending().is_empty());
    assert!(conference.set_owners(&owners(&[(500, 1)], true)).is_empty());
}

#[test]
fn a_closed_conference_drops_frames_without_asking_its_crypto() {
    let (context, crypto) = conference_context();
    let data = [1u8, 2, 3];
    let frame = qc_group_e2e_frame {
        data: data.as_ptr(),
        length: data.len(),
        is_encrypt: 1,
        user_id: 0,
        unencrypted_prefix_size: 0,
    };
    context.conference.as_ref().expect("conference").close();
    assert!(run(&context, &frame).is_empty());
    assert_eq!(crypto.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn an_answer_made_before_the_handle_is_sent_once_it_is_attached() {
    let conference = Conference::new(Arc::new(Marking::default()));
    conference.set_owners(&owners(&[(100, 7)], false));
    let answer = conference.request(1, vec![100]).expect("known ssrc");
    // No handle yet (still inside `qc_group_create`): kept, not lost.
    assert_eq!(conference.deliver(1, &answer), None);
    assert_eq!(conference.unsent(), vec![1]);
    let mut handle = 0u8;
    let handle = std::ptr::from_mut(&mut handle).cast::<qc_group>();
    let sent = conference.attach(handle);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 1);
    assert_eq!((sent[0].1[0].audio_ssrc, sent[0].1[0].user_id), (100, 7));
    assert!(conference.unsent().is_empty());
    // Attached: answers go straight to the instance.
    assert_eq!(conference.deliver(2, &answer), Some(handle));
    // Closed: nothing is kept or sent, and a late attach does nothing.
    conference.close();
    assert_eq!(conference.deliver(3, &answer), None);
    assert!(conference.unsent().is_empty());
    assert!(conference.attach(handle).is_empty());
    assert_eq!(conference.deliver(4, &answer), None);
}

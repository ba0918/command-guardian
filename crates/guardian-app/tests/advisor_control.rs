use guardian_advisor::Mode;
use guardian_app::advisor::control::{Control, negotiation_deadline, probe_deadline};
use guardian_core::Verdict;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

// @kotowari[REQ-advisor-020, EX-advisor-039, EX-advisor-045, EX-advisor-048, EX-advisor-050]
#[test]
fn conservative_probe_and_notification_deadlines_preserve_whole_advice_budget_and_reservation() {
    let zero = Instant::now();
    let ms = |n| zero + Duration::from_millis(n);
    assert_eq!(probe_deadline(ms(100), ms(150), 5880), Some(ms(5980)));
    assert_eq!(probe_deadline(ms(100), ms(201), 5880), None);
    assert_eq!(probe_deadline(ms(100), ms(150), 0), None);
    let (notification, advice) = negotiation_deadline(ms(5000), ms(5980), 10000).unwrap();
    assert_eq!(notification, ms(5100));
    assert_eq!(advice, ms(15000));
    assert_eq!(advice.duration_since(ms(5075)), Duration::from_millis(9925));
    assert!(negotiation_deadline(ms(5500), ms(5980), 10000).is_none());
    assert!(negotiation_deadline(ms(6100), ms(5980), 10000).is_none());
    assert_eq!(
        negotiation_deadline(ms(5000), ms(5980), 50),
        Some((ms(5050), ms(5050)))
    );
}

fn receive(socket: &mut UnixStream) -> serde_json::Value {
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        socket.read_exact(&mut byte).unwrap();
        if byte[0] == b'\n' {
            break;
        }
        bytes.push(byte[0]);
    }
    serde_json::from_slice(&bytes).unwrap()
}
fn send(socket: &mut UnixStream, value: serde_json::Value) {
    writeln!(socket, "{value}").unwrap();
}

// @kotowari[REQ-advisor-020, EX-advisor-040, EX-advisor-046, EX-advisor-047]
#[test]
fn real_control_fd_requires_matching_probe_and_first_timely_ack_and_never_retries() {
    for behavior in [
        "valid",
        "old",
        "wrong_nonce",
        "lost_ack",
        "reject",
        "no_ack",
    ] {
        let (socket, mut peer) = UnixStream::pair().unwrap();
        let child = std::thread::spawn(move || {
            let probe = receive(&mut peer);
            assert_eq!(probe["kind"], "budget_probe");
            if behavior == "old" {
                return;
            }
            let nonce = if behavior == "wrong_nonce" {
                serde_json::json!("00000000000000000000000000000000")
            } else {
                probe["nonce"].clone()
            };
            send(
                &mut peer,
                serde_json::json!({"version":1,"nonce":nonce,"kind":"budget_reply","original_remaining_ms":5980}),
            );
            if behavior == "wrong_nonce" {
                return;
            }
            let start = receive(&mut peer);
            assert_eq!(start["kind"], "advisory_start");
            assert_eq!(start["timeout_ms"], 10000);
            if behavior == "lost_ack" {
                return;
            }
            if behavior == "no_ack" {
                std::thread::sleep(Duration::from_millis(150));
                return;
            }
            send(
                &mut peer,
                serde_json::json!({"version":1,"nonce":start["nonce"],"kind":"advisory_ack","accepted":behavior == "valid"}),
            );
        });
        let mut control = Control::new(socket, [7; 16]);
        let probed = control.probe(Mode::Enforce);
        if ["old", "wrong_nonce"].contains(&behavior) {
            assert!(!probed);
        }
        let finished = Instant::now();
        let deadline = control.start(Mode::Enforce, Verdict::Allow, finished, 10000);
        assert_eq!(deadline.is_some(), behavior == "valid");
        if let Some(deadline) = deadline {
            assert_eq!(deadline.duration_since(finished), Duration::from_secs(10));
        }
        assert!(
            control
                .start(Mode::Enforce, Verdict::Allow, Instant::now(), 10000)
                .is_none()
        );
        assert!(finished.elapsed() < Duration::from_millis(500));
        child.join().unwrap();
    }
}

// @kotowari[REQ-advisor-020, EX-advisor-049]
#[test]
fn off_never_probes_and_block_never_notifies_or_starts_advice() {
    let (socket, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_millis(10)))
        .unwrap();
    let mut control = Control::new(socket, [7; 16]);
    assert!(!control.probe(Mode::Off));
    assert!(
        control
            .start(Mode::Off, Verdict::Allow, Instant::now(), 2000)
            .is_none()
    );
    assert!(peer.read(&mut [0; 1]).is_err());
    let (socket, mut peer) = UnixStream::pair().unwrap();
    let child = std::thread::spawn(move || {
        let probe = receive(&mut peer);
        send(
            &mut peer,
            serde_json::json!({"version":1,"nonce":probe["nonce"],"kind":"budget_reply","original_remaining_ms":5980}),
        );
        peer.set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        assert!(peer.read(&mut [0; 1]).is_err());
    });
    let mut control = Control::new(socket, [7; 16]);
    assert!(control.probe(Mode::Enforce));
    assert!(
        control
            .start(Mode::Enforce, Verdict::Block, Instant::now(), 2000)
            .is_none()
    );
    child.join().unwrap();
}

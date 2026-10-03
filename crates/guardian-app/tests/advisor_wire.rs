use guardian_advisor::{Failure, RawDistribution, ScopeEvidence};
use guardian_app::advisor::wire::{read_reply, write_reply, FrameKind, Reply};
use std::io::Write;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(1)
}

fn reply() -> Reply {
    Reply::Assessment {
        risk: RawDistribution {
            selected: "no_harm".into(),
            probabilities: vec![
                ("harmful_irreversible".into(), 0.0),
                ("major_destructive".into(), 0.0),
                ("irreversible_only".into(), 0.0),
                ("no_harm".into(), 1.0),
                ("unknown".into(), 0.0),
            ],
        },
        scope: RawDistribution {
            selected: "unknown".into(),
            probabilities: vec![
                ("matched".into(), 0.0),
                ("mismatched".into(), 0.0),
                ("unknown".into(), 1.0),
            ],
        },
        evidence: ScopeEvidence::Unavailable,
    }
}

fn frame(body: &[u8]) -> Vec<u8> {
    let mut frame = vec![1];
    frame.extend_from_slice(&[7; 16]);
    frame.push(FrameKind::Reply as u8);
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(body);
    frame
}

fn receive(bytes: Vec<u8>) -> Result<Reply, Failure> {
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    let sender = std::thread::spawn(move || {
        let _ = writer.write_all(&bytes);
        writer.shutdown(Shutdown::Write).unwrap();
    });
    let result = read_reply(&mut reader, [7; 16], deadline());
    sender.join().unwrap();
    result
}

// @kotowari[REQ-advisor-019, EX-advisor-037]
#[test]
fn one_bound_unix_socket_reply_proceeds_to_common_assessment_validation() {
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    let sender = std::thread::spawn(move || {
        write_reply(&mut writer, [7; 16], &reply(), deadline()).unwrap()
    });
    let reply = read_reply(&mut reader, [7; 16], deadline()).unwrap();
    let (assessment, evidence) = reply.validated().unwrap().unwrap();
    assert_eq!(assessment.risk_distribution().selected, "no_harm");
    assert_eq!(evidence, ScopeEvidence::Unavailable);
    sender.join().unwrap();
}

// @kotowari[REQ-advisor-019, EX-advisor-038]
#[test]
fn wrong_header_huge_length_duplicate_missing_extra_fields_and_trailing_frames_are_rejected() {
    let body = serde_json::to_vec(&reply()).unwrap();
    let valid = frame(&body);
    for index in [0, 1, 17] {
        let mut wrong = valid.clone();
        wrong[index] ^= 0xff;
        assert!(receive(wrong).is_err());
    }
    let mut huge = valid[..22].to_vec();
    huge[18..22].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(receive(huge).is_err());
    assert!(receive(valid[..valid.len() - 1].to_vec()).is_err());
    let mut extra = valid.clone();
    extra.extend_from_slice(&valid);
    assert!(receive(extra).is_err());
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    for field in value.as_object().unwrap().keys() {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(receive(frame(&serde_json::to_vec(&missing).unwrap())).is_err());
    }
    let mut extra = value.clone();
    extra["unrecognized"] = true.into();
    assert!(receive(frame(&serde_json::to_vec(&extra).unwrap())).is_err());
    let duplicate = format!(
        "{{\"status\":\"invalid\",{}",
        String::from_utf8(body).unwrap().trim_start_matches('{')
    );
    assert!(receive(frame(duplicate.as_bytes())).is_err());
    assert!(receive(frame(&[255])).is_err());
}

// @kotowari[REQ-advisor-019, EX-advisor-038]
#[test]
fn stopped_socket_and_length_overflow_are_bounded_by_the_supplied_deadline() {
    use guardian_app::advisor::wire::request_limit;
    assert!(request_limit(usize::MAX).is_err());
    assert_eq!(request_limit(65536).unwrap(), 131072);
    let (mut reader, _writer) = UnixStream::pair().unwrap();
    assert_eq!(
        read_reply(
            &mut reader,
            [7; 16],
            Instant::now() + Duration::from_millis(20)
        ),
        Err(Failure::Timeout)
    );
}

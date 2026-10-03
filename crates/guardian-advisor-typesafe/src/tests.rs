use super::*;
use guardian_advisor::{AdvisorClient, Assessment, Failure};
use guardian_core::Verdict;
use std::time::Duration;

struct Fixture {
    outcome: Result<Response, Failure>,
    posts: Vec<Vec<u8>>,
}

impl Transport for Fixture {
    fn post(
        &mut self,
        body: &[u8],
        _authorization: &str,
        _remaining: Duration,
    ) -> Result<Response, Failure> {
        self.posts.push(body.to_vec());
        self.outcome.clone()
    }
}

fn request() -> SendPermit {
    prepare(
        &State {
            command: "printf fixture-safe".into(),
            cwd: "/fixture/work".into(),
            machine: Verdict::Allow,
            reasons: Vec::new(),
            context: Default::default(),
        },
        "fixture-model",
        65536,
    )
    .unwrap()
}

fn answer() -> Vec<u8> {
    json!({"model":"fixture-model","usage":{"input_tokens":1,"output_tokens":1},"answers": {
        "risk":{"type":"choice","choice":"major_destructive","confidence":0.01,"probabilities":{"harmful_irreversible":0.01,"major_destructive":0.95,"irreversible_only":0.01,"no_harm":0.01,"unknown":0.02}},
        "scope":{"type":"choice","choice":"unknown","confidence":1.0,"probabilities":{"matched":0.01,"mismatched":0.01,"unknown":0.98}}
    }}).to_string().into_bytes()
}

fn client(outcome: Result<Response, Failure>) -> Client<Fixture> {
    Client {
        transport: Fixture {
            outcome,
            posts: Vec::new(),
        },
        key: Some("fixture-not-a-real-credential".into()),
        sent: false,
    }
}

// @kotowari[REQ-advisor-012, REQ-advisor-013, REQ-advisor-014, EX-advisor-023, EX-advisor-025, EX-advisor-027]
#[test]
fn one_post_returns_validated_distributions_instead_of_provider_confidence() {
    let mut client = client(Ok(Response {
        status: 200,
        body: answer(),
    }));
    let permit = request();
    let assessment: Assessment = client.assess(&permit, Duration::from_secs(2)).unwrap();
    assert_eq!(assessment.risk_distribution().selected, "major_destructive");
    assert_eq!(
        assessment
            .risk_distribution()
            .probabilities
            .iter()
            .find(|(key, _)| key == "major_destructive")
            .unwrap()
            .1,
        0.95
    );
    assert_eq!(client.transport.posts, vec![permit.body().to_vec()]);
    assert_eq!(
        client.assess(&permit, Duration::from_secs(2)),
        Err(Failure::ServiceResponse)
    );
    assert_eq!(client.transport.posts.len(), 1);
}

// @kotowari[REQ-advisor-012, REQ-advisor-014, EX-advisor-028]
#[test]
fn service_redirect_rate_limit_and_connection_failure_never_retry_or_expose_error_bodies() {
    for status in [301, 302, 307, 429, 500] {
        let mut client = client(Ok(Response {
            status,
            body: b"private fixture error body".to_vec(),
        }));
        assert_eq!(
            client.assess(&request(), Duration::from_secs(2)),
            Err(Failure::ServiceResponse)
        );
        assert_eq!(client.transport.posts.len(), 1);
    }
    for failure in [Failure::Communication, Failure::Timeout] {
        let mut client = client(Err(failure));
        assert_eq!(
            client.assess(&request(), Duration::from_secs(2)),
            Err(failure)
        );
        assert_eq!(client.transport.posts.len(), 1);
    }
    let mut missing = client(Ok(Response {
        status: 200,
        body: answer(),
    }));
    missing.key = None;
    assert_eq!(
        missing.assess(&request(), Duration::from_secs(2)),
        Err(Failure::Authentication)
    );
    assert!(missing.transport.posts.is_empty());
    let mut expired = client(Ok(Response {
        status: 200,
        body: answer(),
    }));
    assert_eq!(
        expired.assess(&request(), Duration::ZERO),
        Err(Failure::Timeout)
    );
    assert!(expired.transport.posts.is_empty());
}

// @kotowari[REQ-advisor-013, REQ-advisor-014, EX-advisor-026]
#[test]
fn duplicate_keys_unknown_answer_kinds_invalid_distributions_and_large_responses_are_rejected() {
    let text = String::from_utf8(answer()).unwrap();
    let invalid = [
        text.replace(
            "\"major_destructive\":0.95",
            "\"major_destructive\":0.95,\"major_destructive\":0.95",
        ),
        text.replace("\"choice\"", "\"noul\""),
        text.replace("\"major_destructive\":0.95", "\"major_destructive\":1.95"),
        "{}".into(),
        "x".repeat(65537),
    ];
    for body in invalid {
        let mut client = client(Ok(Response {
            status: 200,
            body: body.into_bytes(),
        }));
        assert_eq!(
            client.assess(&request(), Duration::from_secs(2)),
            Err(Failure::InvalidResponse)
        );
        assert_eq!(client.transport.posts.len(), 1);
    }
}

fn http_fixture(
    status: u16,
    body: Vec<u8>,
    delay: Duration,
) -> (String, std::thread::JoinHandle<Vec<Vec<u8>>>) {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_millis(300);
        let mut requests = Vec::new();
        while Instant::now() < end {
            let Ok((mut stream, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            };
            stream
                .set_read_timeout(Some(Duration::from_millis(100)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buffer[..n]),
                }
                if let Some(header_end) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let length = headers
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|s| s.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= header_end + 4 + length {
                        break;
                    }
                }
            }
            requests.push(request);
            std::thread::sleep(delay);
            let response = format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nLocation: http://{address}/redirected\r\nConnection: close\r\n\r\n", body.len());
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&body);
        }
        requests
    });
    (format!("http://{address}/v1/systemone"), handle)
}

// @kotowari[REQ-advisor-014, EX-advisor-027, EX-advisor-028]
#[test]
fn actual_http_transport_posts_once_and_never_follows_a_redirect_or_retries_a_rate_limit() {
    for status in [200, 302, 429] {
        let (endpoint, server) = http_fixture(status, answer(), Duration::ZERO);
        let mut client = Client {
            transport: HttpTransport {
                endpoint,
                https_only: false,
            },
            key: Some("fixture-not-a-real-credential".into()),
            sent: false,
        };
        let permit = request();
        let result = client.assess(&permit, Duration::from_secs(1));
        if status == 200 {
            assert!(result.is_ok());
        } else {
            assert_eq!(result, Err(Failure::ServiceResponse));
        }
        let posts = server.join().unwrap();
        assert_eq!(posts.len(), 1);
        assert!(posts[0].starts_with(b"POST /v1/systemone HTTP/1.1\r\n"));
        assert!(posts[0].ends_with(permit.body()));
    }
}

// @kotowari[REQ-advisor-014, EX-advisor-028]
#[test]
fn actual_http_transport_bounds_waiting_and_response_bytes() {
    let (endpoint, server) = http_fixture(200, answer(), Duration::from_millis(150));
    let mut client = Client {
        transport: HttpTransport {
            endpoint,
            https_only: false,
        },
        key: Some("fixture-not-a-real-credential".into()),
        sent: false,
    };
    let permit = request();
    let start = Instant::now();
    assert_eq!(
        client.assess(&permit, Duration::from_millis(30)),
        Err(Failure::Timeout)
    );
    assert!(start.elapsed() < Duration::from_millis(140));
    assert_eq!(server.join().unwrap().len(), 1);
    let (endpoint, server) = http_fixture(200, vec![b'x'; 65537], Duration::ZERO);
    let mut client = Client {
        transport: HttpTransport {
            endpoint,
            https_only: false,
        },
        key: Some("fixture-not-a-real-credential".into()),
        sent: false,
    };
    assert_eq!(
        client.assess(&request(), Duration::from_secs(1)),
        Err(Failure::InvalidResponse)
    );
    assert_eq!(server.join().unwrap().len(), 1);
}

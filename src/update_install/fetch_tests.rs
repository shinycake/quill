use super::*;
use std::io::Write;
use std::net::TcpListener;

const SIZE: u64 = 4096;

fn package() -> Vec<u8> {
    (0..SIZE).map(|n| (n % 251) as u8).collect()
}

fn folder() -> PathBuf {
    crate::scratch::unique_dir("quill-update-fetch")
}

fn serve(answers: Vec<Vec<u8>>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let url = format!("http://{}/package", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for answer in answers {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                assert_eq!(stream.read(&mut byte).unwrap(), 1);
                request.push(byte[0]);
            }
            requests.push(String::from_utf8(request).unwrap().to_ascii_lowercase());
            stream.write_all(&answer).unwrap();
        }
        requests
    });
    (url, server)
}

fn http(status: &str, headers: &str, length: usize, body: &[u8]) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {length}\r\nConnection: close\r\n\r\n"
    );
    [head.as_bytes(), body].concat()
}

fn whole() -> Vec<u8> {
    http("200 OK", "", SIZE as usize, &package())
}

fn rest_from(start: usize, claimed_start: usize) -> Vec<u8> {
    let body = package().split_off(start);
    http(
        "206 Partial Content",
        &format!(
            "Content-Range: bytes {claimed_start}-{}/{SIZE}\r\n",
            SIZE - 1
        ),
        body.len(),
        &body,
    )
}

fn run(url: &str, partial: &Path) -> (Result<(), &'static str>, u64) {
    let progress = DownloadProgress::default();
    let result = fetch(&agent(false, 0), url, partial, SIZE, &progress);
    (result, progress.bytes())
}

#[test]
fn a_dropped_download_goes_on_from_where_it_stopped() {
    let partial = folder().join("package");
    let cut = 1500;
    let (url, server) = serve(vec![
        http("200 OK", "", SIZE as usize, &package()[..cut]),
        rest_from(cut, cut),
    ]);
    assert_eq!(run(&url, &partial), (Err(INTERRUPTED), cut as u64));
    assert_eq!(fs::read(&partial).unwrap(), &package()[..cut]);

    assert_eq!(run(&url, &partial), (Ok(()), SIZE));
    assert_eq!(fs::read(&partial).unwrap(), package());
    let requests = server.join().unwrap();
    assert!(!requests[0].contains("range:"));
    assert!(requests[1].contains(&format!("range: bytes={cut}-\r\n")));
}

#[test]
fn a_server_that_ignores_the_range_replaces_the_file() {
    let partial = folder().join("package");
    fs::write(&partial, b"old bytes").unwrap();
    let (url, server) = serve(vec![whole()]);
    assert_eq!(run(&url, &partial), (Ok(()), SIZE));
    assert_eq!(fs::read(&partial).unwrap(), package());
    assert!(server.join().unwrap()[0].contains("range: bytes=9-"));
}

#[test]
fn a_refused_range_asks_for_the_whole_package() {
    for refusal in [
        http("416 Range Not Satisfiable", "", 0, b""),
        rest_from(100, 100),
    ] {
        let partial = folder().join("package");
        fs::write(&partial, vec![1u8; 200]).unwrap();
        let (url, server) = serve(vec![refusal, whole()]);
        assert_eq!(run(&url, &partial), (Ok(()), SIZE));
        assert_eq!(fs::read(&partial).unwrap(), package());
        let requests = server.join().unwrap();
        assert!(requests[0].contains("range: bytes=200-"));
        assert!(!requests[1].contains("range:"));
    }
}

#[test]
fn a_finished_file_needs_no_request() {
    let partial = folder().join("package");
    fs::write(&partial, package()).unwrap();
    assert_eq!(run("http://127.0.0.1:1/never", &partial), (Ok(()), SIZE));
}

#[test]
fn a_file_longer_than_the_package_starts_over() {
    let partial = folder().join("package");
    fs::write(&partial, vec![1u8; SIZE as usize + 1]).unwrap();
    let (url, server) = serve(vec![whole()]);
    assert_eq!(run(&url, &partial), (Ok(()), SIZE));
    assert_eq!(fs::read(&partial).unwrap(), package());
    assert!(!server.join().unwrap()[0].contains("range:"));
}

#[test]
fn a_body_longer_than_the_package_is_thrown_away() {
    let partial = folder().join("package");
    let mut body = package();
    body.push(0);
    let (url, server) = serve(vec![http("200 OK", "", body.len(), &body)]);
    assert_eq!(run(&url, &partial).0, Err(MISMATCH));
    assert!(!partial.exists());
    server.join().unwrap();
}

#[cfg(unix)]
#[test]
fn a_link_in_its_place_is_not_written_through() {
    let dir = folder();
    let elsewhere = dir.join("elsewhere");
    fs::write(&elsewhere, b"precious").unwrap();
    let partial = dir.join("package");
    std::os::unix::fs::symlink(&elsewhere, &partial).unwrap();
    let (url, server) = serve(vec![whole()]);
    assert_eq!(run(&url, &partial), (Ok(()), SIZE));
    assert_eq!(fs::read(&elsewhere).unwrap(), b"precious");
    assert_eq!(fs::read(&partial).unwrap(), package());
    assert!(!fs::symlink_metadata(&partial).unwrap().is_symlink());
    server.join().unwrap();
}

#[test]
fn only_a_stall_ends_an_attempt() {
    let config = agent(true, 3);
    let config = config.config();
    assert_eq!(config.timeout_per_read(), Some(STALL_LIMIT));
    assert_eq!(config.timeouts().global, None);
}

#[test]
fn a_new_release_drops_the_old_partial_and_keeps_its_own() {
    let dir = folder();
    let keep = partial_path(&dir, &"A".repeat(64));
    let old = partial_path(&dir, &"b".repeat(64));
    fs::write(&keep, b"new").unwrap();
    fs::write(&old, b"old").unwrap();
    swap::clear_temp_entries(&dir);
    clear_other_partials(&dir, &keep);
    assert!(keep.ends_with(format!("{PARTIAL_PREFIX}{}.archive", "a".repeat(64))));
    assert!(keep.exists());
    assert!(!old.exists());
}

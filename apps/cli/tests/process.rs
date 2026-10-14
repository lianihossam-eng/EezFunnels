use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;

fn run(dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eez"))
        .env("EEZ_CONFIG_DIR", dir)
        .current_dir(dir)
        .args(args)
        .output()
        .expect("CLI process")
}

fn request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
    }
    let header = String::from_utf8(bytes.clone()).unwrap();
    let length = header
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    bytes.extend(body);
    String::from_utf8(bytes).unwrap()
}

fn server(replies: Vec<Option<(u16, String)>>) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        replies.into_iter().map(|reply| {
            let (mut stream, _) = listener.accept().unwrap();
            let request = request(&mut stream);
            if let Some((status, body)) = reply {
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            request
        }).collect()
    });
    (endpoint, handle)
}

fn created() -> String {
    r#"{"command_id":"funnel.create","funnel":{"id":"funnel-a","name":"Offer","slug":"offer-a","published_revision_id":null},"initial_revision":{"id":"revision-a"},"future_field":true}"#.into()
}
fn key(request: &str) -> &str {
    request
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("idempotency-key")
                .then(|| value.trim())
        })
        .unwrap()
}
fn retry_id(output: &Output) -> String {
    let text = String::from_utf8(output.stderr.clone()).unwrap();
    text.split("eez funnels retry ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .into()
}

#[test]
fn config_validation_and_help_are_process_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(dir.path(), &["--help"]).status.success());
    assert!(!run(dir.path(), &["funnels", "list"]).status.success());
    for endpoint in [
        "http://example.com",
        "https://user:password@example.com",
        "https://example.com/path",
        "https://example.com/?x=1",
        "https://example.com/#x",
        "file:///tmp/a",
        "https://example.com/\\evil",
    ] {
        assert!(
            !run(dir.path(), &["config", "set", "endpoint", endpoint])
                .status
                .success(),
            "{endpoint}"
        );
    }
    assert!(run(
        dir.path(),
        &["config", "set", "endpoint", "https://example.com"]
    )
    .status
    .success());
    for args in [
        vec!["funnels", "create", ""],
        vec!["funnels", "create", "a\n"],
        vec!["funnels", "create", "Offer", "--blueprint", "wrong"],
        vec!["funnels", "publish", "../escape"],
        vec!["funnels", "retry", "../escape"],
    ] {
        assert!(!run(dir.path(), &args).status.success());
    }
}

#[test]
fn idna_expansion_is_rejected_without_replacing_the_existing_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let list = r#"{"command_id":"funnel.list","items":[]}"#.into();
    let (endpoint, handle) = server(vec![Some((200, list))]);
    assert!(run(dir.path(), &["config", "set", "endpoint", &endpoint])
        .status
        .success());

    let oversized_after_idna = format!("https://{}com", "é.".repeat(500));
    let rejected = run(
        dir.path(),
        &["config", "set", "endpoint", &oversized_after_idna],
    );
    assert!(!rejected.status.success());
    assert!(run(dir.path(), &["funnels", "list"]).status.success());
    assert!(handle.join().unwrap()[0].starts_with("GET /api/funnels HTTP/1.1"));
}

#[test]
fn lost_create_response_replays_exact_ticket_at_original_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let (endpoint, handle) = server(vec![None, Some((201, created())), Some((201, created()))]);
    assert!(run(dir.path(), &["config", "set", "endpoint", &endpoint])
        .status
        .success());
    let lost = run(dir.path(), &["funnels", "create", "Offer"]);
    assert!(!lost.status.success());
    assert!(String::from_utf8_lossy(&lost.stderr).contains("unknown"));
    let operation = retry_id(&lost);
    assert!(run(
        dir.path(),
        &["config", "set", "endpoint", "http://127.0.0.1:1"]
    )
    .status
    .success());
    assert!(run(dir.path(), &["funnels", "retry", &operation])
        .status
        .success());
    assert!(run(dir.path(), &["config", "set", "endpoint", &endpoint])
        .status
        .success());
    assert!(run(dir.path(), &["funnels", "create", "Offer"])
        .status
        .success());
    let requests = handle.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].starts_with("POST /api/funnels HTTP/1.1"));
    assert_eq!(key(&requests[0]), key(&requests[1]));
    assert_ne!(key(&requests[0]), key(&requests[2]));
    assert_eq!(
        requests[0].split("\r\n\r\n").nth(1),
        Some(r#"{"name":"Offer","blueprint":"tripwire"}"#)
    );
    assert_eq!(requests[0], requests[1]);
}

#[test]
fn publish_retry_preserves_cas_and_does_not_refresh_reads() {
    let dir = tempfile::tempdir().unwrap();
    let list = r#"{"command_id":"funnel.list","items":[{"id":"funnel-a","name":"Offer","slug":"offer-a","published_revision_id":"revision-old"}]}"#.into();
    let editor = r#"{"command_id":"funnel.editor.read","funnel":{"id":"funnel-a"},"base_revision_id":"revision-new","preview_html":"<html>additive</html>"}"#.into();
    let published = r#"{"command_id":"funnel.publish","funnel_id":"funnel-a","published_revision_id":"revision-new"}"#.into();
    let (endpoint, handle) = server(vec![
        Some((200, list)),
        Some((200, editor)),
        None,
        Some((200, published)),
    ]);
    assert!(run(dir.path(), &["config", "set", "endpoint", &endpoint])
        .status
        .success());
    let lost = run(dir.path(), &["funnels", "publish", "funnel-a"]);
    assert!(!lost.status.success());
    assert!(run(dir.path(), &["funnels", "retry", &retry_id(&lost)])
        .status
        .success());
    let requests = handle.join().unwrap();
    assert!(requests[0].starts_with("GET /api/funnels HTTP/1.1"));
    assert!(requests[1].starts_with("GET /api/funnels/funnel-a HTTP/1.1"));
    assert!(requests[2].starts_with("POST /api/funnels/funnel-a/publish HTTP/1.1"));
    assert_eq!(requests[2], requests[3]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(requests[2].split("\r\n\r\n").nth(1).unwrap())
            .unwrap(),
        serde_json::json!({"revision_id":"revision-new","expected_published_revision_id":"revision-old"})
    );
}

#[test]
fn publish_accepts_a_maximum_size_editor_preview_response() {
    let dir = tempfile::tempdir().unwrap();
    let list = r#"{"command_id":"funnel.list","items":[{"id":"funnel-a","name":"Offer","slug":"offer-a","published_revision_id":null}]}"#.into();
    let editor = serde_json::json!({
        "command_id":"funnel.editor.read",
        "funnel":{"id":"funnel-a"},
        "base_revision_id":"revision-new",
        "preview_html":"&amp;".repeat(1024 * 1024)
    })
    .to_string();
    assert!(editor.len() > 2 * 1024 * 1024);
    let published = r#"{"command_id":"funnel.publish","funnel_id":"funnel-a","published_revision_id":"revision-new"}"#.into();
    let (endpoint, handle) = server(vec![
        Some((200, list)),
        Some((200, editor)),
        Some((200, published)),
    ]);
    assert!(run(dir.path(), &["config", "set", "endpoint", &endpoint])
        .status
        .success());
    assert!(run(dir.path(), &["funnels", "publish", "funnel-a"])
        .status
        .success());
    let requests = handle.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[2].starts_with("POST /api/funnels/funnel-a/publish HTTP/1.1"));
}

#[test]
fn errors_are_bounded_and_untrusted_html_is_not_printed() {
    for (status, body) in [
        (
            409,
            r#"{"code":"conflict/publish_stale_revision","message":"<script>bad</script>"}"#.into(),
        ),
        (302, "<html>redirect</html>".into()),
        (200, "x".repeat(2 * 1024 * 1024 + 1)),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (endpoint, handle) = server(vec![Some((status, body))]);
        assert!(run(dir.path(), &["config", "set", "endpoint", &endpoint])
            .status
            .success());
        let result = run(dir.path(), &["funnels", "list"]);
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("<"));
        handle.join().unwrap();
    }
}

#[cfg(unix)]
#[test]
fn deploy_forwards_arguments_cwd_and_exit_without_cargo() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let wrangler = dir.path().join("wrangler");
    std::fs::write(
        &wrangler,
        "#!/bin/sh\npwd > cwd\nprintf '%s\\n' \"$@\" > args\nexit 7\n",
    )
    .unwrap();
    std::fs::set_permissions(&wrangler, std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_eez"))
        .args(["deploy", "--env", "a b;$()"])
        .env("PATH", dir.path())
        .env("CARGO", "/missing-cargo")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("args")).unwrap(),
        "deploy\n--env\na b;$()\n"
    );
    let expected_cwd = std::fs::canonicalize(dir.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("cwd"))
            .unwrap()
            .trim(),
        expected_cwd.to_str().unwrap()
    );
    std::fs::remove_file(wrangler).unwrap();
    let missing = Command::new(env!("CARGO_BIN_EXE_eez"))
        .arg("deploy")
        .env("PATH", dir.path())
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("Wrangler"));
}

#[test]
fn malformed_and_expired_tickets_send_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let operations = dir.path().join("operations");
    std::fs::create_dir(&operations).unwrap();
    let id = "1b11d9a7-bbd4-4fd2-9d67-74991870734c";
    let path = operations.join(format!("{id}.json"));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let ticket = serde_json::json!({"version":1,"operation_id":id,"created_at":now,"endpoint":"http://127.0.0.1:1","path":"/api/funnels","body":r#"{"name":"Offer","blueprint":"tripwire"}"#});
    for (field, value) in [
        ("version", serde_json::json!(2)),
        ("created_at", serde_json::json!(now - 23 * 3600)),
        ("created_at", serde_json::json!(now + 3600)),
        ("path", serde_json::json!("/api/funnels/../secret")),
        ("body", serde_json::json!(r#"{"name":"Offer"}"#)),
        ("endpoint", serde_json::json!("http://example.com")),
        (
            "operation_id",
            serde_json::json!("8609d54f-6111-49f3-a5c8-dbd3cfb11026"),
        ),
    ] {
        let mut invalid = ticket.clone();
        let expired = field == "created_at" && value == serde_json::json!(now - 23 * 3600);
        invalid[field] = value;
        std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        let failed = run(dir.path(), &["funnels", "retry", id]);
        assert!(!failed.status.success());
        assert!(
            !String::from_utf8_lossy(&failed.stderr).contains("Worker connection"),
            "{field}"
        );
        if expired {
            assert!(!path.exists(), "expired ticket should be removed");
        }
    }
    std::fs::write(path, "x".repeat(16385)).unwrap();
    assert!(!run(dir.path(), &["funnels", "retry", id]).status.success());
}

#[test]
fn already_published_head_requires_no_post_and_stale_cas_is_visible() {
    for already in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let list = r#"{"command_id":"funnel.list","items":[{"id":"funnel-a","name":"Offer","slug":"offer-a","published_revision_id":"revision-old"}]}"#.into();
        let revision = if already {
            "revision-old"
        } else {
            "revision-new"
        };
        let editor = format!(
            r#"{{"command_id":"funnel.editor.read","funnel":{{"id":"funnel-a"}},"base_revision_id":"{revision}"}}"#
        );
        let mut replies = vec![Some((200, list)), Some((200, editor))];
        if !already {
            replies.push(Some((
                409,
                r#"{"code":"conflict/publish_stale_revision"}"#.into(),
            )));
        }
        let (endpoint, handle) = server(replies);
        assert!(run(dir.path(), &["config", "set", "endpoint", &endpoint])
            .status
            .success());
        let result = run(dir.path(), &["funnels", "publish", "funnel-a"]);
        assert_eq!(result.status.success(), already);
        if already {
            assert!(String::from_utf8_lossy(&result.stdout).contains("already_published"));
            assert!(!dir.path().join("operations").exists());
        } else {
            assert!(
                String::from_utf8_lossy(&result.stderr).contains("conflict/publish_stale_revision")
            );
        }
        assert_eq!(handle.join().unwrap().len(), if already { 2 } else { 3 });
    }
}

#[test]
fn additive_fields_work_but_missing_or_invalid_required_fields_fail() {
    for body in [
        r#"{"command_id":"funnel.list","items":[],"new":true}"#,
        r#"{"command_id":"funnel.list","items":[{"id":"funnel-a","name":"Offer","slug":"offer-a"}]}"#,
        r#"{"command_id":"funnel.list","items":[{"id":"../bad","name":"Offer","slug":"offer-a","published_revision_id":null}]}"#,
        r#"{"command_id":"wrong","items":[]}"#,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (endpoint, handle) = server(vec![Some((200, body.into()))]);
        assert!(run(dir.path(), &["config", "set", "endpoint", &endpoint])
            .status
            .success());
        let result = run(dir.path(), &["funnels", "list"]);
        assert_eq!(result.status.success(), body.contains("\"new\""));
        handle.join().unwrap();
    }
}

#[test]
fn mutation_is_not_sent_without_a_saved_ticket() {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    assert!(run(dir.path(), &["config", "set", "endpoint", &endpoint])
        .status
        .success());
    std::fs::write(dir.path().join("operations"), "not a directory").unwrap();
    let failed = run(dir.path(), &["funnels", "create", "Offer"]);
    assert!(!failed.status.success());
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("Worker connection"));
    assert!(listener.accept().is_err());
}

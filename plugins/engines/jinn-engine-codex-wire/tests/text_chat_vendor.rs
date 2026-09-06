//! Explicit vendor proof; a missing dedicated authenticated home is a skip.
use jinn_engine::EventKind;
use jinn_engine_codex_wire::app_server::{argv, AppServer};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn actual_astra_text_precedes_completion_and_stdio_eof_reaps() {
    let Ok(home) = std::env::var("JINN_TEXT_CHAT_PROBE_HOME") else {
        eprintln!("SKIP vendor text Chat: JINN_TEXT_CHAT_PROBE_HOME is not configured");
        return;
    };
    let home = std::path::PathBuf::from(home);
    let cwd = home.join("workspace");
    let mut child = Command::new(std::env::var("JINN_TEXT_CHAT_CODEX").expect("Codex executable"))
        .args(argv())
        .env_clear()
        .env("HOME", &home)
        .env("CODEX_HOME", home.join("codex"))
        .env("PATH", std::env::var("PATH").expect("PATH"))
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = child.stdout.take().expect("stdout");
    let (sender, receiver) = mpsc::sync_channel(16);
    let reader = std::thread::spawn(move || {
        let mut buf = [0_u8; 8192];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 || sender.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut wire = AppServer::new(
        "Write three paragraphs about a paper boat crossing a pond, around 150 words.".into(),
        "gpt-6-astra".into(),
        "high".into(),
        cwd.to_string_lossy().into_owned(),
    );
    let start = Instant::now();
    let mut deltas = 0;
    while !wire.terminal() && start.elapsed() < Duration::from_secs(100) {
        stdin.write_all(&wire.take_outgoing()).expect("stdin write");
        if let Ok(bytes) = receiver.recv_timeout(Duration::from_millis(100)) {
            for event in wire.feed(&bytes) {
                if matches!(event.kind, EventKind::Delta { .. }) {
                    deltas += 1;
                    if deltas <= 2 {
                        eprintln!("actual partial {deltas} at {:?}", start.elapsed());
                    }
                }
            }
        }
    }
    eprintln!(
        "terminal={} partials={deltas} elapsed={:?} errors={:?}",
        wire.terminal(),
        start.elapsed(),
        wire.errors()
    );
    drop(stdin);
    let until = Instant::now() + Duration::from_secs(5);
    let mut exited = false;
    while Instant::now() < until {
        if child.try_wait().expect("wait").is_some() {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if !exited {
        let _ = child.kill();
    }
    let status = child.wait().expect("reap");
    drop(receiver);
    reader.join().expect("reader");
    eprintln!("stdio EOF reaped={exited}, process={status}");
    assert!(
        wire.terminal() && wire.errors().is_empty(),
        "{:?}",
        wire.errors()
    );
    assert!(deltas >= 2 && exited && status.success());
}

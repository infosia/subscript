//! Compiler contract §260: keep a changed downstream Cargo.lock.
//! Cost: eight temporary git repositories and eight shell runs; no downstream build.
//! Each case runs git init, add, and commit. TERM ends a five-second stub wait early.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const COMMITTED: &str = "# committed lockfile\n";
const CHANGED: &str = "# developer lockfile\n";

struct Downstream {
    dir: PathBuf,
}

impl Downstream {
    fn new(status: i32) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "subscript-downstream-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).unwrap();
        let fixture = Self { dir };
        fixture.git(&["init", "--quiet"]);
        std::fs::write(fixture.dir.join("Cargo.lock"), COMMITTED).unwrap();
        std::fs::create_dir(fixture.dir.join("tools")).unwrap();
        let gate = fixture.dir.join("tools/gate.sh");
        std::fs::write(
            &gate,
            format!(
                "#!/bin/sh\nprintf '# stub change\\n' >> Cargo.lock\nprintf 'stub ran\\n'\nexit {status}\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&gate, std::fs::Permissions::from_mode(0o755)).unwrap();
        fixture.git(&["add", "Cargo.lock", "tools/gate.sh"]);
        fixture.git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ]);
        fixture
    }

    fn git(&self, args: &[&str]) -> Output {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .output()
            .expect("git must be installed for the downstream script tests");
        assert!(output.status.success(), "git {args:?}: {output:?}");
        output
    }

    fn run(&self) -> Output {
        Command::new("sh")
            .arg(root().join("tools/downstream.sh"))
            .current_dir(root())
            .env("SUBSCRIPT_DOWNSTREAM_DIR", &self.dir)
            .output()
            .expect("sh must be installed for the downstream script tests")
    }

    fn assert_changed_is_kept(&self, staged: bool) {
        let output = self.run();
        assert_eq!(
            std::fs::read_to_string(self.dir.join("Cargo.lock")).unwrap(),
            CHANGED,
            "the script must keep the developer change"
        );
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(diagnostic.lines().count(), 1, "{diagnostic}");
        assert!(diagnostic.contains("Cargo.lock"), "{diagnostic}");
        assert!(!diagnostic.contains("stub ran"));
        assert!(!self.dir.join(".cargo").exists());
        let index = self.git(&["show", ":Cargo.lock"]);
        assert_eq!(
            index.stdout,
            if staged { CHANGED } else { COMMITTED }.as_bytes()
        );
    }
}

impl Drop for Downstream {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

#[test]
fn unstaged_change_is_kept() {
    let fixture = Downstream::new(0);
    std::fs::write(fixture.dir.join("Cargo.lock"), CHANGED).unwrap();
    fixture.assert_changed_is_kept(false);
}

#[test]
fn staged_change_is_kept() {
    let fixture = Downstream::new(0);
    std::fs::write(fixture.dir.join("Cargo.lock"), CHANGED).unwrap();
    fixture.git(&["add", "Cargo.lock"]);
    fixture.assert_changed_is_kept(true);
}

fn clean_control(status: i32) {
    let fixture = Downstream::new(status);
    let output = fixture.run();
    assert_eq!(output.status.code(), Some(status), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("stub ran\n"));
    assert_eq!(
        std::fs::read_to_string(fixture.dir.join("Cargo.lock")).unwrap(),
        COMMITTED
    );
    assert!(!fixture.dir.join(".cargo").exists());
    assert!(fixture.git(&["status", "--porcelain"]).stdout.is_empty());
}

#[test]
fn clean_success_restores_lockfile() {
    clean_control(0);
}

#[test]
fn clean_failure_restores_lockfile() {
    clean_control(7);
}

fn assert_setup_rejected(fixture: &Downstream, cause: &str, lockfile: Option<&str>) {
    let gate = std::fs::read(fixture.dir.join("tools/gate.sh")).unwrap();
    let output = fixture.run();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(diagnostic.lines().count(), 1, "{diagnostic}");
    assert!(diagnostic.contains(cause), "{diagnostic}");
    assert_eq!(
        std::fs::read_to_string(fixture.dir.join("Cargo.lock"))
            .ok()
            .as_deref(),
        lockfile
    );
    assert_eq!(
        std::fs::read(fixture.dir.join("tools/gate.sh")).unwrap(),
        gate
    );
    assert!(!fixture.dir.join(".cargo").exists());
}

#[test]
fn untracked_lockfile_is_rejected() {
    let fixture = Downstream::new(0);
    fixture.git(&["rm", "--cached", "--quiet", "Cargo.lock"]);
    assert_setup_rejected(&fixture, "git does not track Cargo.lock", Some(COMMITTED));
}

#[test]
fn absent_untracked_lockfile_is_rejected() {
    let fixture = Downstream::new(0);
    fixture.git(&["rm", "--quiet", "Cargo.lock"]);
    assert_setup_rejected(&fixture, "git does not track Cargo.lock", None);
}

#[test]
fn non_checkout_is_rejected() {
    let fixture = Downstream::new(0);
    std::fs::remove_dir_all(fixture.dir.join(".git")).unwrap();
    assert_setup_rejected(&fixture, "not a git checkout", Some(COMMITTED));
}

#[test]
fn term_restores_lockfile_and_returns_143() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let fixture = Downstream::new(0);
    std::fs::write(
        fixture.dir.join("tools/gate.sh"),
        "#!/bin/sh\nprintf '# stub change\\n' >> Cargo.lock\nsleep 5 &\nwait_pid=$!\nprintf '%s\\n' \"$wait_pid\" > gate.pid\nwait \"$wait_pid\"\n",
    ).unwrap();
    let mut script = Command::new("sh")
        .arg(root().join("tools/downstream.sh"))
        .env("SUBSCRIPT_DOWNSTREAM_DIR", &fixture.dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let pid_file = fixture.dir.join("gate.pid");
    let wait_pid = loop {
        if let Ok(pid) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid.trim().parse::<u32>() {
                break pid;
            }
        }
        if start.elapsed() > Duration::from_secs(2) {
            let _ = script.kill();
            let _ = script.wait();
            panic!("gate stub did not start");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let signal = Command::new("sh")
        .args(["-c", "kill -TERM \"$1\"", "sh", &script.id().to_string()])
        .status()
        .unwrap();
    let stop_wait = Command::new("sh")
        .args(["-c", "kill -TERM \"$1\"", "sh", &wait_pid.to_string()])
        .status()
        .unwrap();
    let output = script.wait_with_output().unwrap();
    assert!(signal.success());
    assert!(stop_wait.success());
    assert_eq!(output.status.code(), Some(143), "{output:?}");
    assert_eq!(
        std::fs::read_to_string(fixture.dir.join("Cargo.lock")).unwrap(),
        COMMITTED
    );
    assert!(!fixture.dir.join(".cargo").exists());
    assert!(start.elapsed() < Duration::from_secs(3));
}

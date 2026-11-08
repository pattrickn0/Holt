//! The child process: threads, the shared handles and the shutdown of every tab.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;

use crate::agents::SpawnSpec;
use crate::{process, Core};

use super::*;

// ---------- Process ----------

/// What the reader threads hand to the interface.
#[derive(Debug)]
pub enum ChatMsg {
    Event(ChatEvent),
    /// The last line the process wrote to stderr (kept for the error shown when it dies).
    Stderr(String),
    /// stdout closed: the process ended with this code.
    Exited(Option<i32>),
}

/// A running chat process. Dropping it kills the process.
pub struct ChatProcess {
    child: Arc<Mutex<Child>>,
    registry_id: u64,
    core: Core,
    stdin: Sender<String>,
    rx: Receiver<ChatMsg>,
    /// The state of the conversation with this process. Whoever writes to `stdin` while holding it keeps the order the
    /// protocol decided.
    proto: Arc<Mutex<Box<dyn Protocol>>>,
}

impl Core {
    /// Starts `spec` (a launch from [`Core::spawn_spec_chat`]) with its pipes. `wake` is called whenever
    /// something arrives, to make the interface look at [`ChatProcess::try_recv`].
    pub fn spawn_chat(&self, spec: &SpawnSpec, wake: Arc<dyn Fn() + Send + Sync>) -> std::io::Result<ChatProcess> {
        let backend = for_kind(&spec.kind).ok_or_else(|| std::io::Error::other(format!("{} has no chat view", spec.kind)))?;
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args).current_dir(&spec.cwd).env_clear().envs(&spec.env);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        process::platform_flags(&mut cmd);
        let mut child = cmd.spawn()?;
        process::die_with_app(&child);
        let stdout = child.stdout.take().ok_or_else(|| std::io::Error::other("no stdout"))?;
        let stderr = child.stderr.take();
        let mut pipe = child.stdin.take().ok_or_else(|| std::io::Error::other("no stdin"))?;
        let child = Arc::new(Mutex::new(child));
        let registry_id = self.inner.processes.add(child.clone());

        let (tx, rx) = channel();
        let (stdin, lines) = channel::<String>();
        let proto: Arc<Mutex<Box<dyn Protocol>>> = Arc::new(Mutex::new(backend.protocol(spec)));
        for line in proto.lock().open() {
            let _ = stdin.send(line);
        }
        // Writer: a stalled process must never block the interface.
        std::thread::Builder::new().name("chat-write".into()).spawn(move || {
            for line in lines {
                if pipe.write_all(line.as_bytes()).and_then(|()| pipe.write_all(b"\n")).and_then(|()| pipe.flush()).is_err() {
                    break;
                }
            }
        })?;
        if let Some(mut err) = stderr {
            let (tx, wake) = (tx.clone(), wake.clone());
            std::thread::Builder::new().name("chat-stderr".into()).spawn(move || {
                let mut text = String::new();
                let _ = err.read_to_string(&mut text);
                if let Some(last) = text.lines().rev().find(|l| !l.trim().is_empty()) {
                    let _ = tx.send(ChatMsg::Stderr(last.trim().to_owned()));
                    wake();
                }
            })?;
        }
        let (reply, proto_for_reader, child_for_reader) = (stdin.clone(), proto.clone(), child.clone());
        std::thread::Builder::new().name("chat-read".into()).spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let mut any = false;
                {
                    let mut proto = proto_for_reader.lock();
                    let parsed = proto.parse(&line);
                    for l in parsed.reply {
                        let _ = reply.send(l);
                    }
                    for ev in parsed.events {
                        any = true;
                        let _ = tx.send(ChatMsg::Event(ev));
                    }
                }
                if any {
                    wake();
                }
            }
            let code = child_for_reader.lock().wait().ok().and_then(|s| s.code());
            let _ = tx.send(ChatMsg::Exited(code));
            wake();
        })?;
        Ok(ChatProcess { child, registry_id, core: self.clone(), stdin, rx, proto })
    }
}

impl ChatProcess {
    pub fn try_recv(&self) -> Option<ChatMsg> {
        self.rx.try_recv().ok()
    }

    fn send_lines(&self, lines: Vec<String>) {
        for line in lines {
            let _ = self.stdin.send(line);
        }
    }

    pub fn send_user(&self, text: &str) {
        let mut proto = self.proto.lock();
        self.send_lines(proto.user_message(text));
    }

    pub fn answer_permission(&self, request_id: &str, allow: bool, input: &Value, deny_message: &str, rules: &[Value]) {
        let mut proto = self.proto.lock();
        self.send_lines(proto.answer_permission(request_id, allow, input, deny_message, rules));
    }

    pub fn interrupt(&self) {
        let mut proto = self.proto.lock();
        match proto.interrupt() {
            Interrupt::Lines(lines) => self.send_lines(lines),
            Interrupt::Signal => process::interrupt_process(&self.child.lock()),
        }
    }

    pub fn kill(&self) {
        process::kill_tree(&mut self.child.lock());
    }
}

impl Drop for ChatProcess {
    fn drop(&mut self) {
        self.kill();
        self.core.inner.processes.remove(self.registry_id);
    }
}

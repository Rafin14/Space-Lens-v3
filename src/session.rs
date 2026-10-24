use crate::scanner::{self, Progress, ScanError, Snapshot};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Empty,
    Loading,
    Ready,
    Cancelled,
    Error,
}
struct Request {
    generation: u64,
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    targeted: Option<(Arc<Snapshot>, PathBuf)>,
}
struct Pending {
    request: Option<Request>,
    closed: bool,
}
pub struct Session {
    pub root: PathBuf,
    pub current: PathBuf,
    pub history: Vec<PathBuf>,
    pub history_index: usize,
    pub snapshot: Option<Arc<Snapshot>>,
    pub state: State,
    pub message: String,
    pub progress: Progress,
    pub generation: u64,
    cancel: Arc<AtomicBool>,
    pending: Arc<(Mutex<Pending>, Condvar)>,
    latest: Arc<Mutex<(u64, Progress)>>,
    events: Receiver<(u64, Result<Snapshot, ScanError>, bool)>,
    worker: Option<JoinHandle<()>>,
}

impl Default for Session {
    fn default() -> Self {
        let pending = Arc::new((
            Mutex::new(Pending {
                request: None,
                closed: false,
            }),
            Condvar::new(),
        ));
        let latest = Arc::new(Mutex::new((0, Progress::default())));
        let (sender, events) = mpsc::channel();
        let work = Arc::clone(&pending);
        let progress = Arc::clone(&latest);
        let worker = thread::Builder::new()
            .name("SpaceLens scanner".into())
            .spawn(move || {
                let mut engine = crate::engine::Scanner::new(crate::engine::options());
                loop {
                    let request = {
                        let (lock, ready) = &*work;
                        let mut state = lock.lock().unwrap();
                        while state.request.is_none() && !state.closed {
                            state = ready.wait(state).unwrap();
                        }
                        if state.closed {
                            return;
                        }
                        state.request.take().unwrap()
                    };
                    if request.cancel.load(Ordering::Relaxed) {
                        continue;
                    }
                    let callback = |value| {
                        *progress.lock().unwrap() = (request.generation, value);
                        unsafe {
                            crate::qt::sl_wake();
                        }
                    };
                    let targeted = request.targeted.is_some();
                    let result = if let Some((previous, target)) = &request.targeted {
                        engine.refresh_after_delete(previous, target, &request.cancel)
                    } else {
                        engine.scan(&request.root, &request.cancel, Some(&callback))
                    };
                    if sender.send((request.generation, result, targeted)).is_err() {
                        return;
                    }
                    unsafe {
                        crate::qt::sl_wake();
                    }
                }
            })
            .expect("Could not start scanner");
        Self {
            root: PathBuf::new(),
            current: PathBuf::new(),
            history: vec![],
            history_index: 0,
            snapshot: None,
            state: State::Empty,
            message: "Choose a folder to see where its space goes.".into(),
            progress: Progress::default(),
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            pending,
            latest,
            events,
            worker: Some(worker),
        }
    }
}

impl Session {
    pub fn start(&mut self, root: &Path, refresh: bool) {
        let root = scanner::normalize(root);
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation += 1;
        self.progress = Progress::default();
        if !refresh || !scanner::same_path(&root, &self.root) {
            self.snapshot = None;
            self.root = root.clone();
            self.current = root.clone();
            self.history = vec![root.clone()];
            self.history_index = 0;
        }
        self.state = State::Loading;
        self.message = "Measuring files and folders…".into();
        let (lock, ready) = &*self.pending;
        lock.lock().unwrap().request = Some(Request {
            generation: self.generation,
            root,
            cancel: Arc::clone(&self.cancel),
            targeted: None,
        });
        ready.notify_one();
    }
    pub fn refresh_after_delete(&mut self, target: &Path) {
        let Some(snapshot) = self.snapshot.clone() else {
            self.start(&self.root.clone(), true);
            return;
        };
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation += 1;
        self.progress = Progress::default();
        self.state = State::Loading;
        self.message = "Updating the affected folder…".into();
        let (lock, ready) = &*self.pending;
        lock.lock().unwrap().request = Some(Request {
            generation: self.generation,
            root: self.root.clone(),
            cancel: self.cancel.clone(),
            targeted: Some((snapshot, target.to_path_buf())),
        });
        ready.notify_one();
    }
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let (generation, value) = *self.latest.lock().unwrap();
        if generation == self.generation
            && self.state == State::Loading
            && (value.files, value.folders, value.bytes)
                != (
                    self.progress.files,
                    self.progress.folders,
                    self.progress.bytes,
                )
        {
            self.progress = value;
            changed = true;
        }
        for _ in 0..32 {
            let Ok((generation, result, targeted)) = self.events.try_recv() else {
                break;
            };
            if generation != self.generation {
                continue;
            }
            changed = true;
            match result {
                Ok(snapshot) => {
                    while !snapshot.index.contains_key(&self.current) && self.current != self.root {
                        self.current = self.current.parent().unwrap_or(&self.root).to_path_buf();
                    }
                    let mut before = self.history[..=self.history_index]
                        .iter()
                        .filter(|p| snapshot.index.contains_key(*p))
                        .cloned()
                        .collect::<Vec<_>>();
                    let after = self.history[self.history_index + 1..]
                        .iter()
                        .filter(|p| snapshot.index.contains_key(*p))
                        .cloned()
                        .collect::<Vec<_>>();
                    if before.last() != Some(&self.current) {
                        before.push(self.current.clone());
                    }
                    self.history_index = before.len() - 1;
                    before.extend(after);
                    self.history = before;
                    self.snapshot = Some(Arc::new(snapshot));
                    self.state = State::Ready;
                    self.message = if targeted {
                        "Updated the affected folder. Other folders retain their previous measurements; Refresh rescans the root.".into()
                    } else {
                        String::new()
                    };
                }
                Err(ScanError::Cancelled) => {}
                Err(ScanError::Io(message)) => {
                    self.state = State::Error;
                    self.message = message;
                }
            }
        }
        changed
    }
    pub fn navigate(&mut self, path: &Path, history: bool) -> bool {
        let path = scanner::normalize(path);
        if self.state != State::Ready
            || !scanner::within(&path, &self.root)
            || !self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.index.contains_key(&path))
        {
            return false;
        }
        if path == self.current {
            return true;
        }
        self.current = path.clone();
        if history {
            self.history.truncate(self.history_index + 1);
            self.history.push(path);
            self.history_index = self.history.len() - 1;
        }
        true
    }
    pub fn back(&mut self) {
        if self.state == State::Ready && self.history_index > 0 {
            self.history_index -= 1;
            let path = self.history[self.history_index].clone();
            self.navigate(&path, false);
        }
    }
    pub fn forward(&mut self) {
        if self.state == State::Ready && self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            let path = self.history[self.history_index].clone();
            self.navigate(&path, false);
        }
    }
    pub fn up(&mut self) {
        if self.current != self.root {
            if let Some(parent) = self.current.parent().map(Path::to_path_buf) {
                self.navigate(&parent, true);
            }
        }
    }
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.generation += 1;
        self.pending.0.lock().unwrap().request = None;
        self.state = if self.snapshot.is_some() {
            State::Ready
        } else {
            State::Cancelled
        };
        self.message = if self.snapshot.is_some() {
            "Refresh cancelled. Showing the previous snapshot."
        } else {
            "Scan cancelled. Choose a folder or retry."
        }
        .into();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        {
            let mut state = self.pending.0.lock().unwrap();
            state.closed = true;
            state.request = None;
        }
        self.pending.1.notify_one();
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

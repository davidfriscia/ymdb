//! An in-process SSH server with a read-only SFTP subsystem, for tests.
//!
//! It listens on `127.0.0.1` with a fixture host key, accepts only the given
//! client public keys and serves a local directory. Names starting with
//! `forbidden` are refused with "permission denied". Nothing can be written.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use russh::keys::{PrivateKey, PublicKey};
use russh::server::{self, Auth, ChannelOpenHandle, Msg, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// A fixture file of `src/remote/testdata` (test keys only).
pub fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("remote")
        .join("testdata")
        .join(name)
}

pub fn private_key(name: &str) -> PrivateKey {
    PrivateKey::from_openssh(fs::read_to_string(testdata(name)).expect("read key"))
        .expect("private key")
}

pub fn public_key(name: &str) -> PublicKey {
    PublicKey::from_openssh(&fs::read_to_string(testdata(&format!("{name}.pub"))).expect("read"))
        .expect("public key")
}

pub struct ServerOptions {
    /// Name of the host key fixture.
    pub host_key: &'static str,
    /// Names of the client key fixtures allowed to log in.
    pub authorized: Vec<&'static str>,
    pub root: PathBuf,
    pub sftp: bool,
}

pub struct TestServer {
    pub port: u16,
    sessions: Arc<Mutex<Vec<server::Handle>>>,
    accept: JoinHandle<()>,
}

impl TestServer {
    pub async fn start(options: ServerOptions) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let config = Arc::new(server::Config {
            keys: vec![private_key(options.host_key)],
            auth_rejection_time: Duration::from_millis(10),
            auth_rejection_time_initial: Some(Duration::ZERO),
            inactivity_timeout: Some(Duration::from_secs(60)),
            ..server::Config::default()
        });
        let authorized: Arc<Vec<PublicKey>> = Arc::new(
            options
                .authorized
                .iter()
                .map(|name| public_key(name))
                .collect(),
        );
        let root = Arc::new(options.root);
        let sessions = Arc::new(Mutex::new(Vec::new()));
        let accept = {
            let sessions = Arc::clone(&sessions);
            tokio::spawn(async move {
                while let Ok((socket, _)) = listener.accept().await {
                    let handler = SshHandler {
                        authorized: Arc::clone(&authorized),
                        root: Arc::clone(&root),
                        sftp: options.sftp,
                        channels: HashMap::new(),
                    };
                    let config = Arc::clone(&config);
                    let sessions = Arc::clone(&sessions);
                    tokio::spawn(async move {
                        if let Ok(running) = server::run_stream(config, socket, handler).await {
                            sessions
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .push(running.handle());
                            let _ = running.await;
                        }
                    });
                }
            })
        };
        Self {
            port,
            sessions,
            accept,
        }
    }

    /// Breaks every open connection, as a server crash or network loss would.
    pub async fn disconnect_all(&self) {
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect();
        for session in sessions {
            let _ = session
                .disconnect(
                    russh::Disconnect::ByApplication,
                    "test".to_owned(),
                    "en".to_owned(),
                )
                .await;
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.accept.abort();
    }
}

/// A port that accepts connections and never answers.
pub async fn silent_server() -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    let task = tokio::spawn(async move {
        let mut sockets = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            sockets.push(socket);
        }
    });
    (port, task)
}

struct SshHandler {
    authorized: Arc<Vec<PublicKey>>,
    root: Arc<PathBuf>,
    sftp: bool,
    channels: HashMap<ChannelId, Channel<Msg>>,
}

impl SshHandler {
    fn check(&self, key: &PublicKey) -> Auth {
        if self
            .authorized
            .iter()
            .any(|authorized| authorized.key_data() == key.key_data())
        {
            Auth::Accept
        } else {
            Auth::reject()
        }
    }
}

impl server::Handler for SshHandler {
    type Error = russh::Error;

    async fn auth_publickey_offered(
        &mut self,
        _user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(self.check(public_key))
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(self.check(public_key))
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        let _ = reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        match self.channels.remove(&channel_id) {
            Some(channel) if self.sftp && name == "sftp" => {
                session.channel_success(channel_id)?;
                russh_sftp::server::run(
                    channel.into_stream(),
                    SftpHandler {
                        root: self.root.as_ref().clone(),
                        handles: HashMap::new(),
                        next: 0,
                    },
                )
                .await;
            }
            _ => {
                session.channel_failure(channel_id)?;
                session.close(channel_id)?;
            }
        }
        Ok(())
    }
}

enum Open {
    File(fs::File),
    Dir(Option<Vec<File>>),
}

struct SftpHandler {
    root: PathBuf,
    handles: HashMap<String, Open>,
    next: u64,
}

fn status(error: &io::Error) -> StatusCode {
    match error.kind() {
        io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_owned(),
        language_tag: "en-US".to_owned(),
    }
}

impl SftpHandler {
    fn local(&self, path: &str) -> Result<PathBuf, StatusCode> {
        let mut local = self.root.clone();
        for segment in path.split('/').filter(|s| !s.is_empty() && *s != ".") {
            if segment == ".." {
                return Err(StatusCode::PermissionDenied);
            }
            if segment.starts_with("forbidden") {
                return Err(StatusCode::PermissionDenied);
            }
            local.push(segment);
        }
        Ok(local)
    }

    fn handle(&mut self, open: Open) -> String {
        self.next += 1;
        let handle = self.next.to_string();
        self.handles.insert(handle.clone(), open);
        handle
    }
}

impl russh_sftp::server::Handler for SftpHandler {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let normalized: Vec<_> = path
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".")
            .collect();
        Ok(Name {
            id,
            files: vec![File::dummy(format!("/{}", normalized.join("/")))],
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let metadata = fs::metadata(self.local(&path)?).map_err(|error| status(&error))?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&metadata),
        })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        self.stat(id, path).await
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let mut entries = vec![
            File::new(".", FileAttributes::empty()),
            File::new("..", FileAttributes::empty()),
        ];
        for entry in fs::read_dir(self.local(&path)?).map_err(|error| status(&error))? {
            let entry = entry.map_err(|error| status(&error))?;
            let metadata = entry.metadata().map_err(|error| status(&error))?;
            entries.push(File::new(
                entry.file_name().to_string_lossy(),
                FileAttributes::from(&metadata),
            ));
        }
        let handle = self.handle(Open::Dir(Some(entries)));
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        match self.handles.get_mut(&handle) {
            Some(Open::Dir(entries)) => match entries.take() {
                Some(files) => Ok(Name { id, files }),
                None => Err(StatusCode::Eof),
            },
            _ => Err(StatusCode::Failure),
        }
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        if pflags.intersects(
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::APPEND,
        ) {
            return Err(StatusCode::PermissionDenied);
        }
        let file = fs::File::open(self.local(&filename)?).map_err(|error| status(&error))?;
        let handle = self.handle(Open::File(file));
        Ok(Handle { id, handle })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let Some(Open::File(file)) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| status(&error))?;
        let mut data = vec![0; len.min(64 * 1024) as usize];
        let read = file.read(&mut data).map_err(|error| status(&error))?;
        if read == 0 {
            return Err(StatusCode::Eof);
        }
        data.truncate(read);
        Ok(Data { id, data })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        let Some(Open::File(file)) = self.handles.get(&handle) else {
            return Err(StatusCode::Failure);
        };
        let metadata = file.metadata().map_err(|error| status(&error))?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&metadata),
        })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.handles.remove(&handle);
        Ok(ok(id))
    }
}

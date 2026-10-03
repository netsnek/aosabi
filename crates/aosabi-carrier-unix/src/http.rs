// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-unix: the listener of HTTP(Unix Socket).
 *
 * No analogue on the reference side.
 */

//! `driver`, `serve_unix` and `UnixListen`.
//!
//! The socket is a stream socket named `<name>.http`, where `<name>` is the
//! package name of the application crate, in the directory `XDG_RUNTIME_DIR`
//! names. The listener refuses to start when that variable is unset, when the
//! directory is not owned by the user of the process or has another mode than
//! 0700, and when a live process serves the socket already. The user is read
//! as the owner of the socket file the process just created, because the
//! standard library has no safe call for the user id and
//! `std::os::unix::fs::MetadataExt::uid` reads the owner of a file. A socket
//! file whose process is gone refuses a connect and is removed before the
//! bind, the file takes the mode 0600, and the listener removes it when it
//! drops. A process that a signal ends leaves its file, which the next start
//! removes as a file of a dead process.
//!
//! On Linux the kernel refuses a connect from a process that has no write
//! permission on the socket file or no search permission on the directory, so
//! the directory of mode 0700 and the file of mode 0600 admit the owning user
//! and a process with the capability to override the permissions of a file. A
//! server in a user namespace of its own reads no credential either, one that
//! maps the owner among them, since the kernel gives every peer the namespace
//! does not map, root of the host among them, the overflow uid, which the owner
//! of the socket file takes as well where the namespace maps no uid. A build
//! without the feature `linux` of `aosabi-serve`, and a development host on
//! macOS, reads no credential, and the file system is the boundary alone.
//!
//! Every socket that passes is served by `serve_stream` as the Unix variant
//! of the stream type of `aosabi-serve` with the scheme `http`, so HTTP(Unix
//! Socket) answers exactly what plain HTTP(TCP) answers, through the same
//! connection code.

use core::time::Duration;
use std::{
    env,
    fmt,
    fs::{
        self,
        Permissions, //
    },
    io::{
        self,
        ErrorKind, //
    },
    os::unix::{
        fs::{
            MetadataExt as _,
            PermissionsExt as _, //
        },
        net::UnixStream as StdUnixStream,
    },
    path::{
        Path,
        PathBuf, //
    },
    rc::Rc, //
};

use aosabi_core::{
    BootedApp,
    Listen, //
};
use aosabi_serve::{
    BuildOutput,
    Stream,
    runtime::{
        LocalBoxFuture,
        hyper_rt::{
            HyperIo,
            RuntimeTimer, //
        },
        net::{
            UnixListener,
            UnixStream, //
        },
        spawn_local,
    },
    serve_stream, //
};

/// The variable that switches HTTP(Unix Socket) on when it reads `1`.
pub const SWITCH_ENV: &str = "AOSABI_UNIX_SOCKET";

/// The variable that names the runtime directory of the user.
pub const RUNTIME_DIR_ENV: &str = "XDG_RUNTIME_DIR";

/// The suffix of the name of the socket file.
pub const SOCKET_SUFFIX: &str = ".http";

/// The mode the runtime directory must have.
pub const DIR_MODE: u32 = 0o700;

/// The mode the socket file takes.
pub const SOCKET_MODE: u32 = 0o600;

/// How long the loop waits after an accept that failed.
const ACCEPT_RETRY: Duration = Duration::from_millis(10);

/// Why HTTP(Unix Socket) did not start.
#[derive(Debug)]
pub enum UnixSocketError {
    /// `XDG_RUNTIME_DIR` is not set.
    NoRuntimeDir,
    /// The runtime directory is owned by another user.
    DirOwner {
        /// The directory.
        dir: PathBuf,
        /// Its owner.
        owner: u32,
        /// The user of the process.
        user: u32,
    },
    /// The runtime directory has another mode than 0700.
    DirMode {
        /// The directory.
        dir: PathBuf,
        /// Its mode.
        mode: u32,
    },
    /// A live process serves the socket already.
    InUse(PathBuf),
    /// A call of the file system or of the socket failed.
    Io(PathBuf, io::Error),
}

impl fmt::Display for UnixSocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRuntimeDir => write!(
                f,
                "HTTP(Unix Socket) needs {RUNTIME_DIR_ENV}, which is not set"
            ),
            Self::DirOwner { dir, owner, user } => write!(
                f,
                "HTTP(Unix Socket) refuses {}, which user {owner} owns and not user {user}",
                dir.display()
            ),
            Self::DirMode { dir, mode } => write!(
                f,
                "HTTP(Unix Socket) refuses {}, whose mode is {mode:04o} and not 0700",
                dir.display()
            ),
            Self::InUse(path) => write!(
                f,
                "HTTP(Unix Socket) refuses {}, which a live process serves",
                path.display()
            ),
            Self::Io(path, error) => write!(f, "HTTP(Unix Socket) at {}: {error}", path.display()),
        }
    }
}

impl core::error::Error for UnixSocketError {}

impl From<UnixSocketError> for io::Error {
    fn from(error: UnixSocketError) -> Self {
        let kind = match &error {
            UnixSocketError::Io(_, inner) => inner.kind(),
            UnixSocketError::InUse(_) => ErrorKind::AddrInUse,
            UnixSocketError::DirOwner { .. } | UnixSocketError::DirMode { .. } => {
                ErrorKind::PermissionDenied
            }
            UnixSocketError::NoRuntimeDir => ErrorKind::NotFound,
        };
        Self::new(kind, error)
    }
}

/// The driver of HTTP(Unix Socket) the facade hands the boot, a listener
/// under `AOSABI_UNIX_SOCKET=1` and none otherwise.
///
/// # Errors
///
/// Returns why the listener did not start, which the boot prints before it
/// exits 70.
pub fn driver(build: &BuildOutput) -> io::Result<Option<Box<dyn Listen>>> {
    if env::var_os(SWITCH_ENV).is_none_or(|value| value != "1") {
        return Ok(None);
    }
    let dir = env::var_os(RUNTIME_DIR_ENV)
        .filter(|dir| !dir.is_empty())
        .ok_or(UnixSocketError::NoRuntimeDir)?;
    let listener = UnixListen::bind(Path::new(&dir), build.name)?;
    Ok(Some(Box::new(listener)))
}

/// Binds HTTP(Unix Socket) as `<name>.http` in `dir` and serves `app` on it
/// until an accept fails for good, which never happens.
///
/// # Errors
///
/// Returns why the listener did not start.
pub async fn serve_unix(dir: &Path, name: &str, app: Rc<BootedApp>) -> io::Result<()> {
    let listener = UnixListen::bind(dir, name)?;
    Box::new(listener).run(app).await;
    Ok(())
}

/// The listener of HTTP(Unix Socket), bound and checked.
pub struct UnixListen {
    /// The bound listener.
    listener: UnixListener,
    /// The socket file, removed on drop.
    path: PathBuf,
    /// The user of the process, the owner of the socket file.
    user: u32,
}

impl UnixListen {
    /// Binds `<name>.http` in `dir` and checks the directory.
    ///
    /// # Errors
    ///
    /// Returns [`UnixSocketError`] as an IO error.
    pub fn bind(dir: &Path, name: &str) -> Result<Self, UnixSocketError> {
        let path = dir.join(format!("{name}{SOCKET_SUFFIX}"));
        remove_stale(&path)?;
        let listener =
            UnixListener::bind(&path).map_err(|error| UnixSocketError::Io(path.clone(), error))?;
        // From here the socket file is ours, so a refusal removes it.
        let listen = Self {
            listener,
            path,
            user: 0,
        };
        listen.checked()
    }

    /// Reads the user as the owner of the new socket file, checks the
    /// directory and sets the mode of the file.
    fn checked(mut self) -> Result<Self, UnixSocketError> {
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |error| UnixSocketError::Io(path, error)
        };
        self.user = fs::metadata(&self.path).map_err(io(&self.path))?.uid();
        let dir = self.path.parent().unwrap_or(Path::new("/")).to_path_buf();
        let meta = fs::metadata(&dir).map_err(io(&dir))?;
        if meta.uid() != self.user {
            return Err(UnixSocketError::DirOwner {
                dir,
                owner: meta.uid(),
                user: self.user,
            });
        }
        let mode = meta.mode() & 0o7777;
        if mode != DIR_MODE {
            return Err(UnixSocketError::DirMode { dir, mode });
        }
        fs::set_permissions(&self.path, Permissions::from_mode(SOCKET_MODE))
            .map_err(io(&self.path))?;
        Ok(self)
    }

    /// Returns the socket file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Tells whether the peer of `stream` may be served: its uid is the owner
    /// of the socket file, or the server reads no credential, as a build
    /// without `linux`, a server in a PID namespace or a user namespace of its
    /// own and a server whose `/proc` hides the links of its namespaces, of,
    /// do.
    fn admits(&self, stream: &UnixStream) -> bool {
        match stream.peer_uid() {
            Ok(Some(uid)) => uid == self.user,
            Ok(None) => true,
            Err(_) => false,
        }
    }
}

impl Drop for UnixListen {
    fn drop(&mut self) {
        // The file may be gone already, which leaves nothing to remove.
        let _ = fs::remove_file(&self.path);
    }
}

impl Listen for UnixListen {
    fn run(self: Box<Self>, app: Rc<BootedApp>) -> LocalBoxFuture<'static, ()> {
        Box::pin(async move {
            loop {
                let stream = match self.listener.accept().await {
                    Ok(stream) => stream,
                    Err(error) => {
                        eprintln!("aosabi-carrier-unix: accept failed: {error}");
                        if let Ok(timer) = RuntimeTimer::current() {
                            timer.sleep_for(ACCEPT_RETRY).await;
                        }
                        continue;
                    }
                };
                if !self.admits(&stream) {
                    // Dropping the socket closes it with no byte.
                    drop(stream);
                    continue;
                }
                spawn_local(serve_stream(
                    Stream::Unix(Box::new(HyperIo::new(stream))),
                    Rc::clone(&app),
                    false,
                ));
            }
        })
    }
}

/// Removes the socket file at `path` when no process serves it, and refuses
/// when one does.
fn remove_stale(path: &Path) -> Result<(), UnixSocketError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(UnixSocketError::Io(path.to_path_buf(), error)),
        Ok(_) => {}
    }
    match StdUnixStream::connect(path) {
        Ok(_) => Err(UnixSocketError::InUse(path.to_path_buf())),
        Err(error) if error.kind() == ErrorKind::ConnectionRefused => {
            fs::remove_file(path).map_err(|error| UnixSocketError::Io(path.to_path_buf(), error))
        }
        Err(error) => Err(UnixSocketError::Io(path.to_path_buf(), error)),
    }
}

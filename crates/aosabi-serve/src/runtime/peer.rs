// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the uid of the peer of a Unix socket.
 *
 * No analogue on the reference side.
 */

//! `UnixStream::peer_uid`.
//!
//! The standard library has no stable way to read the credential of a peer at
//! the pinned toolchain, since `UnixStream::peer_cred` stays behind the
//! unstable feature `peer_credentials_unix_socket`, which the compile cell
//! `c6-unix-peer-cred` of `12_authoring_parity` records as refused with E0658.
//! So the Linux cards read `SO_PEERCRED` through the safe
//! `rustix::net::sockopt::socket_peercred` where the process runs in the
//! initial PID namespace and the initial user namespace, which on a kernel
//! built without such namespaces, as that of the player, is every process.

use std::io;
#[cfg(all(feature = "linux", target_os = "linux"))]
use std::{
    ffi::OsStr,
    fs,
    sync::OnceLock, //
};

use super::net::UnixStream;

/// The link `/proc/self/ns/pid` of a process in the initial PID namespace.
///
/// The kernel gives the initial PID namespace the fixed inode number
/// `PID_NS_INIT_INO`, `0xEFFFFFFC`, which is 4026531836.
///
/// [`nsfs.h`]: https://github.com/torvalds/linux/blob/v6.19/include/uapi/linux/nsfs.h
#[cfg(all(feature = "linux", target_os = "linux"))]
const INITIAL_PID_NAMESPACE: &str = "pid:[4026531836]";

/// The link `/proc/self/ns/user` of a process in the initial user namespace.
///
/// The kernel gives the initial user namespace the fixed inode number
/// `USER_NS_INIT_INO`, `0xEFFFFFFD`, which is 4026531837.
///
/// [`nsfs.h`]: https://github.com/torvalds/linux/blob/v6.19/include/uapi/linux/nsfs.h
#[cfg(all(feature = "linux", target_os = "linux"))]
const INITIAL_USER_NAMESPACE: &str = "user:[4026531837]";

impl UnixStream {
    /// Returns the uid of the peer process where the kernel names every peer by
    /// its pid and its uid.
    ///
    /// Under the feature `linux` on a Linux target the uid is read as
    /// `SO_PEERCRED` when the process runs in the initial PID namespace and the
    /// initial user namespace, whose links `/proc/self/ns/pid` and
    /// `/proc/self/ns/user` read `pid:[4026531836]` and `user:[4026531837]`, or
    /// where the kernel lists no link of such a namespace, and the answer is
    /// `Ok(Some(uid))`. A process in a PID namespace of its own, as in a
    /// container, or in a user namespace of its own answers `Ok(None)` with no
    /// system call after the first, as do a process whose `/proc` hides the
    /// links and every build without `linux`, so the file modes of the socket
    /// are the boundary there.
    ///
    /// Every process is visible in each ancestor PID namespace up to the
    /// initial one, so the kernel names the pid of every peer there. In a user
    /// namespace of its own the kernel gives every peer the namespace does not
    /// map, root of the host among them, the overflow uid, which the owner of
    /// the socket file takes as well where the namespace maps no uid, so a uid
    /// read there cannot tell root from the owner. The links are read once per
    /// process, at the first call.
    ///
    /// # Errors
    ///
    /// Returns the error of `getsockopt`.
    pub fn peer_uid(&self) -> io::Result<Option<u32>> {
        #[cfg(all(feature = "linux", target_os = "linux"))]
        {
            if !in_initial_namespaces() {
                return Ok(None);
            }
            let credential = rustix::net::sockopt::socket_peercred(self.as_std())?;
            Ok(Some(credential.uid.as_raw()))
        }
        #[cfg(not(all(feature = "linux", target_os = "linux")))]
        {
            Ok(None)
        }
    }
}

/// Tells whether the process runs in the initial PID namespace and the initial
/// user namespace, where the kernel names every peer by its pid and its uid.
///
/// The first call reads `/proc/self/ns/pid` through [`names_initial`], and
/// `/proc/self/ns/user` where that read answers the initial PID namespace, a
/// kernel without PID namespaces included.
// An `unsafe` `getsockopt` would skip both reads of the namespaces.
#[cfg(all(feature = "linux", target_os = "linux"))]
fn in_initial_namespaces() -> bool {
    static INITIAL: OnceLock<bool> = OnceLock::new();
    *INITIAL.get_or_init(|| {
        names_initial("/proc/self/ns/pid", INITIAL_PID_NAMESPACE)
            && names_initial("/proc/self/ns/user", INITIAL_USER_NAMESPACE)
    })
}

/// Tells whether the namespace link at `path` names the initial namespace of
/// its kind, whose link reads `initial`.
///
/// A link the kernel does not list, a read that fails with `NotFound`, counts
/// as a kernel built without that kind of namespace, whose processes all run in
/// the initial one, where the link `/proc/self/ns/mnt` reads, since the kernel
/// lists that link whatever its configuration and the links `pid` and `user`
/// only under `CONFIG_PID_NS` and `CONFIG_USER_NS`.
#[cfg(all(feature = "linux", target_os = "linux"))]
fn names_initial(path: &str, initial: &str) -> bool {
    match fs::read_link(path) {
        Ok(link) => link.as_os_str() == OsStr::new(initial),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::read_link("/proc/self/ns/mnt").is_ok()
        }
        Err(_) => false,
    }
}

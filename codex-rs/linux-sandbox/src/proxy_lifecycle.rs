//! Manage privileged bridge processes and transfer their loopback TCP listeners.
//!
//! Received descriptors are CLOEXEC and owned even when validation rejects them.
//! Shape validation does not authenticate the sender: the caller must retain exclusive
//! control of the anonymous channel and close bootstrap descriptors before untrusted execution.

use rustix::io::retry_on_intr;
use rustix::net::RecvAncillaryBuffer;
use rustix::net::RecvAncillaryMessage;
use rustix::net::RecvFlags;
use rustix::net::ReturnFlags;
use rustix::net::SendAncillaryBuffer;
use rustix::net::SendAncillaryMessage;
use rustix::net::SendFlags;
use rustix::net::SocketType;
use rustix::net::ipproto;
use rustix::net::sockopt;
use std::io;
use std::io::IoSlice;
use std::io::IoSliceMut;
use std::mem::MaybeUninit;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::os::fd::AsFd;
use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;

const LISTENER_MESSAGE: u8 = 1;

pub(crate) fn send_listener(channel: &UnixStream, listener: &TcpListener) -> io::Result<()> {
    let descriptors = [listener.as_fd()];
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    if !control.push(SendAncillaryMessage::ScmRights(&descriptors)) {
        return Err(io::Error::other("missing proxy listener control header"));
    }
    // rustix surfaces EINTR on both its libc and raw-syscall backends.
    // Retry interrupted sends so a handled signal does not abort the handoff.
    let sent = retry_on_intr(|| {
        rustix::net::sendmsg(
            channel,
            &[IoSlice::new(&[LISTENER_MESSAGE])],
            &mut control,
            SendFlags::NOSIGNAL,
        )
    })?;
    if sent != 1 {
        return Err(io::Error::other("failed to transfer proxy listener"));
    }
    Ok(())
}

pub(crate) fn receive_listener(channel: &UnixStream) -> io::Result<TcpListener> {
    let mut byte = [0_u8];
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    // rustix surfaces EINTR on both its libc and raw-syscall backends.
    // Retry interrupted receives so a handled signal does not abort the handoff.
    let message = retry_on_intr(|| {
        rustix::net::recvmsg(
            channel,
            &mut [IoSliceMut::new(&mut byte)],
            &mut control,
            RecvFlags::CMSG_CLOEXEC,
        )
    })?;

    // Alignment headroom can fit extra descriptors. Own all of them before checking
    // the count; rustix also closes any descriptors left undrained on error paths.
    let mut descriptors = Vec::new();
    let mut unexpected_message = false;
    for ancillary in control.drain() {
        match ancillary {
            RecvAncillaryMessage::ScmRights(received) => descriptors.extend(received),
            _ => unexpected_message = true,
        }
    }
    if message.bytes != 1
        || byte != [LISTENER_MESSAGE]
        || message
            .flags
            .intersects(ReturnFlags::CTRUNC | ReturnFlags::TRUNC)
        || unexpected_message
        || descriptors.len() != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid proxy listener handoff",
        ));
    }

    let listener = TcpListener::from(descriptors.remove(/*index*/ 0));
    if sockopt::socket_type(&listener)? != SocketType::STREAM
        || sockopt::socket_protocol(&listener)? != Some(ipproto::TCP)
        || !sockopt::socket_acceptconn(&listener)?
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "proxy handoff requires a TCP listener",
        ));
    }
    if !matches!(listener.local_addr()?, SocketAddr::V4(address)
        if *address.ip() == Ipv4Addr::LOCALHOST && address.port() != 0)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "proxy listener must be bound to IPv4 loopback",
        ));
    }
    Ok(listener)
}

pub(crate) fn harden_bridge_process(expected_parent_pid: libc::pid_t) -> io::Result<()> {
    detach_bridge_stdio()?;
    set_parent_death_signal(expected_parent_pid)?;
    codex_process_hardening::disable_process_dumping()
}

fn set_parent_death_signal(expected_parent_pid: libc::pid_t) -> io::Result<()> {
    let res = unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) };
    if res != 0 {
        Err(io::Error::last_os_error())
    } else if unsafe { libc::getppid() } != expected_parent_pid {
        Err(io::Error::other("parent process already exited"))
    } else {
        Ok(())
    }
}

fn detach_bridge_stdio() -> io::Result<()> {
    let null_read_fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY) };
    if null_read_fd < 0 {
        let err = io::Error::last_os_error();
        if unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFD) } < 0 {
            return Err(err);
        }
        return redirect_bridge_output(libc::STDIN_FILENO);
    }

    let null_write_fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY) };
    if null_write_fd < 0 {
        if unsafe { libc::dup2(null_read_fd, libc::STDIN_FILENO) } < 0 {
            let err = io::Error::last_os_error();
            if null_read_fd > libc::STDERR_FILENO {
                let _ = close_fd(null_read_fd);
            }
            return Err(err);
        }
        let result = redirect_bridge_output(null_read_fd);
        if null_read_fd > libc::STDERR_FILENO {
            let _ = close_fd(null_read_fd);
        }
        return result;
    }

    for (source_fd, stream_fd) in [
        (null_read_fd, libc::STDIN_FILENO),
        (null_write_fd, libc::STDOUT_FILENO),
        (null_write_fd, libc::STDERR_FILENO),
    ] {
        if unsafe { libc::dup2(source_fd, stream_fd) } < 0 {
            let err = io::Error::last_os_error();
            if null_read_fd > libc::STDERR_FILENO {
                let _ = close_fd(null_read_fd);
            }
            if null_write_fd > libc::STDERR_FILENO {
                let _ = close_fd(null_write_fd);
            }
            return Err(err);
        }
    }

    if null_read_fd > libc::STDERR_FILENO {
        close_fd(null_read_fd)?;
    }
    if null_write_fd > libc::STDERR_FILENO {
        close_fd(null_write_fd)?;
    }

    Ok(())
}

fn redirect_bridge_output(source_fd: libc::c_int) -> io::Result<()> {
    for stream_fd in [libc::STDOUT_FILENO, libc::STDERR_FILENO] {
        if unsafe { libc::dup2(source_fd, stream_fd) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub(crate) fn move_fd_above_stdio(fd: OwnedFd) -> io::Result<OwnedFd> {
    if fd.as_raw_fd() > libc::STDERR_FILENO {
        return Ok(fd);
    }

    // SAFETY: F_DUPFD_CLOEXEC takes an integer lower bound, not a pointer;
    // `fd` owns the live descriptor throughout the call.
    let relocated_fd = unsafe {
        libc::fcntl(
            fd.as_raw_fd(),
            libc::F_DUPFD_CLOEXEC,
            libc::STDERR_FILENO + 1,
        )
    };
    if relocated_fd < 0 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: F_DUPFD_CLOEXEC returned a new descriptor; the original owner drops here.
    Ok(unsafe { OwnedFd::from_raw_fd(relocated_fd) })
}

pub(crate) fn close_fd(fd: libc::c_int) -> io::Result<()> {
    let res = unsafe { libc::close(fd) };
    if res < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
#[path = "proxy_lifecycle_tests.rs"]
mod tests;

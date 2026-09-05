//! Verify listener ownership, handoff validation, and descriptor cleanup on rejection.

use super::LISTENER_MESSAGE;
use super::receive_listener;
use super::send_listener;
use pretty_assertions::assert_eq;
use rustix::io::FdFlags;
use rustix::io::fcntl_getfd;
use rustix::net::SendAncillaryBuffer;
use rustix::net::SendAncillaryMessage;
use rustix::net::SendFlags;
use std::io;
use std::io::IoSlice;
use std::io::Read;
use std::io::Write;
use std::mem::MaybeUninit;
use std::net::Ipv4Addr;
use std::net::TcpListener;
use std::net::TcpStream;
use std::os::fd::AsFd;
use std::os::fd::BorrowedFd;
use std::os::unix::net::UnixStream;
use std::time::Duration;
use test_case::test_case;

#[test]
fn transferred_listener_survives_sender_close_and_is_cloexec() -> io::Result<()> {
    let (sender, receiver) = UnixStream::pair()?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    send_listener(&sender, &listener)?;
    drop(listener);
    drop(sender);

    let listener = receive_listener(&receiver)?;
    let flags = fcntl_getfd(&listener)?;
    assert_eq!(
        (listener.local_addr()?, flags & FdFlags::CLOEXEC),
        (address, FdFlags::CLOEXEC)
    );
    let mut client = TcpStream::connect(address)?;
    client.write_all(b"proxy")?;
    let (mut accepted, _) = listener.accept()?;
    let mut bytes = [0; 5];
    accepted.read_exact(&mut bytes)?;
    assert_eq!(bytes, *b"proxy");
    Ok(())
}

#[test_case(1; "non_listener")]
#[test_case(2; "extra_descriptors")]
// Eight descriptors exceed the receive buffer even with alignment headroom.
#[test_case(8; "truncated_control_message")]
fn malformed_handoff_closes_all_received_descriptors(count: usize) -> io::Result<()> {
    let (sender, receiver) = UnixStream::pair()?;
    let mut peers = Vec::new();
    let mut passed = Vec::new();
    for _ in 0..count {
        let (peer, descriptor) = UnixStream::pair()?;
        peer.set_read_timeout(Some(Duration::from_secs(/*secs*/ 1)))?;
        peers.push(peer);
        passed.push(descriptor);
    }
    let descriptors = passed.iter().map(AsFd::as_fd).collect::<Vec<_>>();
    send_descriptors(&sender, LISTENER_MESSAGE, &descriptors)?;
    drop(passed);
    assert!(receive_listener(&receiver).is_err());
    for mut peer in peers {
        assert_eq!(peer.read(&mut [0])?, 0);
    }
    Ok(())
}

#[test]
fn rejects_missing_descriptors_and_invalid_payloads() -> io::Result<()> {
    let (mut sender, receiver) = UnixStream::pair()?;
    sender.write_all(&[LISTENER_MESSAGE])?;
    assert!(receive_listener(&receiver).is_err());

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    send_descriptors(&sender, /*payload*/ 0, &[listener.as_fd()])?;
    assert!(receive_listener(&receiver).is_err());
    Ok(())
}

#[test_case(2; "extra_descriptors")]
#[test_case(8; "truncated_control_message")]
fn rejects_extra_descriptors_even_when_first_is_a_listener(count: usize) -> io::Result<()> {
    let (sender, receiver) = UnixStream::pair()?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    send_descriptors(&sender, LISTENER_MESSAGE, &vec![listener.as_fd(); count])?;
    assert!(receive_listener(&receiver).is_err());
    Ok(())
}

#[test]
fn rejects_listener_bound_to_all_interfaces() -> io::Result<()> {
    let (sender, receiver) = UnixStream::pair()?;
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    send_listener(&sender, &listener)?;
    assert!(receive_listener(&receiver).is_err());
    Ok(())
}

fn send_descriptors(
    channel: &UnixStream,
    payload: u8,
    descriptors: &[BorrowedFd<'_>],
) -> io::Result<()> {
    let message = SendAncillaryMessage::ScmRights(descriptors);
    let mut space = vec![MaybeUninit::uninit(); message.size()];
    let mut control = SendAncillaryBuffer::new(&mut space);
    assert!(control.push(message));
    let sent = rustix::io::retry_on_intr(|| {
        rustix::net::sendmsg(
            channel,
            &[IoSlice::new(&[payload])],
            &mut control,
            SendFlags::NOSIGNAL,
        )
    })?;
    assert_eq!(sent, 1);
    Ok(())
}

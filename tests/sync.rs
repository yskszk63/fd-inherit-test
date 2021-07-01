use std::io;
use std::os::unix::io::RawFd;
use std::os::raw::c_int;
use std::process::Command;
use std::os::unix::process::CommandExt;

fn set_cloexc(fd: RawFd) -> io::Result<()> {
    let flags = unsafe {
        libc::fcntl(fd, libc::F_GETFD)
    };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    let r = unsafe {
        libc::fcntl(fd, libc::F_SETFL, flags | libc::FD_CLOEXEC)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::fcntl(fd, libc::F_GETFL)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }

    let r = unsafe {
        libc::fcntl(fd, libc::F_SETFL, r | libc::O_NONBLOCK)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

fn pipe() -> io::Result<(c_int, c_int)> {
    let mut pair = [0; 2];
    let r = unsafe {
        libc::pipe(pair.as_mut_ptr()) // no CLOEXEC (pipe2 does not exist on mac)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    let [r, w] = pair;

    for fd in pair {
        set_cloexc(fd)?;
        set_nonblocking(fd)?;
    }

    Ok((r, w))
}

fn dup(fd: RawFd) -> io::Result<RawFd> {
    let r = unsafe {
        libc::dup(fd)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(r)
}

fn dup2(fd: RawFd, newfd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::dup2(fd, newfd)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn close(fd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::close(fd)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn write_all(fd: RawFd, buf: &[u8]) -> io::Result<()> {
    let mut n = buf.len();
    while n > 0 {
        let r = unsafe {
            libc::write(fd, buf.as_ptr() as *const _, n)
        };
        if r == -1 {
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::WouldBlock {
                return Err(err);
            }
            continue;
        }
        n -= r as usize;
    }
    Ok(())
}

fn read_to_end(fd: RawFd, buf: &mut Vec<u8>) -> io::Result<()> {
    let mut b = [0; libc::BUFSIZ as usize];
    loop {
        let r = unsafe {
            libc::read(fd, b.as_mut_ptr() as *mut _, b.len())
        };
        if r == -1 {
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::WouldBlock {
                return Err(err);
            }
            continue;
        }
        if r == 0 {
            return Ok(())
        }
        buf.extend_from_slice(&b[..r as usize]);
    }
}

#[test]
fn test_synchronous() -> io::Result<()> {
    let script = r#"#!/usr/bin/env python3
import os, shutil
with os.fdopen(3, 'rb') as r:
    with os.fdopen(4, 'wb') as w:
        shutil.copyfileobj(r, w)
"#;

    let (input, their_input) = pipe()?;
    let (their_output, output) = pipe()?;

    let mut command = Command::new("python3");
    command.args(["-c", script]);
    let mut child = unsafe {
        command.pre_exec(move || {
            let no_cloexec_input = dup(input)?;
            let no_cloexec_output = dup(output)?;

            dup2(no_cloexec_input, 3)?;
            dup2(no_cloexec_output, 4)?;

            close(no_cloexec_input)?;
            close(no_cloexec_output)?;

            Ok(())
        }).spawn()?
    };
    close(input)?;
    close(output)?;

    write_all(their_input, &b"Hello, World!"[..])?;
    close(their_input)?;

    let mut buf = vec![];
    read_to_end(their_output, &mut buf)?;

    assert_eq!(&b"Hello, World!"[..], &buf);

    assert!(child.wait()?.success());
    Ok(())
}

use std::io;
use std::os::raw::c_int;
use std::os::unix::io::RawFd;
use std::os::unix::prelude::CommandExt;
use std::process::Command;

use tokio::io::unix::AsyncFd;

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

    for fd in pair {
        set_cloexc(fd)?;
        set_nonblocking(fd)?;
    }

    let [r, w] = pair;
    Ok((r, w))
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

#[tokio::test]
async fn test_registered_fd_mai_not_be_inherit() -> io::Result<()> {
    let (r, w) = pipe()?;

    let r = AsyncFd::new(r)?;

    let script = r#"#!/usr/bin/env python3
import os
print(os.stat(3))
"#;
    let mut command = Command::new("python3");
    command.args(["-c", script]);
    let mut child = unsafe {
        let r = *r.get_ref();
        command.pre_exec(move || {
            let t = dup(r)?;
            dup2(t, 3)?;
            close(t)?;
            Ok(())
        });
        let result = command.spawn();
        drop(r);
        result
    }?;

    let ok = child.wait()?.success();
    assert!(ok);

    close(w)?;
    Ok(())
}
